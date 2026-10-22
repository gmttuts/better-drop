//! Máy chủ HTTPS nhận yêu cầu và dữ liệu từ máy khác.

use crate::models::*;
use crate::paths::{sanitize_rel_path, unique_path};
use crate::state::{AppState, InFile, InSession, Tracker};
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Query, State};
use axum::Extension;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
        .route("/api/v1/upload-status", get(upload_status))
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
    let cfg = axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(tls_config(&state.identity)?));
    let acceptor = PeerCertAcceptor(axum_server::tls_rustls::RustlsAcceptor::new(cfg));
    axum_server::from_tcp(listener)
        .acceptor(acceptor)
        .serve(router(state).into_make_service_with_connect_info::<SocketAddr>())
        .await
        .map_err(|e| e.to_string())
}

// ---- TLS hai chiều: biết chắc máy gửi là máy nào ----

/// Dấu vân tay chứng chỉ mà máy gửi đã trình khi kết nối (None: bản cũ, trình duyệt).
/// Có được nghĩa là máy gửi nắm khoá bí mật của chứng chỉ đó (chữ ký TLS đã được kiểm),
/// nên không thể giả mạo bằng cách chỉ khai id/tên của máy khác.
#[derive(Debug, Clone, Default)]
pub struct ClientFp(pub Option<String>);

/// Nhận mọi chứng chỉ client (tự ký) nhưng vẫn kiểm chữ ký bắt tay, và không bắt buộc
/// phải có: bản cũ không trình chứng chỉ vẫn kết nối được.
#[derive(Debug)]
struct AnyClientCert(Arc<rustls::crypto::CryptoProvider>);

impl rustls::server::danger::ClientCertVerifier for AnyClientCert {
    fn offer_client_auth(&self) -> bool {
        true
    }
    fn client_auth_mandatory(&self) -> bool {
        false
    }
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn tls_config(id: &crate::identity::Identity) -> Result<rustls::ServerConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let (certs, key) = id.der()?;
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_client_cert_verifier(Arc::new(AnyClientCert(provider)))
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

/// Sau bắt tay TLS, gắn dấu vân tay chứng chỉ của máy gửi vào mọi request.
#[derive(Clone)]
struct PeerCertAcceptor(axum_server::tls_rustls::RustlsAcceptor);

impl<I, S> axum_server::accept::Accept<I, S> for PeerCertAcceptor
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = tokio_rustls::server::TlsStream<I>;
    type Service = <axum::Extension<ClientFp> as tower::Layer<S>>::Service;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<(Self::Stream, Self::Service)>> + Send>,
    >;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.0.accept(stream, service);
        Box::pin(async move {
            let (stream, service) = inner.await?;
            let fp = stream
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|c| c.first())
                .map(|c| crate::identity::fingerprint(c.as_ref()));
            Ok((stream, tower::Layer::layer(&axum::Extension(ClientFp(fp)), service)))
        })
    }
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
    Extension(ClientFp(client_fp)): Extension<ClientFp>,
    Json(req): Json<PrepareRequest>,
) -> Result<Json<PrepareResponse>, StatusCode> {
    if !st.settings.read().unwrap().receive_enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    if req.kind == TransferKind::Text {
        remember_sender(&st, &req.sender, remote);
        receive_text(&st, &req.sender.name, Some(&req.sender.id), req.text.unwrap_or_default());
        return Ok(Json(PrepareResponse::default()));
    }
    let trust = trust_of(&st, &req.sender, client_fp.as_deref());
    let resp = accept_files(&st, &req.sender.name, Some(&req.sender.id), &req.sender.os, &req.files, trust).await?;
    remember_sender(&st, &req.sender, remote);
    Ok(Json(resp))
}

/// Máy gửi có được tự chấp nhận không, và có thể được đánh dấu tin cậy không.
#[derive(Default)]
pub struct Trust {
    /// Nhận luôn, không hỏi (máy đã được tin cậy và chứng minh đúng danh tính).
    pub auto: bool,
    /// Có thể hiện ô "Luôn chấp nhận từ máy này".
    pub candidate: Option<crate::devices::TrustedDevice>,
}

/// Danh tính chỉ được coi là thật khi chứng chỉ máy gửi trình trong TLS (đã kiểm chữ
/// ký) có đúng dấu vân tay mà máy đó khai báo/quảng bá. Bản cũ không trình chứng chỉ
/// nên không bao giờ được tự chấp nhận, vẫn phải hỏi như trước.
pub fn trust_of(st: &AppState, sender: &DeviceInfo, client_fp: Option<&str>) -> Trust {
    let proven = client_fp.is_some_and(|fp| fp.eq_ignore_ascii_case(&sender.fp));
    if !proven {
        return Trust::default();
    }
    Trust {
        auto: st.devices.lock().unwrap().is_trusted(&sender.id, &sender.fp),
        candidate: Some(crate::devices::TrustedDevice {
            id: sender.id.clone(),
            name: sender.name.clone(),
            os: sender.os.clone(),
            fp: sender.fp.clone(),
            added_at: crate::history::now_secs(),
        }),
    }
}

/// Nhận một đoạn văn bản: hiện cho người dùng và ghi vào mục Hoạt động.
pub fn receive_text(st: &AppState, sender_name: &str, sender_id: Option<&str>, text: String) {
    st.emit(
        "incoming-text",
        IncomingText {
            sender_name: sender_name.to_string(),
            text: text.clone(),
        },
    );
    let tracker = Tracker::new(
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
            status: TransferStatus::Waiting,
            error: None,
            dir_path: None,
        },
    );
    tracker.record_to(st.recorder(), sender_id.map(str::to_string));
    tracker.set_history_text(&text);
    tracker.set_status(TransferStatus::Done, None);
}

/// Hỏi người dùng có nhận các file này không (chờ tối đa ACCEPT_TIMEOUT). Nếu đồng ý,
/// tạo phiên nhận và trả về token cho từng file. Dùng chung cho app và trang web.
pub async fn accept_files(
    st: &Arc<AppState>,
    sender_name: &str,
    sender_id: Option<&str>,
    sender_os: &str,
    req_files: &[FileMeta],
    trust: Trust,
) -> Result<PrepareResponse, StatusCode> {
    if req_files.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let total_bytes: u64 = req_files.iter().map(|f| f.size).sum();

    if trust.auto {
        // Máy tin cậy: nhận luôn, chỉ báo cho người dùng biết.
        st.emit(
            "auto-accepted",
            serde_json::json!({ "sender_name": sender_name, "file_count": req_files.len(), "total_bytes": total_bytes }),
        );
    } else {
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let can_trust = trust.candidate.is_some();
        st.pending.lock().unwrap().insert(
            request_id.clone(),
            crate::state::PendingRequest { tx, trust_candidate: trust.candidate },
        );
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
                can_trust,
            },
        );

        let accepted = matches!(tokio::time::timeout(ACCEPT_TIMEOUT, rx).await, Ok(Ok(true)));
        drop(guard);
        if !accepted {
            return Err(StatusCode::FORBIDDEN);
        }
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
    tracker.record_to(st.recorder(), sender_id.map(str::to_string));
    tracker.set_status(TransferStatus::Transferring, None);
    st.incoming.lock().unwrap().insert(
        session_id.clone(),
        Arc::new(InSession {
            tracker,
            files,
            base_dir,
            root_map: Default::default(),
            active: Default::default(),
            done_files: Default::default(),
            counted: Default::default(),
            interrupted: Default::default(),
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
/// Máy nhận giữ phần đã nhận bao lâu để chờ máy gửi nối lại sau khi mất kết nối.
pub const RESUME_GRACE: Duration = Duration::from_secs(120);

fn part_path(session: &InSession, file: &InFile, token: &str) -> std::io::Result<PathBuf> {
    let dest = session.dest_path(&file.rel)?;
    Ok(dest.with_file_name(format!(
        "{}.{}.part",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        &token[..8]
    )))
}

fn find_upload(st: &AppState, q: &UploadQuery) -> Result<(Arc<InSession>, String), (StatusCode, String)> {
    let session = st
        .incoming
        .lock()
        .unwrap()
        .get(&q.session)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Không tìm thấy phiên".into()))?;
    match session.files.get(&q.file) {
        None => Err((StatusCode::NOT_FOUND, "Không tìm thấy file".into())),
        Some(f) if f.token != q.token => Err((StatusCode::FORBIDDEN, "Sai token".into())),
        Some(_) => Ok((session, q.file.clone())),
    }
}

/// Máy gửi hỏi đã nhận được bao nhiêu byte, để gửi tiếp sau khi mất kết nối.
async fn upload_status(State(st): St, Query(q): Query<UploadQuery>) -> Result<Json<UploadStatus>, (StatusCode, String)> {
    let (session, id) = find_upload(&st, &q)?;
    if session.tracker.status().is_final() && !session.done_files.lock().unwrap().contains(&id) {
        return Err((StatusCode::GONE, "Phiên nhận đã kết thúc".into()));
    }
    let done = session.done_files.lock().unwrap().contains(&id);
    let file = &session.files[&id];
    let received = if done {
        file.size
    } else {
        let part = part_path(&session, file, &q.token).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0)
    };
    Ok(Json(UploadStatus { received, done }))
}

/// Nhận dữ liệu một file của phiên đã được chấp nhận (dùng chung cho app và trang web).
/// Có `offset`: ghi tiếp vào phần đã nhận (gửi tiếp sau khi mất kết nối).
pub async fn receive_upload(
    st: &Arc<AppState>,
    q: UploadQuery,
    body: Body,
) -> Result<StatusCode, (StatusCode, String)> {
    let (session, id) = find_upload(st, &q)?;
    let file = &session.files[&id];
    let tracker = &session.tracker;
    if tracker.is_cancelled() {
        return Err((StatusCode::GONE, "Máy nhận đã huỷ".into()));
    }
    if tracker.status().is_final() {
        return Err((StatusCode::GONE, "Phiên nhận đã kết thúc".into()));
    }
    if session.done_files.lock().unwrap().contains(&id) {
        return Err((StatusCode::CONFLICT, "File đã được gửi".into()));
    }
    if !session.active.lock().unwrap().insert(id.clone()) {
        return Err((StatusCode::CONFLICT, "File đang được gửi".into()));
    }
    session.interrupted.lock().unwrap().remove(&id);
    // Bỏ khỏi `active` khi kết thúc, dù thành công hay lỗi.
    struct ActiveGuard<'a>(&'a InSession, String);
    impl Drop for ActiveGuard<'_> {
        fn drop(&mut self) {
            self.0.active.lock().unwrap().remove(&self.1);
        }
    }
    let _active = ActiveGuard(&session, id.clone());

    let fail = |msg: String| {
        tracker.set_status(TransferStatus::Error, Some(msg.clone()));
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    };
    let dest = session
        .dest_path(&file.rel)
        .map_err(|e| fail(format!("Không tạo được thư mục: {e}")))?;
    let part = part_path(&session, file, &q.token).map_err(|e| fail(format!("Không tạo được thư mục: {e}")))?;

    // Gửi tiếp: phần đã có phải đủ dài; cắt bỏ phần thừa (nếu có) rồi ghi nối.
    let offset = q.offset.unwrap_or(0);
    let have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if offset > have || offset > file.size {
        return Err((StatusCode::CONFLICT, format!("Máy nhận mới có {have} byte")));
    }
    {
        // Tiến trình: chỉ tính phần thực sự có trên đĩa.
        let mut counted = session.counted.lock().unwrap();
        let c = counted.entry(id.clone()).or_insert(0);
        if *c > offset {
            tracker.sub(*c - offset);
        } else if offset > *c {
            tracker.add(offset - *c);
        }
        *c = offset;
    }
    tracker.set_note(None);

    let result = write_body(&part, offset, body, file.size, tracker, &session, &id).await;
    if let Err(e) = result {
        return Err(match e {
            WriteError::Cancelled => {
                let _ = tokio::fs::remove_file(&part).await;
                (StatusCode::GONE, "Máy nhận đã huỷ".into())
            }
            WriteError::Disconnected => {
                // Khi bên gửi bấm Huỷ, kết nối đứt trước rồi lệnh /cancel mới tới.
                tokio::time::sleep(Duration::from_millis(1500)).await;
                if tracker.is_cancelled() {
                    let _ = tokio::fs::remove_file(&part).await;
                } else {
                    wait_for_resume(st.clone(), session.clone(), id.clone(), part.clone());
                }
                (StatusCode::BAD_REQUEST, "Mất kết nối".into())
            }
            WriteError::Io(msg) => {
                let _ = tokio::fs::remove_file(&part).await;
                fail(msg)
            }
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
        tracker.add_history_file(file.rel.join("/"), Some(final_path.to_string_lossy().into_owned()));
    }
    session.done_files.lock().unwrap().insert(id.clone());

    let done = session.done.fetch_add(1, Ordering::SeqCst) + 1;
    if done == session.files.len() {
        tracker.set_status(TransferStatus::Done, None);
    }
    Ok(StatusCode::OK)
}

/// Giữ phần đã nhận và chờ máy gửi nối lại; quá RESUME_GRACE thì báo lỗi và xoá.
fn wait_for_resume(st: Arc<AppState>, session: Arc<InSession>, id: String, part: PathBuf) {
    let at = Instant::now();
    session.interrupted.lock().unwrap().insert(id.clone(), at);
    session
        .tracker
        .set_note(Some("Mất kết nối, đang chờ máy gửi nối lại…".into()));
    let _ = st;
    tokio::spawn(async move {
        tokio::time::sleep(RESUME_GRACE).await;
        let still_waiting = session.interrupted.lock().unwrap().get(&id) == Some(&at);
        if still_waiting && !session.active.lock().unwrap().contains(&id) {
            session.interrupted.lock().unwrap().remove(&id);
            let _ = tokio::fs::remove_file(&part).await;
            session.tracker.set_note(None);
            session
                .tracker
                .set_status(TransferStatus::Error, Some("Mất kết nối với máy gửi".into()));
        }
    });
}

enum WriteError {
    Cancelled,
    Disconnected,
    Io(String),
}

async fn write_body(
    part: &std::path::Path,
    offset: u64,
    body: Body,
    expected: u64,
    tracker: &Tracker,
    session: &InSession,
    id: &str,
) -> Result<(), WriteError> {
    let io = |e: std::io::Error| WriteError::Io(format!("Không ghi được file: {e}"));
    let f = if offset == 0 {
        tokio::fs::File::create(part).await.map_err(io)?
    } else {
        let f = tokio::fs::OpenOptions::new().write(true).open(part).await.map_err(io)?;
        f.set_len(offset).await.map_err(io)?;
        let mut f = f;
        tokio::io::AsyncSeekExt::seek(&mut f, std::io::SeekFrom::Start(offset)).await.map_err(io)?;
        f
    };
    let mut w = tokio::io::BufWriter::with_capacity(1 << 20, f);
    let mut stream = body.into_data_stream();
    let mut written: u64 = offset;
    let result = loop {
        // Máy gửi im lặng quá lâu (mất Wi‑Fi không báo lỗi): coi như ngắt kết nối.
        let next = tokio::time::timeout(Duration::from_secs(45), stream.next()).await;
        let Ok(next) = next else { break Err(WriteError::Disconnected) };
        let Some(chunk) = next else { break Ok(()) };
        if tracker.is_cancelled() {
            break Err(WriteError::Cancelled);
        }
        let Ok(chunk) = chunk else { break Err(WriteError::Disconnected) };
        written += chunk.len() as u64;
        if written > expected {
            break Err(WriteError::Io("Dữ liệu nhận được lớn hơn kích thước khai báo".into()));
        }
        if let Err(e) = w.write_all(&chunk).await {
            break Err(io(e));
        }
        tracker.add(chunk.len() as u64);
        *session.counted.lock().unwrap().entry(id.to_string()).or_insert(0) += chunk.len() as u64;
    };
    // Luôn ghi phần đệm xuống đĩa, kể cả khi mất kết nối: để gửi tiếp được từ đúng chỗ.
    w.flush().await.map_err(io)?;
    result?;
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
