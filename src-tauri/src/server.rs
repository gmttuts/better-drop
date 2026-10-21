//! Máy chủ HTTPS nhận yêu cầu và dữ liệu từ máy khác.

use crate::models::*;
use crate::paths::{sanitize_rel_path, unique_path};
use crate::state::{AppState, InFile, InSession, Tracker};
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);
pub const DEFAULT_PORT: u16 = 53318;

type St = State<Arc<AppState>>;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/info", get(info))
        .route(
            "/api/v1/prepare",
            post(prepare).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/api/v1/upload", post(upload).layer(DefaultBodyLimit::disable()))
        .route("/api/v1/cancel", post(cancel))
        .with_state(state)
}

/// Mở cổng lắng nghe: thử cổng mặc định trước (dễ nhập tay, dễ mở firewall),
/// nếu bận thì lấy cổng ngẫu nhiên.
pub fn bind() -> std::io::Result<std::net::TcpListener> {
    let preferred = if crate::identity::profile().is_some() { 0 } else { DEFAULT_PORT };
    let listener = std::net::TcpListener::bind(("0.0.0.0", preferred))
        .or_else(|_| std::net::TcpListener::bind(("0.0.0.0", 0)))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub async fn serve(state: Arc<AppState>, listener: std::net::TcpListener) -> Result<(), String> {
    let cfg = axum_server::tls_rustls::RustlsConfig::from_pem(
        state.identity.cert_pem.clone().into_bytes(),
        state.identity.key_pem.clone().into_bytes(),
    )
    .await
    .map_err(|e| e.to_string())?;
    axum_server::from_tcp_rustls(listener, cfg)
        .serve(router(state).into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| e.to_string())
}

async fn info(State(st): St) -> Json<DeviceInfo> {
    Json(st.device_info())
}

/// Xoá yêu cầu đang chờ khi handler kết thúc (kể cả khi bên gửi ngắt kết nối)
/// và báo UI đóng hộp thoại.
struct PendingGuard {
    st: Arc<AppState>,
    id: String,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.st.pending.lock().unwrap().remove(&self.id);
        self.st
            .emit("request-closed", serde_json::json!({ "request_id": self.id }));
    }
}

/// Ghi nhớ máy vừa gửi tới mình để có thể gửi ngược lại, kể cả khi mạng chặn
/// tự tìm máy (mDNS) — ví dụ điện thoại đã quét QR của máy tính.
fn remember_sender(st: &AppState, sender: &DeviceInfo, remote: SocketAddr) {
    if sender.id == st.device_info().id {
        return;
    }
    let addr = format!("{}:{}", remote.ip().to_canonical(), sender.port);
    let known = st
        .peers
        .lock()
        .unwrap()
        .values()
        .any(|p| p.info.id == sender.id && (!p.manual || (p.online && p.addrs.contains(&addr))));
    if !known {
        st.remember_peer(Peer {
            info: sender.clone(),
            addrs: vec![addr],
            manual: true,
            online: true,
        });
    }
}

async fn prepare(
    State(st): St,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(req): Json<PrepareRequest>,
) -> Result<Json<PrepareResponse>, StatusCode> {
    if !st.settings.read().unwrap().receive_enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    if req.kind == TransferKind::Text {
        remember_sender(&st, &req.sender, remote);
        receive_text(&st, &req.sender.name, req.text.unwrap_or_default());
        return Ok(Json(PrepareResponse::default()));
    }
    let resp = accept_files(&st, &req.sender.name, &req.sender.os, &req.files).await?;
    remember_sender(&st, &req.sender, remote);
    Ok(Json(resp))
}

/// Nhận một đoạn văn bản: hiện cho người dùng và ghi vào mục Hoạt động.
pub fn receive_text(st: &AppState, sender_name: &str, text: String) {
    st.emit(
        "incoming-text",
        IncomingText {
            sender_name: sender_name.to_string(),
            text: text.clone(),
        },
    );
    Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: uuid::Uuid::new_v4().to_string(),
            direction: Direction::Receive,
            peer_name: sender_name.to_string(),
            kind: TransferKind::Text,
            file_count: 0,
            total_bytes: text.len() as u64,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Done,
            error: None,
            dir_path: None,
        },
    );
}

/// Hỏi người dùng có nhận các file này không (chờ tối đa ACCEPT_TIMEOUT). Nếu đồng ý,
/// tạo phiên nhận và trả về token cho từng file. Dùng chung cho app và trang web.
pub async fn accept_files(
    st: &Arc<AppState>,
    sender_name: &str,
    sender_os: &str,
    req_files: &[FileMeta],
) -> Result<PrepareResponse, StatusCode> {
    if req_files.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let total_bytes: u64 = req_files.iter().map(|f| f.size).sum();

    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    st.pending.lock().unwrap().insert(request_id.clone(), tx);
    let guard = PendingGuard {
        st: st.clone(),
        id: request_id.clone(),
    };
    st.emit(
        "incoming-request",
        IncomingRequest {
            request_id: request_id.clone(),
            sender_name: sender_name.to_string(),
            sender_os: sender_os.to_string(),
            file_count: req_files.len(),
            total_bytes,
            preview: req_files.iter().take(5).map(|f| f.rel_path.clone()).collect(),
            timeout_secs: ACCEPT_TIMEOUT.as_secs(),
        },
    );

    let accepted = matches!(tokio::time::timeout(ACCEPT_TIMEOUT, rx).await, Ok(Ok(true)));
    drop(guard);
    if !accepted {
        return Err(StatusCode::FORBIDDEN);
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let base_dir = PathBuf::from(st.settings.read().unwrap().download_dir.clone());
    let mut tokens = HashMap::new();
    let mut files = HashMap::new();
    for f in req_files {
        let token = uuid::Uuid::new_v4().simple().to_string();
        tokens.insert(f.id.clone(), token.clone());
        files.insert(
            f.id.clone(),
            InFile {
                rel: sanitize_rel_path(&f.rel_path),
                size: f.size,
                token,
            },
        );
    }
    let tracker = Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: session_id.clone(),
            direction: Direction::Receive,
            peer_name: sender_name.to_string(),
            kind: TransferKind::Files,
            file_count: files.len(),
            total_bytes,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Waiting,
            error: None,
            dir_path: Some(base_dir.to_string_lossy().into_owned()),
        },
    );
    tracker.set_status(TransferStatus::Transferring, None);
    st.incoming.lock().unwrap().insert(
        session_id.clone(),
        Arc::new(InSession {
            tracker,
            files,
            base_dir,
            root_map: Default::default(),
            started_files: Default::default(),
            done: Default::default(),
        }),
    );
    Ok(PrepareResponse { session_id, tokens })
}

async fn upload(
    State(st): St,
    Query(q): Query<UploadQuery>,
    body: Body,
) -> Result<StatusCode, (StatusCode, String)> {
    receive_upload(&st, q, body).await
}

/// Nhận dữ liệu một file của phiên đã được chấp nhận (dùng chung cho app và trang web).
pub async fn receive_upload(
    st: &Arc<AppState>,
    q: UploadQuery,
    body: Body,
) -> Result<StatusCode, (StatusCode, String)> {
    let session = st
        .incoming
        .lock()
        .unwrap()
        .get(&q.session)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Không tìm thấy phiên".into()))?;
    let file = session
        .files
        .get(&q.file)
        .ok_or((StatusCode::NOT_FOUND, "Không tìm thấy file".into()))?;
    if file.token != q.token {
        return Err((StatusCode::FORBIDDEN, "Sai token".into()));
    }
    let tracker = &session.tracker;
    if tracker.is_cancelled() {
        return Err((StatusCode::GONE, "Máy nhận đã huỷ".into()));
    }
    if !session.started_files.lock().unwrap().insert(q.file.clone()) {
        return Err((StatusCode::CONFLICT, "File đã được gửi".into()));
    }

    let fail = |msg: String| {
        tracker.set_status(TransferStatus::Error, Some(msg.clone()));
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    };

    let dest = session
        .dest_path(&file.rel)
        .map_err(|e| fail(format!("Không tạo được thư mục: {e}")))?;
    let part = dest.with_file_name(format!(
        "{}.{}.part",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        &q.token[..8]
    ));

    let result = write_body(&part, body, file.size, tracker).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(match e {
            WriteError::Cancelled => (StatusCode::GONE, "Máy nhận đã huỷ".into()),
            WriteError::Disconnected => {
                // Khi bên gửi bấm Huỷ, kết nối đứt trước rồi lệnh /cancel mới tới:
                // chờ một chút để hiện đúng "Máy gửi đã huỷ" thay vì báo lỗi.
                tokio::time::sleep(Duration::from_millis(1500)).await;
                if !tracker.is_cancelled() {
                    tracker.set_status(
                        TransferStatus::Error,
                        Some("Mất kết nối với máy gửi".into()),
                    );
                }
                (StatusCode::BAD_REQUEST, "Mất kết nối".into())
            }
            WriteError::Io(msg) => fail(msg),
        });
    }

    {
        let _lock = st.rename_lock.lock().unwrap();
        let final_path = unique_path(&dest);
        std::fs::rename(&part, &final_path)
            .map_err(|e| fail(format!("Không lưu được file: {e}")))?;
        if let Some(saved) = &st.hooks.file_saved {
            saved(&final_path);
        }
    }

    let done = session.done.fetch_add(1, Ordering::SeqCst) + 1;
    if done == session.files.len() {
        tracker.set_status(TransferStatus::Done, None);
    }
    Ok(StatusCode::OK)
}

enum WriteError {
    Cancelled,
    Disconnected,
    Io(String),
}

async fn write_body(
    part: &std::path::Path,
    body: Body,
    expected: u64,
    tracker: &Tracker,
) -> Result<(), WriteError> {
    let f = tokio::fs::File::create(part)
        .await
        .map_err(|e| WriteError::Io(format!("Không ghi được file: {e}")))?;
    let mut w = tokio::io::BufWriter::with_capacity(1 << 20, f);
    let mut stream = body.into_data_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        if tracker.is_cancelled() {
            return Err(WriteError::Cancelled);
        }
        let chunk = chunk.map_err(|_| WriteError::Disconnected)?;
        written += chunk.len() as u64;
        if written > expected {
            return Err(WriteError::Io("Dữ liệu nhận được lớn hơn kích thước khai báo".into()));
        }
        w.write_all(&chunk)
            .await
            .map_err(|e| WriteError::Io(format!("Không ghi được file: {e}")))?;
        tracker.add(chunk.len() as u64);
    }
    w.flush()
        .await
        .map_err(|e| WriteError::Io(format!("Không ghi được file: {e}")))?;
    if written != expected {
        return Err(if tracker.is_cancelled() {
            WriteError::Cancelled
        } else {
            WriteError::Disconnected
        });
    }
    Ok(())
}

async fn cancel(State(st): St, Query(q): Query<SessionQuery>) -> StatusCode {
    let session = st.incoming.lock().unwrap().get(&q.session).cloned();
    match session {
        Some(s) => {
            s.tracker.set_status(
                TransferStatus::Cancelled,
                Some("Máy gửi đã huỷ".into()),
            );
            StatusCode::OK
        }
        None => StatusCode::NOT_FOUND,
    }
}
