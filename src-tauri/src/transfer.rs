//! Phía gửi: gom file, xin phép máy nhận rồi stream dữ liệu.

use crate::client::{api_url, build_client, describe_error, Pin};
use crate::models::*;
use crate::paths::to_wire_path;
use crate::state::{AppState, Tracker};
use futures_util::{StreamExt, TryStreamExt};
use reqwest::StatusCode;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::io::ReaderStream;

const PARALLEL_UPLOADS: usize = 3;
const SKIP_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

pub struct LocalFile {
    pub meta: FileMeta,
    pub path: PathBuf,
}

fn file_name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into())
}

/// Gom danh sách file từ các đường dẫn được chọn/kéo thả. Thư mục được duyệt
/// đệ quy và giữ nguyên cấu trúc bên trong.
pub fn collect_files(paths: &[String]) -> Result<Vec<LocalFile>, String> {
    let mut out = Vec::new();
    let mut push = |path: PathBuf, rel: Vec<String>| -> Result<(), String> {
        let size = std::fs::metadata(&path)
            .map_err(|e| format!("Không đọc được {}: {e}", path.display()))?
            .len();
        out.push(LocalFile {
            meta: FileMeta {
                id: uuid::Uuid::new_v4().simple().to_string(),
                rel_path: to_wire_path(&rel),
                size,
            },
            path,
        });
        Ok(())
    };

    for raw in paths {
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
    if out.is_empty() {
        return Err("Không có file nào để gửi".into());
    }
    Ok(out)
}

fn peer_client(peer: &Peer) -> Result<reqwest::Client, String> {
    build_client(Pin::Fingerprint(peer.info.fp.clone()))
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
    Err(last_err)
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
    st.outgoing.lock().unwrap().insert(tracker.id(), tracker.clone());
    tracker
}

pub async fn send_text(st: Arc<AppState>, peer: Peer, text: String) -> Result<(), String> {
    let tracker = new_tracker(&st, &peer, TransferKind::Text, 0, text.len() as u64);
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Text,
        files: vec![],
        text: Some(text),
    };
    let result = async {
        let client = peer_client(&peer)?;
        let (_, resp) = send_prepare(&client, &peer, &req).await?;
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

/// Bắt đầu gửi file; chạy nền và trả về id lượt chuyển ngay.
pub fn start_send_files(st: Arc<AppState>, peer: Peer, files: Vec<LocalFile>) -> String {
    let total: u64 = files.iter().map(|f| f.meta.size).sum();
    let tracker = new_tracker(&st, &peer, TransferKind::Files, files.len(), total);
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
    let client = peer_client(peer)?;
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Files,
        files: files.iter().map(|f| f.meta.clone()).collect(),
        text: None,
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
        s => return Err(format!("Máy nhận trả lỗi {s}")),
    }
    let prep: PrepareResponse = resp.json().await.map_err(|e| e.to_string())?;
    tracker.set_status(TransferStatus::Transferring, None);

    let result = futures_util::stream::iter(files.into_iter().map(Ok))
        .try_for_each_concurrent(PARALLEL_UPLOADS, |f| {
            upload_one(&client, &addr, &prep, f, tracker)
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

async fn upload_one(
    client: &reqwest::Client,
    addr: &str,
    prep: &PrepareResponse,
    f: LocalFile,
    tracker: &Arc<Tracker>,
) -> Result<(), String> {
    let token = prep
        .tokens
        .get(&f.meta.id)
        .ok_or("Máy nhận không cấp token cho file")?;
    let file = tokio::fs::File::open(&f.path)
        .await
        .map_err(|e| format!("Không mở được {}: {e}", f.path.display()))?;
    let t = tracker.clone();
    let stream = ReaderStream::with_capacity(file, 256 * 1024).map(move |chunk| {
        if t.is_cancelled() {
            return Err(std::io::Error::other("cancelled"));
        }
        if let Ok(b) = &chunk {
            t.add(b.len() as u64);
        }
        chunk
    });
    let resp = client
        .post(api_url(addr, "upload"))
        .query(&[
            ("session", prep.session_id.as_str()),
            ("file", f.meta.id.as_str()),
            ("token", token.as_str()),
        ])
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .map_err(|e| describe_error(&e))?;
    match resp.status() {
        s if s.is_success() => Ok(()),
        StatusCode::GONE => {
            tracker.set_status(TransferStatus::Cancelled, Some("Máy nhận đã huỷ".into()));
            Err("Máy nhận đã huỷ".into())
        }
        s => {
            let msg = resp.text().await.unwrap_or_default();
            Err(if msg.is_empty() { format!("Máy nhận trả lỗi {s}") } else { msg })
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
