//! Trạng thái dùng chung của app và bộ theo dõi tiến trình chuyển file.

use crate::identity::{self, Identity};
use crate::models::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// Hàm gửi event lên giao diện. Trong app là `AppHandle::emit`, trong test là một closure.
pub type EmitFn = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

/// Mở một file theo URI (`content://` trên Android, `file://` trên iOS).
pub type UriOpener = Arc<dyn Fn(&str) -> std::io::Result<std::fs::File> + Send + Sync>;

/// Phần việc khác nhau giữa các nền tảng, gắn vào từ `lib.rs`.
#[derive(Default, Clone)]
pub struct Hooks {
    pub open_uri: Option<UriOpener>,
    /// Tên và kích thước (nếu biết) của một URI do bộ chọn file trả về.
    pub uri_info: Option<Arc<dyn Fn(&str) -> std::io::Result<(String, Option<u64>)> + Send + Sync>>,
    /// Gọi sau khi lưu xong một file nhận được (Android: báo hệ thống quét file).
    pub file_saved: Option<Arc<dyn Fn(&std::path::Path) + Send + Sync>>,
}

pub struct AppState {
    pub emit: EmitFn,
    pub data_dir: PathBuf,
    pub identity: Identity,
    pub port: u16,
    pub settings: RwLock<Settings>,
    /// Yêu cầu nhận đang chờ người dùng trả lời.
    pub pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub incoming: Mutex<HashMap<String, Arc<InSession>>>,
    pub outgoing: Mutex<HashMap<String, Arc<Tracker>>>,
    /// Khoá: tên đầy đủ mDNS, hoặc "manual:<id>".
    pub peers: Mutex<HashMap<String, Peer>>,
    /// Giữ cho việc chọn tên file trống + đổi tên là một bước nguyên tử.
    pub rename_lock: Mutex<()>,
    pub mdns: Mutex<Option<mdns_sd::ServiceDaemon>>,
    pub hooks: Hooks,
}

impl AppState {
    pub fn new(emit: EmitFn, data_dir: PathBuf, identity: Identity, port: u16, settings: Settings) -> Self {
        AppState {
            emit,
            data_dir,
            identity,
            port,
            settings: RwLock::new(settings),
            pending: Mutex::default(),
            incoming: Mutex::default(),
            outgoing: Mutex::default(),
            peers: Mutex::default(),
            rename_lock: Mutex::default(),
            mdns: Mutex::default(),
            hooks: Hooks::default(),
        }
    }

    pub fn device_info(&self) -> DeviceInfo {
        let s = self.settings.read().unwrap();
        DeviceInfo {
            id: s.device_id.clone(),
            name: s.name.clone(),
            os: identity::os_name().to_string(),
            fp: self.identity.fingerprint.clone(),
            port: self.port,
        }
    }

    pub fn emit<T: serde::Serialize>(&self, event: &'static str, payload: T) {
        if let Ok(v) = serde_json::to_value(payload) {
            (self.emit)(event, v);
        }
    }

    /// Danh sách máy, gộp trùng theo id (ưu tiên bản tìm qua mDNS).
    pub fn peer_list(&self) -> Vec<Peer> {
        let peers = self.peers.lock().unwrap();
        let mut by_id: HashMap<String, Peer> = HashMap::new();
        for p in peers.values() {
            match by_id.get(&p.info.id) {
                Some(existing) if !existing.manual => {}
                _ => {
                    by_id.insert(p.info.id.clone(), p.clone());
                }
            }
        }
        let mut list: Vec<Peer> = by_id.into_values().collect();
        list.sort_by(|a, b| a.info.name.to_lowercase().cmp(&b.info.name.to_lowercase()));
        list
    }

    pub fn find_peer(&self, id: &str) -> Option<Peer> {
        self.peer_list().into_iter().find(|p| p.info.id == id)
    }

    /// Mở file nguồn để gửi: đường dẫn thường hoặc URI của điện thoại.
    pub fn open_source(&self, src: &str) -> std::io::Result<std::fs::File> {
        let is_uri = src.starts_with("content://") || src.starts_with("file://");
        match (&self.hooks.open_uri, is_uri) {
            (Some(open), true) => open(src),
            _ => std::fs::File::open(src),
        }
    }

    pub fn emit_peers(&self) {
        self.emit("peers-changed", self.peer_list());
    }
}

/// Theo dõi một lượt chuyển và báo tiến trình lên UI (tối đa ~7 lần/giây).
pub struct Tracker {
    emit: EmitFn,
    update: Mutex<TransferUpdate>,
    transferred: AtomicU64,
    started: Mutex<Option<Instant>>,
    last_emit: Mutex<Instant>,
    cancelled: AtomicBool,
}

const EMIT_INTERVAL: Duration = Duration::from_millis(150);

impl Tracker {
    pub fn new(emit: EmitFn, update: TransferUpdate) -> Arc<Self> {
        let t = Arc::new(Tracker {
            emit,
            update: Mutex::new(update),
            transferred: AtomicU64::new(0),
            started: Mutex::new(None),
            last_emit: Mutex::new(Instant::now()),
            cancelled: AtomicBool::new(false),
        });
        t.emit_now();
        t
    }

    pub fn id(&self) -> String {
        self.update.lock().unwrap().id.clone()
    }

    pub fn snapshot(&self) -> TransferUpdate {
        let mut u = self.update.lock().unwrap().clone();
        u.transferred = self.transferred.load(Ordering::Relaxed);
        if let Some(start) = *self.started.lock().unwrap() {
            let secs = start.elapsed().as_secs_f64();
            if secs > 0.0 {
                u.speed_bps = u.transferred as f64 / secs;
            }
        }
        u
    }

    pub fn emit_now(&self) {
        *self.last_emit.lock().unwrap() = Instant::now();
        if let Ok(v) = serde_json::to_value(self.snapshot()) {
            (self.emit)("transfer-update", v);
        }
    }

    pub fn add(&self, n: u64) {
        self.transferred.fetch_add(n, Ordering::Relaxed);
        let due = self.last_emit.lock().unwrap().elapsed() >= EMIT_INTERVAL;
        if due {
            self.emit_now();
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn status(&self) -> TransferStatus {
        self.update.lock().unwrap().status
    }

    /// Đổi trạng thái; bỏ qua nếu lượt chuyển đã kết thúc. Trả về true nếu đã đổi.
    pub fn set_status(&self, status: TransferStatus, error: Option<String>) -> bool {
        {
            let mut u = self.update.lock().unwrap();
            if u.status.is_final() {
                return false;
            }
            u.status = status;
            u.error = error;
        }
        if status == TransferStatus::Transferring {
            self.started.lock().unwrap().get_or_insert_with(Instant::now);
        }
        if status == TransferStatus::Cancelled {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        self.emit_now();
        true
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub struct InFile {
    pub rel: Vec<String>,
    pub size: u64,
    pub token: String,
}

/// Một phiên nhận file đã được chấp nhận.
pub struct InSession {
    pub tracker: Arc<Tracker>,
    pub files: HashMap<String, InFile>,
    pub base_dir: PathBuf,
    /// Thư mục gốc gửi tới -> tên thực tế trên đĩa (vd "Photos" -> "Photos (1)").
    pub root_map: Mutex<HashMap<String, String>>,
    pub started_files: Mutex<HashSet<String>>,
    pub done: AtomicUsize,
}

impl InSession {
    /// Đường dẫn đích (chưa đảm bảo duy nhất với file đơn lẻ; xử lý khi đổi tên).
    pub fn dest_path(&self, rel: &[String]) -> std::io::Result<PathBuf> {
        if rel.len() == 1 {
            std::fs::create_dir_all(&self.base_dir)?;
            return Ok(self.base_dir.join(&rel[0]));
        }
        let root = {
            let mut map = self.root_map.lock().unwrap();
            match map.get(&rel[0]) {
                Some(r) => r.clone(),
                None => {
                    let dir = crate::paths::unique_path(&self.base_dir.join(&rel[0]));
                    std::fs::create_dir_all(&dir)?;
                    let name = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| rel[0].clone());
                    map.insert(rel[0].clone(), name.clone());
                    name
                }
            }
        };
        let mut p = self.base_dir.join(root);
        for c in &rel[1..] {
            p.push(c);
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(p)
    }
}
