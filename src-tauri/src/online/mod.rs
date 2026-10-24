//! "Gửi qua Internet" (kiểu WeTransfer): mã hoá trên máy, đưa lên một dịch vụ lưu file
//! tạm không cần tài khoản, rồi tạo link
//! `https://gmttuts.github.io/better-drop/nhan/#1.<dịch vụ>.<mã file>.<khoá>`.
//!
//! Khoá nằm sau `#` nên trình duyệt không bao giờ gửi nó lên máy chủ nào. Người nhận mở
//! link bằng trình duyệt (trang `website/nhan/`) hoặc dán vào app.

pub mod crypto;
pub mod package;
pub mod providers;
pub mod zip;
#[cfg(test)]
mod e2e_tests;

use crate::history::{HistoryEntry, HistoryFile, OnlineLink};
use crate::models::*;
use crate::state::{AppState, Tracker};
use package::{Item, Opener, Package};
use providers::{Provider, ProviderInfo, Upload};
use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const RECEIVE_PAGE: &str = "https://gmttuts.github.io/better-drop/nhan/";
/// Phiên bản định dạng link (đổi khi đổi cách mã hoá/đóng gói).
const LINK_VERSION: &str = "1";

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub provider: Provider,
    pub id: String,
    pub key: crypto::Key,
}

impl Link {
    pub fn to_url(&self) -> String {
        format!(
            "{RECEIVE_PAGE}#{LINK_VERSION}.{}.{}.{}",
            self.provider.code(),
            self.id,
            crypto::key_to_b64(&self.key)
        )
    }

    /// Nhận cả link đầy đủ lẫn riêng phần sau `#`.
    pub fn parse(s: &str) -> Result<Link, String> {
        const BAD: &str = "Link BetterDrop không hợp lệ (thiếu hoặc sai phần sau dấu #)";
        let s = s.trim();
        let frag = s.rsplit_once('#').map(|(_, f)| f).unwrap_or(s);
        let (version, rest) = frag.split_once('.').ok_or(BAD)?;
        if version != LINK_VERSION {
            return Err("Link được tạo bằng BetterDrop mới hơn. Hãy cập nhật app".into());
        }
        let (code, rest) = rest.split_once('.').ok_or(BAD)?;
        let (id, key) = rest.rsplit_once('.').ok_or(BAD)?;
        let provider = Provider::from_code(code).ok_or("Link dùng dịch vụ lưu file mà bản này chưa hỗ trợ")?;
        if !provider.valid_id(id) {
            return Err(BAD.into());
        }
        let key = crypto::key_from_b64(key).ok_or(BAD)?;
        Ok(Link { provider, id: id.to_string(), key })
    }
}

pub fn providers() -> Vec<ProviderInfo> {
    providers::ALL.iter().map(|p| p.info()).collect()
}

/// Kết quả gửi xong, báo lên giao diện qua event `online-done`.
#[derive(Debug, Clone, Serialize)]
pub struct Sent {
    pub transfer_id: String,
    pub link: String,
    pub provider: Provider,
    pub provider_name: String,
    pub page_url: String,
    pub expires_at: u64,
    pub qr: String,
    pub file_count: usize,
    pub name: String,
}

pub fn qr_svg(link: &str) -> Result<String, String> {
    crate::discovery::pair_svg(link)
}

fn opener(st: &Arc<AppState>) -> Opener {
    let st = st.clone();
    Arc::new(move |src: &str| st.open_source(src))
}

/// Mã khách cố định của máy này cho storage.to (lưu trong online.json).
fn visitor_token(st: &AppState) -> String {
    let path = st.data_dir.join("online.json");
    let saved = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["visitor"].as_str().map(String::from));
    saved.unwrap_or_else(|| {
        let t = format!("betterdrop-{}", uuid::Uuid::new_v4().simple());
        let _ = std::fs::create_dir_all(&st.data_dir);
        let _ = std::fs::write(&path, serde_json::json!({ "visitor": t }).to_string());
        t
    })
}

fn new_tracker(
    st: &AppState,
    direction: Direction,
    provider: Provider,
    file_count: usize,
    total: u64,
    dir_path: Option<String>,
) -> Arc<Tracker> {
    let tracker = Tracker::new(
        st.emit.clone(),
        TransferUpdate {
            id: uuid::Uuid::new_v4().to_string(),
            direction,
            peer_name: provider.info().name.to_string(),
            kind: TransferKind::Files,
            file_count,
            total_bytes: total,
            transferred: 0,
            speed_bps: 0.0,
            status: TransferStatus::Waiting,
            error: None,
            dir_path,
        },
    );
    // Nằm trong `outgoing` để nút Huỷ (cancel_transfer) dùng được.
    st.outgoing.lock().unwrap().insert(tracker.id(), tracker.clone());
    tracker
}

fn record(st: &AppState, tracker: &Tracker, files: Vec<HistoryFile>, online: Option<OnlineLink>) {
    let u = tracker.snapshot();
    (st.recorder())(HistoryEntry {
        id: u.id,
        finished_at: crate::history::now_secs(),
        direction: u.direction,
        peer_name: u.peer_name,
        peer_id: None,
        kind: u.kind,
        file_count: u.file_count,
        total_bytes: u.total_bytes,
        status: u.status,
        error: u.error,
        dir_path: u.dir_path,
        files,
        text: None,
        online,
    });
}

fn finish_err(tracker: &Tracker, e: String) {
    if tracker.is_cancelled() {
        return;
    }
    tracker.set_status(TransferStatus::Error, Some(e));
}

/// Bắt đầu gửi `paths` qua dịch vụ `provider`. Trả về mã lượt chuyển (dùng để huỷ).
pub async fn send(st: Arc<AppState>, paths: Vec<String>, provider: Provider) -> Result<String, String> {
    let state = st.clone();
    let files = tauri::async_runtime::spawn_blocking(move || crate::transfer::collect_files(&state, &paths))
        .await
        .map_err(|e| e.to_string())??;
    let info = provider.info();
    let items: Vec<Item> = files
        .iter()
        .map(|f| Item { source: f.source.clone(), path: f.meta.rel_path.clone(), size: f.meta.size })
        .collect();
    let raw: u64 = items.iter().map(|i| i.size).sum();
    // Ước lượng (zip + mã hoá thêm rất ít) để báo lỗi ngay, trước khi đọc file.
    if crypto::encrypted_len(raw, crypto::CHUNK) > info.max_bytes {
        return Err(format!(
            "Quá giới hạn của {}: tối đa {} GB mỗi lượt gửi",
            info.name,
            info.max_bytes / 1_000_000_000
        ));
    }
    let tracker = new_tracker(&st, Direction::Send, provider, items.len(), crypto::encrypted_len(raw, crypto::CHUNK), None);
    let id = tracker.id();
    let history_files: Vec<HistoryFile> = files
        .iter()
        .take(crate::history::MAX_FILES)
        .map(|f| HistoryFile { name: f.meta.rel_path.clone(), path: Some(f.source.clone()) })
        .collect();
    tauri::async_runtime::spawn(async move {
        let res = run_send(&st, &tracker, items, provider).await;
        let online = match res {
            Ok(sent) => {
                tracker.set_note(None);
                tracker.set_status(TransferStatus::Done, None);
                let link = OnlineLink {
                    url: sent.link.clone(),
                    provider: provider.info().name.to_string(),
                    expires_at: sent.expires_at,
                };
                st.emit("online-done", &sent);
                Some(link)
            }
            Err(e) => {
                finish_err(&tracker, e);
                None
            }
        };
        record(&st, &tracker, history_files, online);
        st.outgoing.lock().unwrap().remove(&tracker.id());
    });
    Ok(id)
}

async fn run_send(st: &Arc<AppState>, tracker: &Arc<Tracker>, items: Vec<Item>, provider: Provider) -> Result<Sent, String> {
    let info = provider.info();
    let file_count = items.len();
    if file_count > 1 {
        tracker.set_note(Some("Đang chuẩn bị…".into()));
    }
    let open = opener(st);
    let t = tracker.clone();
    let open2 = open.clone();
    let pkg = tauri::async_runtime::spawn_blocking(move || Package::build(items, &open2, &|| t.is_cancelled()))
        .await
        .map_err(|e| e.to_string())??;
    tracker.set_note(None);
    let key = crypto::random_key();
    let len = crypto::encrypted_len(pkg.plain_len, crypto::CHUNK);
    if len > info.max_bytes {
        return Err(format!("Quá giới hạn của {}: tối đa {} GB", info.name, info.max_bytes / 1_000_000_000));
    }
    let source: providers::Source =
        Arc::new(Mutex::new(Box::new(crypto::Encryptor::new(pkg.reader(open), &key)) as Box<dyn Read + Send>));
    let upload = Upload {
        client: providers::internet_client()?,
        source,
        len,
        tracker: tracker.clone(),
        visitor: visitor_token(st),
    };
    let up = providers::upload(provider, &upload).await?;
    let link = Link { provider, id: up.id.clone(), key }.to_url();
    Ok(Sent {
        transfer_id: tracker.id(),
        qr: qr_svg(&link)?,
        link,
        provider,
        provider_name: info.name.to_string(),
        page_url: provider.page_url(&up.id),
        expires_at: up.expires_at,
        file_count,
        name: pkg.manifest.name.clone(),
    })
}

/// Trả lời khi dán link: đã bắt đầu tải về, hoặc người nhận phải tự tải file .bdrop
/// từ trang của dịch vụ rồi chọn file đó.
#[derive(Debug, Clone, Serialize)]
pub struct ReceiveStart {
    pub transfer_id: Option<String>,
    pub provider_name: String,
    pub page_url: String,
}

/// Đếm byte đã đọc vào tiến trình (lượt nhận chỉ được tạo sau khi đọc xong manifest,
/// nên phần đọc trước đó được cộng dồn rồi báo sau); dừng khi bị huỷ.
struct Counting<R> {
    inner: R,
    slot: Arc<std::sync::OnceLock<Arc<Tracker>>>,
    early: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let tracker = self.slot.get();
        if tracker.is_some_and(|t| t.is_cancelled()) {
            return Err(std::io::Error::other("Đã huỷ"));
        }
        let n = self.inner.read(buf)?;
        match tracker {
            Some(t) => t.add(std::mem::take(&mut self.early) + n as u64),
            None => self.early += n as u64,
        }
        Ok(n)
    }
}

type Reader = crypto::Decryptor<Counting<Box<dyn Read + Send>>>;

fn download_dir(st: &AppState) -> PathBuf {
    PathBuf::from(st.settings.read().unwrap().download_dir.clone())
}

/// Đọc phần đầu (kiểm tra khoá và lấy manifest) ngay, để sai khoá thì báo lỗi luôn;
/// phần còn lại tải/giải mã trong nền như một lượt nhận bình thường.
async fn start_receive(
    st: Arc<AppState>,
    link: Link,
    src: Box<dyn Read + Send>,
    cipher_len: Option<u64>,
) -> Result<String, String> {
    let slot: Arc<std::sync::OnceLock<Arc<Tracker>>> = Arc::default();
    let counting = Counting { inner: src, slot: slot.clone(), early: 0 };
    let key = link.key;
    let (reader, manifest) = tauri::async_runtime::spawn_blocking(move || {
        let mut r: Reader = crypto::Decryptor::new(counting, &key);
        package::read_manifest(&mut r).map(|m| (r, m)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let dest = download_dir(&st);
    let total = cipher_len.unwrap_or_else(|| crypto::encrypted_len(manifest.size, crypto::CHUNK));
    let dir = Some(dest.to_string_lossy().into_owned());
    let tracker = new_tracker(&st, Direction::Receive, link.provider, manifest.files.len(), total, dir);
    let _ = slot.set(tracker.clone());
    tracker.set_status(TransferStatus::Transferring, None);
    let id = tracker.id();
    tauri::async_runtime::spawn_blocking(move || finish_receive(&st, &tracker, &link, reader, manifest, &dest));
    Ok(id)
}

fn finish_receive(st: &Arc<AppState>, tracker: &Arc<Tracker>, link: &Link, mut reader: Reader, m: package::Manifest, dest: &std::path::Path) {
    let mut files = Vec::new();
    match package::unpack_with(&mut reader, m, dest) {
        Ok(saved) => {
            if let Some(f) = &st.hooks.file_saved {
                saved.files.iter().for_each(|p| f(p));
            }
            files = saved
                .files
                .iter()
                .zip(&saved.manifest.files)
                .take(crate::history::MAX_FILES)
                .map(|(p, m)| HistoryFile { name: m.path.clone(), path: Some(p.to_string_lossy().into_owned()) })
                .collect();
            tracker.set_status(TransferStatus::Done, None);
        }
        Err(e) => {
            let msg = if tracker.is_cancelled() { "Đã huỷ".to_string() } else { e.to_string() };
            finish_err(tracker, msg);
        }
    }
    let online = OnlineLink { url: link.to_url(), provider: link.provider.info().name.to_string(), expires_at: 0 };
    record(st, tracker, files, Some(online));
    st.outgoing.lock().unwrap().remove(&tracker.id());
}

/// Dán link vào app: tải về nếu dịch vụ cho phép, không thì báo trang để tự tải.
pub async fn receive_link(st: Arc<AppState>, link: String) -> Result<ReceiveStart, String> {
    let link = Link::parse(&link)?;
    let info = link.provider.info();
    let page_url = link.provider.page_url(&link.id);
    if !info.app_download {
        return Ok(ReceiveStart { transfer_id: None, provider_name: info.name.into(), page_url });
    }
    let client = providers::internet_client()?;
    let (resp, len) = providers::download(link.provider, &client, &link.id).await?;
    let reader: Box<dyn Read + Send> = Box::new(providers::response_reader(resp));
    let id = start_receive(st, link, reader, len).await?;
    Ok(ReceiveStart { transfer_id: Some(id), provider_name: info.name.into(), page_url })
}

/// Giải mã file .bdrop đã tải về bằng khoá trong link.
pub async fn receive_file(st: Arc<AppState>, link: String, path: String) -> Result<String, String> {
    let link = Link::parse(&link)?;
    let f = st.open_source(&path).map_err(|e| format!("Không mở được file: {e}"))?;
    let len = f.metadata().ok().map(|m| m.len()).filter(|l| *l > 0);
    start_receive(st, link, Box::new(std::io::BufReader::new(f)), len).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_round_trip_and_parse() {
        let key = crypto::random_key();
        let l = Link { provider: Provider::TempSh, id: "AbCdE/betterdrop.bdrop".into(), key };
        let url = l.to_url();
        assert!(url.starts_with("https://gmttuts.github.io/better-drop/nhan/#1.ts.AbCdE/betterdrop.bdrop."));
        assert_eq!(Link::parse(&url).unwrap(), l);
        // Chỉ phần sau dấu # cũng được, có khoảng trắng thừa cũng được.
        let frag = url.split_once('#').unwrap().1;
        assert_eq!(Link::parse(&format!("  {frag}\n")).unwrap(), l);

        let st = Link { provider: Provider::StorageTo, id: "FQxyz1234".into(), key };
        assert_eq!(Link::parse(&st.to_url()).unwrap(), st);
    }

    #[test]
    fn bad_links_are_rejected() {
        let k = crypto::key_to_b64(&crypto::random_key());
        assert!(Link::parse("https://example.com").is_err());
        assert!(Link::parse(&format!("#2.st.FQxyz1234.{k}")).unwrap_err().contains("mới hơn"));
        assert!(Link::parse(&format!("#1.zz.FQxyz1234.{k}")).is_err());
        assert!(Link::parse("#1.st.FQxyz1234.ngắn").is_err());
        assert!(Link::parse(&format!("#1.st.../../x.{k}")).is_err());
    }

    /// Tạo file .bdrop mẫu để thử trang nhận trên website (không chạy trong `cargo test` thường):
    /// `BDROP_SRC=<file hoặc thư mục> BDROP_OUT=<thư mục> cargo test make_sample -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn make_sample() {
        let src = PathBuf::from(std::env::var("BDROP_SRC").expect("thiếu BDROP_SRC"));
        let out = PathBuf::from(std::env::var("BDROP_OUT").expect("thiếu BDROP_OUT"));
        let root = src.file_name().unwrap().to_string_lossy().into_owned();
        let items: Vec<Item> = if src.is_dir() {
            walkdir::WalkDir::new(&src)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| {
                    let rel = e.path().strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
                    Item {
                        source: e.path().to_string_lossy().into_owned(),
                        path: format!("{root}/{rel}"),
                        size: e.metadata().unwrap().len(),
                    }
                })
                .collect()
        } else {
            vec![Item { source: src.to_string_lossy().into_owned(), path: root, size: src.metadata().unwrap().len() }]
        };
        let open: Opener = Arc::new(|p: &str| std::fs::File::open(p));
        let pkg = Package::build(items, &open, &|| false).unwrap();
        let key = crypto::random_key();
        std::fs::create_dir_all(&out).unwrap();
        let mut f = std::fs::File::create(out.join(providers::UPLOAD_NAME)).unwrap();
        std::io::copy(&mut crypto::Encryptor::new(pkg.reader(open), &key), &mut f).unwrap();
        let link = Link { provider: Provider::StorageTo, id: "SAMPLE1234".into(), key };
        println!("LINK {}", link.to_url());
    }
}
