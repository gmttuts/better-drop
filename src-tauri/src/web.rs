//! Chia sẻ qua trình duyệt trong mạng LAN: thiết bị KHÔNG cài BetterDrop mở
//! `http://<ip>:53319` để gửi file/văn bản lên máy này hoặc tải những gì máy này
//! chia sẻ. Bảo vệ bằng mã PIN 6 số; file gửi lên vẫn phải được bấm Chấp nhận.
//!
//! Dùng HTTP thường (không phải HTTPS) vì chứng chỉ tự ký làm trình duyệt hiện cảnh
//! báo đáng sợ. Dữ liệu chỉ đi trong mạng nội bộ.

use crate::models::{Direction, FileMeta, TransferKind, TransferStatus, TransferUpdate, UploadQuery};
use futures_util::StreamExt;
use crate::server::{accept_files, receive_text, receive_upload};
use crate::state::{AppState, Tracker};
use crate::transfer::LocalFile;
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio_util::io::ReaderStream;

pub const WEB_PORT: u16 = 53319;
const MAX_PIN_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);
const COOKIE: &str = "bd_session";
const PAGE: &str = include_str!("../web/index.html");

/// Một file máy này đang chia sẻ cho trình duyệt tải về.
pub struct SharedFile {
    pub meta: FileMeta,
    pub source: String,
}

pub struct WebShare {
    pub pin: String,
    pub port: u16,
    sessions: HashSet<String>,
    failures: HashMap<IpAddr, (u32, Instant)>,
    pub files: Vec<SharedFile>,
    pub text: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
}

/// Trạng thái gửi lên giao diện máy tính.
#[derive(Debug, Clone, Serialize)]
pub struct WebInfo {
    pub pin: String,
    pub urls: Vec<String>,
    /// Mã QR của link đầu tiên, có kèm PIN (sau dấu #, không bao giờ gửi lên máy chủ).
    pub qr_svg: String,
    pub files: Vec<FileMeta>,
    pub text: Option<String>,
    /// Số trình duyệt đã nhập đúng PIN (kể từ khi bật hoặc đổi PIN).
    pub clients: usize,
}

fn new_pin() -> String {
    let b = uuid::Uuid::new_v4();
    let n = u32::from_le_bytes(b.as_bytes()[..4].try_into().unwrap()) % 1_000_000;
    format!("{n:06}")
}

fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn info(st: &AppState, local_ips: &[String]) -> Option<WebInfo> {
    let web = st.web.lock().unwrap();
    let w = web.as_ref()?;
    let urls: Vec<String> = local_ips.iter().map(|ip| format!("http://{ip}:{}", w.port)).collect();
    let qr_svg = urls
        .first()
        .and_then(|u| crate::discovery::pair_svg(&format!("{u}/#pin={}", w.pin)).ok())
        .unwrap_or_default();
    Some(WebInfo {
        pin: w.pin.clone(),
        urls,
        qr_svg,
        files: w.files.iter().map(|f| f.meta.clone()).collect(),
        text: w.text.clone(),
        clients: w.sessions.len(),
    })
}

/// Bật máy chủ web (nếu chưa bật). Thử cổng 53319 trước, bận thì lấy cổng ngẫu nhiên.
pub async fn start(st: Arc<AppState>) -> Result<(), String> {
    if st.web.lock().unwrap().is_some() {
        return Ok(());
    }
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", WEB_PORT)).await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind(("0.0.0.0", 0))
            .await
            .map_err(|e| format!("Không mở được cổng cho trang web: {e}"))?,
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, rx) = oneshot::channel::<()>();
    *st.web.lock().unwrap() = Some(WebShare {
        pin: new_pin(),
        port,
        sessions: HashSet::new(),
        failures: HashMap::new(),
        files: Vec::new(),
        text: None,
        shutdown: Some(tx),
    });
    let app = router(st.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(())
}

pub fn stop(st: &AppState) {
    if let Some(mut w) = st.web.lock().unwrap().take() {
        if let Some(tx) = w.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// Đổi PIN: mọi trình duyệt đang đăng nhập phải nhập lại.
pub fn renew_pin(st: &AppState) {
    if let Some(w) = st.web.lock().unwrap().as_mut() {
        w.pin = new_pin();
        w.sessions.clear();
        w.failures.clear();
    }
}

pub fn set_files(st: &AppState, files: Vec<LocalFile>) {
    if let Some(w) = st.web.lock().unwrap().as_mut() {
        w.files = files
            .into_iter()
            .map(|f| SharedFile { meta: f.meta, source: f.source })
            .collect();
    }
}

/// Thêm file vào danh sách chia sẻ (bỏ qua file đã có).
pub fn add_files(st: &AppState, files: Vec<LocalFile>) {
    if let Some(w) = st.web.lock().unwrap().as_mut() {
        for f in files {
            if !w.files.iter().any(|x| x.source == f.source) {
                w.files.push(SharedFile { meta: f.meta, source: f.source });
            }
        }
    }
}

pub fn remove_file(st: &AppState, id: &str) {
    if let Some(w) = st.web.lock().unwrap().as_mut() {
        w.files.retain(|f| f.meta.id != id);
    }
}

pub fn set_text(st: &AppState, text: Option<String>) {
    if let Some(w) = st.web.lock().unwrap().as_mut() {
        w.text = text.filter(|t| !t.trim().is_empty());
    }
}

// ---- Máy chủ HTTP ----

type St = State<Arc<AppState>>;

fn router(st: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/api/login", post(login))
        .route("/api/state", get(state))
        .route("/api/file/{id}", get(download))
        .route("/api/text", post(send_text))
        .route("/api/prepare", post(prepare).layer(DefaultBodyLimit::max(8 * 1024 * 1024)))
        .route("/api/upload", post(upload).layer(DefaultBodyLimit::disable()))
        .route("/api/cancel", post(cancel))
        .with_state(st)
}

fn session_of(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|c| {
        let (k, v) = c.trim().split_once('=')?;
        (k == COOKIE).then(|| v.to_string())
    })
}

/// Kiểm tra trình duyệt đã đăng nhập bằng PIN chưa.
fn authed(st: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let token = session_of(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let web = st.web.lock().unwrap();
    match web.as_ref() {
        Some(w) if w.sessions.contains(&token) => Ok(()),
        Some(_) => Err(StatusCode::UNAUTHORIZED),
        None => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(Deserialize)]
struct LoginReq {
    pin: String,
}

async fn login(
    State(st): St,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(req): Json<LoginReq>,
) -> Response {
    let ip = remote.ip().to_canonical();
    let mut web = st.web.lock().unwrap();
    let Some(w) = web.as_mut() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if let Some((n, at)) = w.failures.get(&ip) {
        if *n >= MAX_PIN_FAILURES && at.elapsed() < LOCKOUT {
            let wait = (LOCKOUT - at.elapsed()).as_secs() + 1;
            return (StatusCode::TOO_MANY_REQUESTS, format!("Nhập sai quá nhiều lần. Thử lại sau {wait} giây.")).into_response();
        }
    }
    if req.pin.trim() != w.pin {
        let e = w.failures.entry(ip).or_insert((0, Instant::now()));
        if e.1.elapsed() >= LOCKOUT {
            e.0 = 0;
        }
        e.0 += 1;
        e.1 = Instant::now();
        return (StatusCode::UNAUTHORIZED, "Sai mã PIN").into_response();
    }
    w.failures.remove(&ip);
    let token = new_token();
    w.sessions.insert(token.clone());
    drop(web);
    // Báo giao diện máy tính cập nhật số trình duyệt đã kết nối.
    st.emit("web-changed", serde_json::json!({}));
    let cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict");
    let mut resp = Json(serde_json::json!({ "name": st.device_info().name })).into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    resp
}

async fn state(State(st): St, headers: HeaderMap) -> Result<Json<serde_json::Value>, StatusCode> {
    authed(&st, &headers)?;
    let web = st.web.lock().unwrap();
    let w = web.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(serde_json::json!({
        "name": st.device_info().name,
        "receive_enabled": st.settings.read().unwrap().receive_enabled,
        "files": w.files.iter().map(|f| &f.meta).collect::<Vec<_>>(),
        "text": w.text,
    })))
}

/// Mã hoá tên file cho Content-Disposition (RFC 5987), giữ được tiếng Việt.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "._- ()".contains(c) { c } else { '_' })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"._-".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

async fn download(State(st): St, headers: HeaderMap, Path(id): Path<String>) -> Result<Response, StatusCode> {
    authed(&st, &headers)?;
    let (meta, source) = {
        let web = st.web.lock().unwrap();
        let w = web.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        let f = w.files.iter().find(|f| f.meta.id == id).ok_or(StatusCode::NOT_FOUND)?;
        (f.meta.clone(), f.source.clone())
    };
    let file = st.open_source(&source).map_err(|_| StatusCode::GONE)?;
    let name = meta.rel_path.rsplit('/').next().unwrap_or(&meta.rel_path).to_string();

    // Ghi lượt tải vào mục Hoạt động trên máy tính: tiến trình, rồi Hoàn tất hoặc
    // "Trình duyệt dừng tải" nếu người dùng huỷ giữa chừng.
    let tracker = Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: uuid::Uuid::new_v4().to_string(),
            direction: Direction::Send,
            peer_name: format!("trình duyệt · {name}"),
            kind: TransferKind::Files,
            file_count: 1,
            total_bytes: meta.size,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Waiting,
            error: None,
            dir_path: None,
        },
    );
    tracker.set_status(TransferStatus::Transferring, None);
    if meta.size == 0 {
        tracker.set_status(TransferStatus::Done, None);
    }
    let guard = DownloadGuard(tracker.clone());
    let total = meta.size;
    let mut sent = 0u64;
    let stream = ReaderStream::with_capacity(tokio::fs::File::from_std(file), 256 * 1024).map(move |chunk| {
        let _keep = &guard;
        if let Ok(b) = &chunk {
            sent += b.len() as u64;
            tracker.add(b.len() as u64);
            if sent >= total {
                tracker.set_status(TransferStatus::Done, None);
            }
        }
        chunk
    });
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, meta.size.to_string()),
            (header::CONTENT_DISPOSITION, content_disposition(&name)),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

/// Khi luồng tải bị huỷ trước khi xong (trình duyệt đóng/huỷ), đánh dấu đã dừng.
struct DownloadGuard(Arc<Tracker>);

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.0
            .set_status(TransferStatus::Cancelled, Some("Trình duyệt dừng tải".into()));
    }
}

#[derive(Deserialize)]
struct TextReq {
    #[serde(default)]
    name: String,
    text: String,
}

fn browser_name(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        "Trình duyệt".to_string()
    } else {
        format!("{} (trình duyệt)", n.chars().take(40).collect::<String>())
    }
}

async fn send_text(State(st): St, headers: HeaderMap, Json(req): Json<TextReq>) -> StatusCode {
    if let Err(e) = authed(&st, &headers) {
        return e;
    }
    if !st.settings.read().unwrap().receive_enabled {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if req.text.trim().is_empty() || req.text.len() > 1_000_000 {
        return StatusCode::BAD_REQUEST;
    }
    receive_text(&st, &browser_name(&req.name), req.text);
    StatusCode::OK
}

#[derive(Deserialize)]
struct PrepareReq {
    #[serde(default)]
    name: String,
    files: Vec<FileMeta>,
}

async fn prepare(State(st): St, headers: HeaderMap, Json(req): Json<PrepareReq>) -> Response {
    if let Err(e) = authed(&st, &headers) {
        return e.into_response();
    }
    if !st.settings.read().unwrap().receive_enabled {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    match accept_files(&st, &browser_name(&req.name), "web", &req.files).await {
        Ok(resp) => Json(resp).into_response(),
        Err(code) => code.into_response(),
    }
}

async fn upload(State(st): St, headers: HeaderMap, Query(q): Query<UploadQuery>, body: Body) -> Response {
    if let Err(e) = authed(&st, &headers) {
        return e.into_response();
    }
    match receive_upload(&st, q, body).await {
        Ok(code) => code.into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize)]
struct CancelQuery {
    session: String,
}

async fn cancel(State(st): St, headers: HeaderMap, Query(q): Query<CancelQuery>) -> StatusCode {
    if let Err(e) = authed(&st, &headers) {
        return e;
    }
    crate::transfer::cancel(&st, &q.session);
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_keeps_vietnamese() {
        let h = content_disposition("Báo cáo.pdf");
        assert!(h.contains("filename*=UTF-8''B%C3%A1o%20c%C3%A1o.pdf"), "{h}");
        assert!(h.contains("filename=\"B_o c_o.pdf\""), "{h}");
    }

    #[test]
    fn pin_is_six_digits() {
        for _ in 0..50 {
            let p = new_pin();
            assert_eq!(p.len(), 6);
            assert!(p.chars().all(|c| c.is_ascii_digit()));
        }
    }
}
