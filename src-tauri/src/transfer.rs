//! Phía gửi: gom file, xin phép máy nhận rồi stream dữ liệu.

use crate::client::{api_url, build_client, describe_error, Pin};
use crate::models::*;
use crate::paths::to_wire_path;
use crate::state::{AppState, Tracker};
use futures_util::{StreamExt, TryStreamExt};
use reqwest::StatusCode;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::io::ReaderStream;

/// Số "suất" gửi song song. File lớn chiếm `LARGE_COST` suất (tối đa 3 file lớn cùng
/// lúc, như trước); file nhỏ chiếm 1 suất, nên nhiều file nhỏ được gửi chồng lên nhau
/// thay vì mỗi file phải chờ trọn một vòng hỏi-đáp qua mạng.
const UPLOAD_SLOTS: u32 = 16;
const LARGE_COST: u32 = 5;
const LARGE_FILE: u64 = 1024 * 1024;
const RECEIVE_OFF: &str = "Máy nhận đang tắt chế độ nhận file";
const SKIP_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

pub struct LocalFile {
    pub meta: FileMeta,
    /// Đường dẫn, hoặc URI (`content://`, `file://`) trên điện thoại.
    pub source: String,
}

fn is_uri(s: &str) -> bool {
    s.starts_with("content://") || s.starts_with("file://")
}

/// Tên và kích thước của một URI do bộ chọn file trên điện thoại trả về.
fn uri_info(st: &AppState, uri: &str) -> Result<(String, u64), String> {
    let (name, size) = match &st.hooks.uri_info {
        Some(f) => f(uri).map_err(|e| e.to_string())?,
        None => (file_name_of(Path::new(uri)), None),
    };
    let size = match size {
        Some(s) => s,
        None => st
            .open_source(uri)
            .and_then(|f| f.metadata())
            .map(|m| m.len())
            .map_err(|e| format!("Không đọc được {name}: {e}"))?,
    };
    Ok((name, size))
}

/// Mô tả các mục vừa chọn để hiện trong danh sách chờ gửi.
pub fn describe(st: &AppState, paths: &[String]) -> Vec<StagedItem> {
    paths
        .iter()
        .map(|p| {
            if is_uri(p) {
                let (name, size) = uri_info(st, p)
                    .map(|(n, s)| (n, Some(s)))
                    .unwrap_or_else(|_| (file_name_of(Path::new(p)), None));
                return StagedItem { path: p.clone(), name, is_dir: false, size };
            }
            let meta = std::fs::metadata(p).ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            StagedItem {
                path: p.clone(),
                name: file_name_of(Path::new(p)),
                is_dir,
                size: meta.filter(|m| m.is_file()).map(|m| m.len()),
            }
        })
        .collect()
}

fn file_name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into())
}

/// Gom danh sách file từ các đường dẫn được chọn/kéo thả. Thư mục được duyệt
/// đệ quy và giữ nguyên cấu trúc bên trong.
pub fn collect_files(st: &AppState, paths: &[String]) -> Result<Vec<LocalFile>, String> {
    let mut out = Vec::new();
    let new_file = |source: String, rel: Vec<String>, size: u64| LocalFile {
        meta: FileMeta {
            id: uuid::Uuid::new_v4().simple().to_string(),
            rel_path: to_wire_path(&rel),
            size,
        },
        source,
    };
    let mut push = |path: PathBuf, rel: Vec<String>| -> Result<(), String> {
        let size = std::fs::metadata(&path)
            .map_err(|e| format!("Không đọc được {}: {e}", path.display()))?
            .len();
        out.push(new_file(path.to_string_lossy().into_owned(), rel, size));
        Ok(())
    };

    let mut uri_files = Vec::new();
    for raw in paths {
        if is_uri(raw) {
            let (name, size) = uri_info(st, raw)?;
            uri_files.push(new_file(raw.clone(), vec![name], size));
            continue;
        }
        let path = PathBuf::from(raw);
        let meta = std::fs::metadata(&path)
            .map_err(|e| format!("Không đọc được {}: {e}", path.display()))?;
        if meta.is_dir() {
            let root = file_name_of(&path);
            for entry in walkdir::WalkDir::new(&path).follow_links(false) {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.file_type().is_file()
                    || SKIP_NAMES.contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    continue;
                }
                let mut rel = vec![root.clone()];
                if let Ok(sub) = entry.path().strip_prefix(&path) {
                    rel.extend(sub.components().map(|c| c.as_os_str().to_string_lossy().into_owned()));
                }
                push(entry.into_path(), rel)?;
            }
        } else {
            push(path.clone(), vec![file_name_of(&path)])?;
        }
    }
    out.extend(uri_files);
    if out.is_empty() {
        return Err("Không có file nào để gửi".into());
    }
    Ok(out)
}

/// Client gửi tới `peer`, có trình chứng chỉ của máy này để máy nhận nhận ra mình.
fn peer_client(st: &AppState, peer: &Peer) -> Result<reqwest::Client, String> {
    build_client(Pin::Fingerprint(peer.info.fp.clone()), Some(&st.identity))
}

/// Gửi yêu cầu `prepare`, thử lần lượt các địa chỉ của máy nhận.
/// Trả về địa chỉ dùng được và phản hồi.
async fn send_prepare(
    client: &reqwest::Client,
    peer: &Peer,
    req: &PrepareRequest,
) -> Result<(String, reqwest::Response), String> {
    let mut last_err = "Máy nhận không có địa chỉ nào".to_string();
    for addr in &peer.addrs {
        let res = client
            .post(api_url(addr, "prepare"))
            .json(req)
            .timeout(crate::server::ACCEPT_TIMEOUT + Duration::from_secs(15))
            .send()
            .await;
        match res {
            Ok(r) => return Ok((addr.clone(), r)),
            Err(e) => last_err = describe_error(&e),
        }
    }
    Err(with_ios_hint(peer, last_err))
}

/// iOS dừng app vài giây sau khi chuyển ra màn hình khác, nên iPhone/iPad chỉ nhận
/// được khi BetterDrop đang mở. Nói rõ điều đó thay vì chỉ báo "không kết nối được".
pub const IOS_HINT: &str = "iPhone/iPad chỉ nhận được khi BetterDrop đang mở trên màn hình. Hãy mở app trên máy đó rồi gửi lại.";

fn with_ios_hint(peer: &Peer, err: String) -> String {
    if peer.info.os == "ios" {
        format!("{IOS_HINT} ({err})")
    } else {
        err
    }
}

fn new_tracker(st: &AppState, peer: &Peer, kind: TransferKind, file_count: usize, total: u64) -> Arc<Tracker> {
    let tracker = Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: uuid::Uuid::new_v4().to_string(),
            direction: Direction::Send,
            peer_name: peer.info.name.clone(),
            kind,
            file_count,
            total_bytes: total,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Waiting,
            error: None,
            dir_path: None,
        },
    );
    tracker.record_to(st.recorder(), Some(peer.info.id.clone()));
    st.outgoing.lock().unwrap().insert(tracker.id(), tracker.clone());
    tracker
}

pub async fn send_text(st: Arc<AppState>, peer: Peer, text: String) -> Result<(), String> {
    let tracker = new_tracker(&st, &peer, TransferKind::Text, 0, text.len() as u64);
    tracker.set_history_text(&text);
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Text,
        files: vec![],
        text: Some(text),
        group: None,
    };
    let result = async {
        let client = peer_client(&st, &peer)?;
        let (_, resp) = send_prepare(&client, &peer, &req).await?;
        if resp.status() == StatusCode::SERVICE_UNAVAILABLE {
            return Err(RECEIVE_OFF.into());
        }
        if !resp.status().is_success() {
            return Err(format!("Máy nhận trả lỗi {}", resp.status()));
        }
        Ok(())
    }
    .await;
    match &result {
        Ok(()) => tracker.set_status(TransferStatus::Done, None),
        Err(e) => tracker.set_status(TransferStatus::Error, Some(e.clone())),
    };
    result
}

/// Chia sẻ một nhóm máy cho `peer` (máy nhận phải bấm Chấp nhận).
pub async fn share_group(st: Arc<AppState>, peer: Peer, group: crate::models::SharedGroup) -> Result<(), String> {
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Group,
        files: vec![],
        text: None,
        group: Some(group),
    };
    let client = peer_client(&st, &peer)?;
    let (_, resp) = send_prepare(&client, &peer, &req).await?;
    match resp.status() {
        s if s.is_success() => Ok(()),
        StatusCode::FORBIDDEN => Err(format!("{} đã từ chối", peer.info.name)),
        StatusCode::SERVICE_UNAVAILABLE => Err(RECEIVE_OFF.into()),
        // Bản cũ không hiểu loại "group" nên không đọc được yêu cầu.
        StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST => Err(GROUP_TOO_OLD.into()),
        s => Err(format!("Máy nhận trả lỗi {s}")),
    }
}

pub const GROUP_TOO_OLD: &str = "Máy nhận cần cập nhật BetterDrop 0.3.5 trở lên để nhận nhóm";

/// Bắt đầu gửi file; chạy nền và trả về id lượt chuyển ngay.
pub fn start_send_files(st: Arc<AppState>, peer: Peer, files: Vec<LocalFile>) -> String {
    let total: u64 = files.iter().map(|f| f.meta.size).sum();
    let tracker = new_tracker(&st, &peer, TransferKind::Files, files.len(), total);
    for f in &files {
        tracker.add_history_file(f.meta.rel_path.clone(), Some(f.source.clone()));
    }
    let id = tracker.id();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = send_files(&st, &peer, files, &tracker).await {
            if !tracker.is_cancelled() {
                tracker.set_status(TransferStatus::Error, Some(e));
            }
        }
    });
    id
}

pub async fn send_files(
    st: &AppState,
    peer: &Peer,
    files: Vec<LocalFile>,
    tracker: &Arc<Tracker>,
) -> Result<(), String> {
    let client = peer_client(st, peer)?;
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Files,
        files: files.iter().map(|f| f.meta.clone()).collect(),
        text: None,
        group: None,
    };
    let (addr, resp) = send_prepare(&client, peer, &req).await?;
    if tracker.is_cancelled() {
        return Ok(());
    }
    match resp.status() {
        StatusCode::OK => {}
        StatusCode::FORBIDDEN => {
            tracker.set_status(TransferStatus::Rejected, Some("Máy nhận đã từ chối".into()));
            return Ok(());
        }
        StatusCode::SERVICE_UNAVAILABLE => {
            tracker.set_status(TransferStatus::Rejected, Some(RECEIVE_OFF.into()));
            return Ok(());
        }
        s => return Err(format!("Máy nhận trả lỗi {s}")),
    }
    let prep: PrepareResponse = resp.json().await.map_err(|e| e.to_string())?;
    tracker.set_status(TransferStatus::Transferring, None);

    let slots = tokio::sync::Semaphore::new(UPLOAD_SLOTS as usize);
    let result = futures_util::stream::iter(files.into_iter().map(Ok))
        .try_for_each_concurrent(UPLOAD_SLOTS as usize, |f| {
            let (slots, client, addr, prep) = (&slots, &client, &addr, &prep);
            async move {
                let cost = if f.meta.size >= LARGE_FILE { LARGE_COST } else { 1 };
                let _slot = slots.acquire_many(cost).await.map_err(|e| e.to_string())?;
                upload_one(st, client, addr, prep, f, tracker).await
            }
        })
        .await;

    match result {
        Ok(()) => {
            tracker.set_status(TransferStatus::Done, None);
        }
        Err(e) => {
            if tracker.is_cancelled() {
                // Báo máy nhận để nó dọn file dở dang.
                let _ = client
                    .post(api_url(&addr, "cancel"))
                    .query(&[("session", &prep.session_id)])
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await;
            } else {
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Thời gian chờ giữa các lần thử lại sau khi mất kết nối (tổng ~1,5 phút, ngắn hơn
/// thời gian máy nhận giữ phần đã nhận: `server::RESUME_GRACE`).
const RETRY_DELAYS: [u64; 7] = [2, 3, 5, 8, 13, 20, 30];
/// Không gửi được byte nào trong chừng này thì coi như mất kết nối.
const STALL: Duration = Duration::from_secs(30);

enum UploadError {
    /// Lỗi mạng: thử lại được.
    Network(String),
    /// Lỗi khác (máy nhận từ chối, huỷ, lỗi đĩa…): dừng.
    Fatal(String),
}

async fn upload_one(
    st: &AppState,
    client: &reqwest::Client,
    addr: &str,
    prep: &PrepareResponse,
    f: LocalFile,
    tracker: &Arc<Tracker>,
) -> Result<(), String> {
    let token = prep
        .tokens
        .get(&f.meta.id)
        .ok_or("Máy nhận không cấp token cho file")?
        .clone();
    // Số byte của file này đã tính vào tiến trình.
    let counted = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut offset = 0u64;
    let mut attempt = 0usize;
    loop {
        match upload_from(st, client, addr, prep, &f, &token, offset, tracker, &counted).await {
            Ok(()) => {
                // Chỉ xoá ghi chú khi file này đã phải thử lại: mỗi lần đổi ghi chú là
                // một lần báo lên giao diện, gọi cho từng file thì hàng nghìn file nhỏ
                // sẽ làm giao diện giật.
                if attempt > 0 {
                    tracker.set_note(None);
                }
                return Ok(());
            }
            Err(UploadError::Fatal(e)) => return Err(e),
            Err(UploadError::Network(e)) => {
                if tracker.is_cancelled() {
                    return Err(e);
                }
                let Some(delay) = RETRY_DELAYS.get(attempt) else {
                    return Err(e);
                };
                attempt += 1;
                tracker.set_note(Some(format!("Mất kết nối, đang thử lại (lần {attempt})…")));
                tokio::time::sleep(Duration::from_secs(*delay)).await;
                if tracker.is_cancelled() {
                    return Err(e);
                }
                // Hỏi máy nhận đã có bao nhiêu byte để gửi tiếp từ đó.
                match upload_status(client, addr, prep, &f, &token).await {
                    Ok(Some(status)) => {
                        if status.done {
                            tracker.set_note(None);
                            let c = counted.swap(f.meta.size, std::sync::atomic::Ordering::SeqCst);
                            tracker.add(f.meta.size.saturating_sub(c));
                            return Ok(());
                        }
                        offset = status.received.min(f.meta.size);
                        let c = counted.swap(offset, std::sync::atomic::Ordering::SeqCst);
                        if c > offset {
                            tracker.sub(c - offset);
                        } else {
                            tracker.add(offset - c);
                        }
                    }
                    // Máy nhận bản cũ (chưa hỗ trợ gửi tiếp) hoặc phiên đã hết hạn.
                    Ok(None) => return Err(e),
                    // Vẫn chưa kết nối lại được: thử lần sau.
                    Err(_) => continue,
                }
            }
        }
    }
}

/// Hỏi máy nhận tiến độ của một file. Ok(None): máy nhận không hỗ trợ gửi tiếp
/// (bản cũ) hoặc phiên đã kết thúc. Err: chưa kết nối được.
async fn upload_status(
    client: &reqwest::Client,
    addr: &str,
    prep: &PrepareResponse,
    f: &LocalFile,
    token: &str,
) -> Result<Option<crate::models::UploadStatus>, ()> {
    let resp = client
        .get(api_url(addr, "upload-status"))
        .query(&[("session", prep.session_id.as_str()), ("file", f.meta.id.as_str()), ("token", token)])
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|_| ())?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    resp.json().await.map(Some).map_err(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn upload_from(
    st: &AppState,
    client: &reqwest::Client,
    addr: &str,
    prep: &PrepareResponse,
    f: &LocalFile,
    token: &str,
    offset: u64,
    tracker: &Arc<Tracker>,
    counted: &Arc<std::sync::atomic::AtomicU64>,
) -> Result<(), UploadError> {
    use std::sync::atomic::Ordering::SeqCst;
    let file = tokio::task::block_in_place(|| st.open_source(&f.source))
        .map(tokio::fs::File::from_std)
        .map_err(|e| UploadError::Fatal(format!("Không mở được {}: {e}", f.meta.rel_path)))?;
    let mut file = file;
    if offset > 0 {
        tokio::io::AsyncSeekExt::seek(&mut file, std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| UploadError::Fatal(format!("Không đọc được {}: {e}", f.meta.rel_path)))?;
    }
    let t = tracker.clone();
    let c = counted.clone();
    let last_progress = Arc::new(Mutex::new(std::time::Instant::now()));
    let lp = last_progress.clone();
    let stream = ReaderStream::with_capacity(file, 256 * 1024).map(move |chunk| {
        if t.is_cancelled() {
            return Err(std::io::Error::other("cancelled"));
        }
        if let Ok(b) = &chunk {
            t.add(b.len() as u64);
            c.fetch_add(b.len() as u64, SeqCst);
            *lp.lock().unwrap() = std::time::Instant::now();
        }
        chunk
    });
    let mut query = vec![
        ("session", prep.session_id.clone()),
        ("file", f.meta.id.clone()),
        ("token", token.to_string()),
    ];
    if offset > 0 {
        query.push(("offset", offset.to_string()));
    }
    let send = client
        .post(api_url(addr, "upload"))
        .query(&query)
        .body(reqwest::Body::wrap_stream(stream))
        .send();
    // Kết nối "chết lặng" (mất Wi‑Fi không báo lỗi): không có byte nào đi trong STALL.
    let size = f.meta.size;
    let watchdog = async {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let idle = last_progress.lock().unwrap().elapsed();
            if counted.load(SeqCst) < size && idle > STALL {
                return;
            }
        }
    };
    let resp = tokio::select! {
        r = send => r.map_err(|e| UploadError::Network(describe_error(&e)))?,
        _ = watchdog => return Err(UploadError::Network("Không gửi được dữ liệu trong 30 giây".into())),
    };
    match resp.status() {
        s if s.is_success() => Ok(()),
        StatusCode::GONE => {
            tracker.set_status(TransferStatus::Cancelled, Some("Máy nhận đã huỷ".into()));
            Err(UploadError::Fatal("Máy nhận đã huỷ".into()))
        }
        // Máy nhận chưa thấy kết nối cũ bị đứt: thử lại sau.
        StatusCode::CONFLICT => Err(UploadError::Network("Máy nhận đang chờ kết nối cũ".into())),
        s => {
            let msg = resp.text().await.unwrap_or_default();
            Err(UploadError::Fatal(if msg.is_empty() { format!("Máy nhận trả lỗi {s}") } else { msg }))
        }
    }
}

/// Huỷ một lượt chuyển (gửi hoặc nhận) theo id.
pub fn cancel(st: &AppState, id: &str) {
    if let Some(t) = st.outgoing.lock().unwrap().get(id) {
        t.set_status(TransferStatus::Cancelled, Some("Đã huỷ".into()));
    }
    if let Some(s) = st.incoming.lock().unwrap().get(id) {
        s.tracker.set_status(TransferStatus::Cancelled, Some("Đã huỷ".into()));
    }
}
