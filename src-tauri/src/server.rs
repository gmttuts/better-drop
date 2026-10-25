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

/// Văn bản nhận được dài nhất (byte), giống giới hạn của trang web.
pub const MAX_TEXT_BYTES: usize = 1_000_000;
/// Tên máy/nhóm hiện trong hộp thoại dài nhất (ký tự).
const MAX_NAME_CHARS: usize = 64;
/// Số hộp thoại Chấp nhận chờ cùng lúc, tổng cộng và từ một địa chỉ IP.
pub const MAX_PENDING: usize = 6;
pub const MAX_PENDING_PER_IP: usize = 2;
/// Số đoạn văn bản tối đa một IP được gửi trong TEXT_WINDOW (văn bản hiện ngay, không hỏi).
pub const MAX_TEXTS_PER_WINDOW: usize = 20;
const TEXT_WINDOW: Duration = Duration::from_secs(60);

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
        .route("/api/v1/pause", post(pause))
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

// ---- Chống spam: máy lạ trong LAN gửi dồn dập /prepare ----

/// Bộ đếm theo (máy nhận, IP gửi). Khoá gồm dấu vân tay máy nhận để các "máy" chạy
/// cùng tiến trình trong test không đếm lẫn của nhau.
#[derive(Default)]
struct Flood {
    pending: HashMap<(String, std::net::IpAddr), usize>,
    texts: HashMap<(String, std::net::IpAddr), Vec<Instant>>,
}

static FLOOD: std::sync::LazyLock<std::sync::Mutex<Flood>> = std::sync::LazyLock::new(Default::default);

fn flood_key(st: &AppState, ip: std::net::IpAddr) -> (String, std::net::IpAddr) {
    (st.identity.fingerprint.clone(), ip.to_canonical())
}

/// Một chỗ trong hàng chờ hộp thoại Chấp nhận; trả lại khi drop.
pub struct PendingSlot {
    key: (String, std::net::IpAddr),
}

impl Drop for PendingSlot {
    fn drop(&mut self) {
        let mut f = FLOOD.lock().unwrap();
        if let Some(n) = f.pending.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                f.pending.remove(&self.key);
            }
        }
    }
}

/// Giữ chỗ cho một yêu cầu cần hỏi người dùng. Quá nhiều yêu cầu đang chờ (tổng cộng
/// hoặc từ cùng một IP) thì trả 429, để không bị spam hộp thoại hay giữ kết nối vô hạn.
pub fn pending_slot(st: &Arc<AppState>, ip: std::net::IpAddr) -> Result<PendingSlot, StatusCode> {
    let key = flood_key(st, ip);
    let mut f = FLOOD.lock().unwrap();
    let total: usize = f.pending.iter().filter(|(k, _)| k.0 == key.0).map(|(_, n)| n).sum();
    let mine = f.pending.get(&key).copied().unwrap_or(0);
    if total >= MAX_PENDING || mine >= MAX_PENDING_PER_IP {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    *f.pending.entry(key.clone()).or_insert(0) += 1;
    Ok(PendingSlot { key })
}

/// Văn bản hiện ngay không cần hỏi, nên giới hạn số lần mỗi IP được gửi.
pub fn allow_text(st: &Arc<AppState>, ip: std::net::IpAddr) -> bool {
    let key = flood_key(st, ip);
    let mut f = FLOOD.lock().unwrap();
    f.texts.retain(|_, v| {
        v.retain(|t| t.elapsed() < TEXT_WINDOW);
        !v.is_empty()
    });
    let sent = f.texts.entry(key).or_default();
    if sent.len() >= MAX_TEXTS_PER_WINDOW {
        return false;
    }
    sent.push(Instant::now());
    true
}

/// So sánh hai chuỗi bí mật (token, PIN) trong thời gian không phụ thuộc nội dung.
pub fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Tên do máy khác khai để hiện lên giao diện: bỏ ký tự điều khiển/đảo chiều chữ
/// (giả tên máy khác), cắt bớt nếu quá dài.
pub fn clean_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| !crate::paths::is_bidi_control(*c))
        .take(MAX_NAME_CHARS)
        .collect();
    s.trim().to_string()
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
    // Máy đã biết (đã lưu, tin cậy, hay mDNS) nhưng với chứng chỉ khác: không ghi đè.
    // Nếu không, máy lạ chỉ cần khai id của máy khác (không cần hỏi, vd gửi văn bản) là
    // lần sau bấm gửi tới máy đó, file sẽ đi tới máy lạ (client ghim theo dấu vân tay
    // đã lưu). Máy cài lại có chứng chỉ mới: người dùng "Quên" rồi thêm lại.
    let other_fp = |fp: &str| !fp.is_empty() && !fp.eq_ignore_ascii_case(&sender.fp);
    if st.devices.lock().unwrap().fp_of(&sender.id).is_some_and(other_fp) {
        return;
    }
    if st.peers.lock().unwrap().values().any(|p| p.info.id == sender.id && other_fp(&p.info.fp)) {
        return;
    }
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
    Json(mut req): Json<PrepareRequest>,
) -> Result<Json<PrepareResponse>, StatusCode> {
    if !st.settings.read().unwrap().receive_enabled {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    req.sender.name = clean_name(&req.sender.name);
    req.sender.os = clean_name(&req.sender.os);
    if req.kind == TransferKind::Group {
        let group = req.group.clone().ok_or(StatusCode::BAD_REQUEST)?;
        let _slot = pending_slot(&st, remote.ip())?;
        receive_group(&st, &req.sender, group).await?;
        remember_sender(&st, &req.sender, remote);
        return Ok(Json(PrepareResponse::default()));
    }
    if req.kind == TransferKind::Text {
        let text = req.text.unwrap_or_default();
        if text.len() > MAX_TEXT_BYTES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        if !allow_text(&st, remote.ip()) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        remember_sender(&st, &req.sender, remote);
        receive_text(&st, &req.sender.name, Some(&req.sender.id), text);
        return Ok(Json(PrepareResponse::default()));
    }
    let _slot = pending_slot(&st, remote.ip())?;
    let trust = trust_of(&st, &req.sender, client_fp.as_deref());
    let resp = accept_files_with_thumbs(
        &st,
        &req.sender.name,
        Some(&req.sender.id),
        &req.sender.os,
        &req.files,
        &req.thumbs,
        trust,
    )
    .await?;
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

/// Hiện hộp thoại Chấp nhận/Từ chối và chờ trả lời (tối đa ACCEPT_TIMEOUT).
pub async fn ask_user(
    st: &Arc<AppState>,
    request: IncomingRequest,
    trust_candidate: Option<crate::devices::TrustedDevice>,
) -> bool {
    let (tx, rx) = oneshot::channel();
    st.pending.lock().unwrap().insert(
        request.request_id.clone(),
        crate::state::PendingRequest { tx, trust_candidate },
    );
    let guard = PendingGuard {
        st: st.clone(),
        id: request.request_id.clone(),
    };
    st.emit("incoming-request", request);
    let accepted = matches!(tokio::time::timeout(ACCEPT_TIMEOUT, rx).await, Ok(Ok(true)));
    drop(guard);
    accepted
}

/// Nhận một nhóm máy được chia sẻ: hỏi người dùng, rồi thêm nhóm và các máy trong
/// nhóm (để kết nối được ngay). Nhóm trùng tên thì cập nhật danh sách máy.
async fn receive_group(st: &Arc<AppState>, sender: &DeviceInfo, group: SharedGroup) -> Result<(), StatusCode> {
    let me = st.device_info().id;
    let mut group = group;
    group.name = clean_name(&group.name);
    let members: Vec<SharedMember> = group
        .members
        .into_iter()
        .filter(|m| m.id != me)
        .take(500)
        .map(|m| SharedMember { name: clean_name(&m.name), os: clean_name(&m.os), ..m })
        .collect();
    if group.name.trim().is_empty() || members.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let request = IncomingRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        sender_name: sender.name.clone(),
        sender_os: sender.os.clone(),
        file_count: members.len(),
        total_bytes: 0,
        preview: members.iter().take(5).map(|m| m.name.clone()).collect(),
        timeout_secs: ACCEPT_TIMEOUT.as_secs(),
        can_trust: false,
        group_name: Some(group.name.clone()),
        media: Vec::new(),
        media_more: 0,
    };
    if !ask_user(st, request, None).await {
        return Err(StatusCode::FORBIDDEN);
    }
    let refs: Vec<crate::devices::DeviceRef> = members
        .iter()
        .map(|m| crate::devices::DeviceRef { id: m.id.clone(), name: m.name.clone(), os: m.os.clone() })
        .collect();
    {
        let mut book = st.devices.lock().unwrap();
        let existing = book.groups.iter().find(|g| g.name == group.name).map(|g| g.id.clone());
        book.save_group(existing, group.name.clone(), refs);
        let _ = book.save(&st.data_dir);
    }
    // Máy chưa biết nhưng có địa chỉ + dấu vân tay: lưu lại để gửi được ngay.
    for m in members.into_iter().filter(|m| !m.fp.is_empty() && !m.addrs.is_empty()) {
        let known = st.peer_list().iter().any(|p| p.info.id == m.id);
        if !known {
            st.remember_peer(Peer {
                info: DeviceInfo { id: m.id, name: m.name, os: m.os, fp: m.fp, port: 0, version: String::new() },
                addrs: m.addrs,
                manual: true,
                online: true,
            });
        }
    }
    st.emit_peers();
    st.emit_devices();
    st.emit("group-received", serde_json::json!({ "name": group.name, "sender_name": sender.name }));
    Ok(())
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
    accept_files_with_thumbs(st, sender_name, sender_id, sender_os, req_files, &[], trust).await
}

/// Như `accept_files`, kèm ảnh thu nhỏ máy gửi khai (chỉ giữ ảnh hợp lệ, xem `thumbs::accept`).
pub async fn accept_files_with_thumbs(
    st: &Arc<AppState>,
    sender_name: &str,
    sender_id: Option<&str>,
    sender_os: &str,
    req_files: &[FileMeta],
    thumbs: &[Thumb],
    trust: Trust,
) -> Result<PrepareResponse, StatusCode> {
    if req_files.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Cộng có kiểm tra tràn: nếu không, kích thước khổng lồ cộng lại "quay vòng" thành
    // vài KB trong hộp thoại, rồi máy gửi được ghi tới hết ổ đĩa.
    let total_bytes = req_files
        .iter()
        .try_fold(0u64, |acc, f| acc.checked_add(f.size))
        .ok_or(StatusCode::BAD_REQUEST)?;
    let thumbs = crate::thumbs::accept(req_files, thumbs);
    // Tên đúng như sẽ lưu (đã làm sạch), không phải tên máy gửi khai.
    let clean = |f: &FileMeta| sanitize_rel_path(&f.rel_path).join("/");

    if trust.auto {
        // Máy tin cậy: nhận luôn, chỉ báo cho người dùng biết.
        st.emit(
            "auto-accepted",
            serde_json::json!({ "sender_name": sender_name, "file_count": req_files.len(), "total_bytes": total_bytes }),
        );
    } else {
        let can_trust = trust.candidate.is_some();
        let media_files: Vec<(usize, &FileMeta)> = req_files
            .iter()
            .enumerate()
            .filter(|(_, f)| crate::thumbs::media_kind(&f.rel_path).is_some())
            .collect();
        let media = media_files
            .iter()
            .take(crate::thumbs::MAX_THUMBS)
            .map(|(i, f)| MediaPreview {
                name: clean(f),
                video: crate::thumbs::media_kind(&f.rel_path) == Some(crate::thumbs::Media::Video),
                thumb: thumbs.iter().find(|(t, _)| t == i).map(|(_, jpeg)| crate::thumbs::data_url(jpeg)),
            })
            .collect();
        let request = IncomingRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            sender_name: sender_name.to_string(),
            sender_os: sender_os.to_string(),
            file_count: req_files.len(),
            total_bytes,
            // Hiện đúng tên sẽ được lưu (đã làm sạch), không phải tên máy gửi khai.
            preview: req_files.iter().take(5).map(clean).collect(),
            timeout_secs: ACCEPT_TIMEOUT.as_secs(),
            can_trust,
            group_name: None,
            media,
            media_more: media_files.len().saturating_sub(crate::thumbs::MAX_THUMBS),
        };
        if !ask_user(st, request, trust.candidate).await {
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
    tracker.set_history_thumbs(thumbs.into_iter().map(|(i, jpeg)| (clean(&req_files[i]), jpeg)).collect());
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
            made_dirs: Default::default(),
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
/// Máy gửi đã báo tạm dừng: giữ phiên và phần đã nhận tối đa chừng này.
pub const PAUSE_HOLD: Duration = Duration::from_secs(24 * 3600);

fn part_path(dest: &std::path::Path, token: &str) -> PathBuf {
    dest.with_file_name(format!(
        "{}.{}.part",
        dest.file_name().unwrap_or_default().to_string_lossy(),
        &token[..8]
    ))
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
        Some(f) if !ct_eq(&f.token, &q.token) => Err((StatusCode::FORBIDDEN, "Sai token".into())),
        Some(_) => Ok((session, q.file.clone())),
    }
}

/// Máy gửi hỏi đã nhận được bao nhiêu byte, để gửi tiếp sau khi mất kết nối.
async fn upload_status(State(st): St, Query(q): Query<UploadQuery>) -> Result<Json<UploadStatus>, (StatusCode, String)> {
    let (session, id) = find_upload(&st, &q)?;
    if session.tracker.status().is_final() && !session.done_files.lock().unwrap().contains(&id) {
        let msg = if session.tracker.is_cancelled() { "Máy nhận đã huỷ" } else { "Phiên nhận đã kết thúc" };
        return Err((StatusCode::GONE, msg.into()));
    }
    // Máy gửi hỏi tiến độ sau khi tạm dừng tức là đã tiếp tục (phòng khi lệnh tiếp tục bị lạc).
    if session.tracker.status() == TransferStatus::Paused {
        session.tracker.set_status(TransferStatus::Transferring, None);
    }
    let done = session.done_files.lock().unwrap().contains(&id);
    let file = &session.files[&id];
    let received = if done {
        file.size
    } else {
        let dest = session.dest_path(&file.rel).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let part = part_path(&dest, &q.token);
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
    let resumed = session.interrupted.lock().unwrap().remove(&id).is_some();
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
    let part = part_path(&dest, &q.token);

    // Gửi tiếp: phần đã có phải đủ dài; cắt bỏ phần thừa (nếu có) rồi ghi nối.
    // Gửi từ đầu (offset 0, mọi file nhỏ) thì khỏi stat: file .part sẽ được tạo mới.
    let offset = q.offset.unwrap_or(0);
    let have = if offset == 0 {
        0
    } else {
        tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0)
    };
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
    // Ghi chú "mất kết nối" chỉ có khi file bị ngắt; xoá mỗi file là một sự kiện lên giao diện.
    if resumed {
        tracker.set_note(None);
    }

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

    // Đổi tên trên luồng blocking: khoá + rename + stat ngay trên luồng async làm các
    // luồng worker đứng chờ nhau khi nhận nhiều file nhỏ cùng lúc. Ghi sổ cũng nằm trong
    // đó để luôn chạy trọn, kể cả khi máy gửi ngắt kết nối lúc đang chờ.
    let (st, session) = (st.clone(), session.clone());
    tokio::task::spawn_blocking(move || {
        let file = &session.files[&id];
        let tracker = &session.tracker;
        // Rename vẫn nằm trong khoá: thử cho rename chạy song song (chỉ khoá lúc chọn tên)
        // thì APFS tranh chấp khoá thư mục trong kernel, CPU hệ thống tăng gấp 3, chậm hơn.
        let final_path = {
            let _lock = st.rename_lock.lock().unwrap();
            let final_path = unique_path(&dest);
            if let Err(e) = std::fs::rename(&part, &final_path) {
                let msg = format!("Không lưu được file: {e}");
                tracker.set_status(TransferStatus::Error, Some(msg.clone()));
                return Err((StatusCode::INTERNAL_SERVER_ERROR, msg));
            }
            final_path
        };
        if let Some(saved) = &st.hooks.file_saved {
            saved(&final_path);
        }
        tracker.add_history_file(file.rel.join("/"), Some(final_path.to_string_lossy().into_owned()));
        session.done_files.lock().unwrap().insert(id);

        let done = session.done.fetch_add(1, Ordering::SeqCst) + 1;
        if done == session.files.len() {
            tracker.set_status(TransferStatus::Done, None);
        }
        Ok(())
    })
    .await
    .unwrap_or_else(|e| Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())))?;
    Ok(StatusCode::OK)
}

/// Giữ phần đã nhận và chờ máy gửi nối lại; quá RESUME_GRACE thì báo lỗi và xoá.
/// Máy gửi đang tạm dừng thì giữ tới khi tiếp tục (giới hạn bởi `PAUSE_HOLD`, xem `pause`).
/// Lượt nhận bị huỷ hoặc kết thúc trong lúc chờ thì xoá phần đã nhận ngay.
fn wait_for_resume(st: Arc<AppState>, session: Arc<InSession>, id: String, part: PathBuf) {
    let at = Instant::now();
    session.interrupted.lock().unwrap().insert(id.clone(), at);
    if session.tracker.status() != TransferStatus::Paused {
        session
            .tracker
            .set_note(Some("Mất kết nối, đang chờ máy gửi nối lại…".into()));
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let still_waiting = session.interrupted.lock().unwrap().get(&id) == Some(&at);
            if !still_waiting || session.active.lock().unwrap().contains(&id) {
                return;
            }
            let tracker = &session.tracker;
            let status = tracker.status();
            if status.is_final() {
                session.interrupted.lock().unwrap().remove(&id);
                let _ = tokio::fs::remove_file(&part).await;
                return;
            }
            if status == TransferStatus::Paused {
                continue;
            }
            let grace = Duration::from_millis(st.resume_grace_ms.load(Ordering::Relaxed));
            let since = tracker.resumed_at().map_or(at, |r| r.max(at));
            if since.elapsed() >= grace {
                session.interrupted.lock().unwrap().remove(&id);
                let _ = tokio::fs::remove_file(&part).await;
                tracker.set_note(None);
                tracker.set_status(TransferStatus::Error, Some("Mất kết nối với máy gửi".into()));
                return;
            }
        }
    });
}

/// Máy gửi tạm dừng / tiếp tục. Không biết phiên thì trả 410 (404 là máy nhận bản cũ,
/// chưa có đường dẫn này).
async fn pause(State(st): St, Query(q): Query<PauseQuery>) -> StatusCode {
    #[cfg(test)]
    if st.legacy_no_pause.load(Ordering::Relaxed) {
        return StatusCode::NOT_FOUND;
    }
    let Some(session) = st.incoming.lock().unwrap().get(&q.session).cloned() else {
        return StatusCode::GONE;
    };
    let tracker = session.tracker.clone();
    if !q.paused {
        if tracker.status() == TransferStatus::Paused {
            tracker.set_status(TransferStatus::Transferring, None);
        }
        return StatusCode::OK;
    }
    if tracker.status() == TransferStatus::Paused {
        return StatusCode::OK;
    }
    if !tracker.set_status(TransferStatus::Paused, None) {
        return StatusCode::GONE;
    }
    tracker.set_note(None);
    // Máy gửi biến mất khi đang tạm dừng: đừng giữ phiên (và các file .part) mãi.
    let generation = tracker.pause_gen();
    let hold = Duration::from_millis(st.pause_hold_ms.load(Ordering::Relaxed));
    tokio::spawn(async move {
        tokio::time::sleep(hold).await;
        if tracker.status() == TransferStatus::Paused && tracker.pause_gen() == generation {
            tracker.set_status(TransferStatus::Error, Some("Máy gửi tạm dừng quá lâu".into()));
        }
    });
    StatusCode::OK
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
        match tokio::fs::File::create(part).await {
            // Thư mục đã tạo trong phiên bị xoá mất giữa chừng: tạo lại như trước.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = part.parent() {
                    tokio::fs::create_dir_all(dir).await.map_err(io)?;
                }
                tokio::fs::File::create(part).await.map_err(io)?
            }
            r => r.map_err(io)?,
        }
    } else {
        let f = tokio::fs::OpenOptions::new().write(true).open(part).await.map_err(io)?;
        f.set_len(offset).await.map_err(io)?;
        let mut f = f;
        tokio::io::AsyncSeekExt::seek(&mut f, std::io::SeekFrom::Start(offset)).await.map_err(io)?;
        f
    };
    // Đệm 1 MB cho file lớn; file nhỏ chỉ cần đệm vừa đủ (khỏi cấp phát 1 MB mỗi file).
    let cap = (expected - offset).clamp(8 << 10, 1 << 20) as usize;
    let mut w = tokio::io::BufWriter::with_capacity(cap, f);
    let mut stream = body.into_data_stream();
    let mut written: u64 = offset;
    let mut added: u64 = 0;
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
        added += chunk.len() as u64;
    };
    // Cộng một lần khi xong thay vì khoá map mỗi chunk. Không ai đọc giữa chừng: lượt gửi
    // tiếp của cùng file bị `active` chặn tới khi lượt này kết thúc.
    *session.counted.lock().unwrap().entry(id.to_string()).or_insert(0) += added;
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
