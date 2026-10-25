//! Windows: mục "Gửi bằng BetterDrop" trong menu chuột phải của Explorer.
//!
//! Bộ cài (NSIS hoặc MSI) tạo mục cơ bản dưới `HKCU\Software\Classes\{*,Directory}\shell\BetterDrop`
//! và xoá khi gỡ cài đặt. Khi chạy, app ghi lại mục đó thành menu con có danh sách máy
//! ("Gửi bằng BetterDrop › Máy A / Máy B / Chọn máy khác…"): mỗi máy chạy
//! `BetterDrop.exe --send-to <id> "<file>"`.
//!
//! Phần sinh nội dung registry và chọn máy là hàm thuần (test chạy được trên Mac);
//! chỉ phần ghi registry nằm trong `#[cfg(windows)]`.

use crate::devices::{DeviceBook, DeviceRef};
use crate::models::Peer;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Tham số dòng lệnh: gửi luôn các file theo sau tới máy có id này.
pub const SEND_TO_ARG: &str = "--send-to";

/// Các khoá gốc của menu: file (`*`) và thư mục.
pub const ROOTS: [&str; 2] = [r"Software\Classes\*\shell\BetterDrop", r"Software\Classes\Directory\shell\BetterDrop"];

/// Menu con dài quá thì khó chọn: chỉ hiện chừng này máy.
pub const MAX_DEVICES: usize = 8;

/// Chờ các tiến trình khác của cùng một lần chọn nhiều file (Explorer chạy app một lần cho
/// mỗi file), và chờ tìm thấy máy nhận khi app vừa được mở.
const BATCH_WINDOW: Duration = Duration::from_millis(800);
const PEER_WAIT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegValue {
    Str(String),
    Dword(u32),
}

/// Một khoá registry cần ghi (đường dẫn dưới HKCU) cùng các giá trị; tên "" là giá trị mặc định.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegKey {
    pub path: String,
    pub values: Vec<(&'static str, RegValue)>,
}

/// id máy được đưa vào dòng lệnh, nên chỉ nhận chữ, số, `-` và `_`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Tên trên menu: `&` là phím tắt của menu Windows nên phải nhân đôi; cắt bớt tên quá dài.
pub fn menu_label(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    let short: String = if name.chars().count() > 40 {
        name.chars().take(39).chain(['…']).collect()
    } else {
        name.to_string()
    };
    short.replace('&', "&&")
}

/// Các máy hiện trên menu: máy đang thấy, máy đã lưu, máy tin cậy, máy trong nhóm và máy đã
/// thấy trong lần chạy này (để máy vừa tắt không biến mất khỏi menu). Bỏ máy đã ẩn, xếp theo tên.
pub fn pick_devices(peers: &[Peer], book: &DeviceBook, seen: &[DeviceRef]) -> Vec<DeviceRef> {
    let mut out: Vec<DeviceRef> = Vec::new();
    let mut add = |d: DeviceRef| {
        if !valid_id(&d.id) || book.is_hidden(&d.id) || d.name.trim().is_empty() {
            return;
        }
        if !out.iter().any(|o| o.id == d.id) {
            out.push(d);
        }
    };
    let as_ref = |p: &Peer| DeviceRef { id: p.info.id.clone(), name: p.info.name.clone(), os: p.info.os.clone() };
    // Tên mới nhất đứng trước: máy đang thấy, rồi tới các danh sách đã lưu.
    peers.iter().map(as_ref).for_each(&mut add);
    book.known.iter().map(as_ref).for_each(&mut add);
    seen.iter().cloned().for_each(&mut add);
    book.trusted
        .iter()
        .map(|t| DeviceRef { id: t.id.clone(), name: t.name.clone(), os: t.os.clone() })
        .for_each(&mut add);
    book.groups.iter().flat_map(|g| g.members.iter().cloned()).for_each(&mut add);
    out.sort_by_key(|d| d.name.to_lowercase());
    out.truncate(MAX_DEVICES);
    out
}

/// Nội dung registry của menu. Không có máy nào thì chỉ một mục mở app với các file đã chọn.
pub fn menu_keys(exe: &str, devices: &[DeviceRef], english: bool) -> Vec<RegKey> {
    let tr = |vi: &str, en: &str| if english { en } else { vi }.to_string();
    let s = |v: &str| RegValue::Str(v.to_string());
    let open_cmd = format!("\"{exe}\" \"%1\"");
    let mut keys = Vec::new();
    for root in ROOTS {
        let mut top = vec![
            ("MUIVerb", RegValue::Str(tr("Gửi bằng BetterDrop", "Send with BetterDrop"))),
            ("Icon", RegValue::Str(format!("\"{exe}\",0"))),
            // Chọn nhiều file vẫn hiện mục này (mặc định Windows chỉ cho tới 15 file).
            ("MultiSelectModel", s("Player")),
        ];
        let devices: Vec<&DeviceRef> = devices.iter().filter(|d| valid_id(&d.id)).collect();
        if devices.is_empty() {
            keys.push(RegKey { path: root.to_string(), values: top });
            keys.push(RegKey { path: format!(r"{root}\command"), values: vec![("", RegValue::Str(open_cmd.clone()))] });
            continue;
        }
        // SubCommands rỗng + khoá con `shell`: menu con (Windows xếp mục theo tên khoá).
        top.push(("SubCommands", s("")));
        keys.push(RegKey { path: root.to_string(), values: top });
        for (i, d) in devices.iter().enumerate() {
            let item = format!(r"{root}\shell\{i:02}");
            keys.push(RegKey {
                path: item.clone(),
                values: vec![("MUIVerb", RegValue::Str(menu_label(&d.name))), ("MultiSelectModel", s("Player"))],
            });
            keys.push(RegKey {
                path: format!(r"{item}\command"),
                values: vec![("", RegValue::Str(format!("\"{exe}\" {SEND_TO_ARG} {} \"%1\"", d.id)))],
            });
        }
        let other = format!(r"{root}\shell\99other");
        keys.push(RegKey {
            path: other.clone(),
            values: vec![
                ("MUIVerb", RegValue::Str(tr("Chọn máy khác…", "Choose another device…"))),
                ("MultiSelectModel", s("Player")),
                // ECF_SEPARATORBEFORE: kẻ ngang phía trên mục này.
                ("CommandFlags", RegValue::Dword(0x20)),
            ],
        });
        keys.push(RegKey { path: format!(r"{other}\command"), values: vec![("", RegValue::Str(open_cmd.clone()))] });
    }
    keys
}

// ---- Cập nhật menu theo danh sách máy ----

/// Máy đã thấy trong lần chạy này, menu đang ghi (để không ghi lại khi không đổi).
static SEEN: Mutex<Vec<DeviceRef>> = Mutex::new(Vec::new());
static WRITTEN: Mutex<Option<Vec<RegKey>>> = Mutex::new(None);
static SCHEDULED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Danh sách máy đổi (hoặc ngôn ngữ đổi): ghi lại menu sau một chút, gộp các lần gọi liền nhau.
/// Chạy trên luồng riêng vì event có thể phát ra khi đang giữ khoá trạng thái.
pub fn refresh(app: &AppHandle) {
    if !cfg!(windows) || SCHEDULED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        SCHEDULED.store(false, std::sync::atomic::Ordering::SeqCst);
        refresh_now(&app);
    });
}

fn refresh_now(app: &AppHandle) {
    let Some(st) = app.try_state::<Arc<crate::state::AppState>>() else { return };
    let peers = st.peer_list();
    let book = st.devices.lock().unwrap().clone();
    let english = st.settings.read().unwrap().english();
    let devices = {
        let mut seen = SEEN.lock().unwrap();
        for p in &peers {
            seen.retain(|d| d.id != p.info.id);
            seen.push(DeviceRef { id: p.info.id.clone(), name: p.info.name.clone(), os: p.info.os.clone() });
        }
        pick_devices(&peers, &book, &seen)
    };
    let Ok(exe) = std::env::current_exe() else { return };
    let keys = menu_keys(&exe.to_string_lossy(), &devices, english);
    let mut written = WRITTEN.lock().unwrap();
    if written.as_ref() == Some(&keys) {
        return;
    }
    #[cfg(windows)]
    if !write_registry(&keys) {
        return;
    }
    *written = Some(keys);
}

/// Ghi menu vào registry. Chỉ khi bộ cài đã tạo mục cơ bản (gỡ cài đặt thì xoá, nên app
/// không để lại khoá rác), và không ghi khi chạy bản dev hay chạy thử nhiều bản bằng profile.
#[cfg(windows)]
fn write_registry(keys: &[RegKey]) -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    if cfg!(debug_assertions) || crate::identity::profile().is_some() {
        return false;
    }
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    if ROOTS.iter().any(|r| hkcu.open_subkey(r).is_err()) {
        return false;
    }
    // Xoá phần cũ (mục đơn hoặc menu con) nhưng giữ khoá gốc của bộ cài.
    for root in ROOTS {
        let _ = hkcu.delete_subkey_all(format!(r"{root}\command"));
        let _ = hkcu.delete_subkey_all(format!(r"{root}\shell"));
        if let Ok(k) = hkcu.open_subkey_with_flags(root, winreg::enums::KEY_SET_VALUE) {
            let _ = k.delete_value("SubCommands");
        }
    }
    let write = || -> std::io::Result<()> {
        for k in keys {
            let (key, _) = hkcu.create_subkey(&k.path)?;
            for (name, v) in &k.values {
                match v {
                    RegValue::Str(s) => key.set_value(name, s)?,
                    RegValue::Dword(d) => key.set_value(name, d)?,
                }
            }
        }
        Ok(())
    };
    write().is_ok()
}

// ---- `BetterDrop.exe --send-to <id> <file>` ----

/// Các file chờ gửi theo từng máy. Chọn nhiều file thì Explorer chạy app một lần cho mỗi file
/// (single-instance chuyển về bản đang chạy), nên gom lại một lúc rồi mới gửi một lượt.
static QUEUE: Mutex<Queue> = Mutex::new(Queue { last_add: None, items: Vec::new() });

struct Queue {
    /// Lúc thêm file gần nhất; None = chưa có lượt gom nào đang chờ.
    last_add: Option<Instant>,
    items: Vec<(String, Vec<String>)>,
}

pub fn queue_send(app: &AppHandle, id: String, paths: Vec<String>) {
    let first = {
        let mut q = QUEUE.lock().unwrap();
        let first = q.last_add.is_none();
        q.last_add = Some(Instant::now());
        match q.items.iter_mut().find(|(i, _)| *i == id) {
            Some((_, list)) => {
                for p in paths {
                    if !list.contains(&p) {
                        list.push(p);
                    }
                }
            }
            None => q.items.push((id, paths)),
        }
        first
    };
    if !first {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Gửi khi đã một lúc không có file mới (app vừa mở thì các tiến trình khác tới chậm hơn).
        let batches = loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let mut q = QUEUE.lock().unwrap();
            if q.last_add.is_some_and(|t| t.elapsed() >= BATCH_WINDOW) {
                q.last_add = None;
                break std::mem::take(&mut q.items);
            }
        };
        for (id, paths) in batches {
            send_now(&app, id, paths).await;
        }
    });
}

/// Gửi tới máy `id` nếu máy đang bật; không thì để file sẵn trong danh sách gửi và báo.
async fn send_now(app: &AppHandle, id: String, paths: Vec<String>) {
    let Some(st) = app.try_state::<Arc<crate::state::AppState>>().map(|s| s.inner().clone()) else { return };
    let english = st.settings.read().unwrap().english();
    let tr = |vi: String, en: String| if english { en } else { vi };
    // App vừa mở thì cần vài giây để tìm thấy máy trong mạng.
    let started = Instant::now();
    let peer = loop {
        if let Some(p) = st.find_peer(&id).filter(|p| p.online) {
            break Some(p);
        }
        if started.elapsed() >= PEER_WAIT {
            break None;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let fallback = |title: String, body: String| {
        st.add_shared(paths.clone(), None);
        crate::tray::show_main(app);
        crate::tray::notify(app, &title, &body);
    };
    let Some(peer) = peer else {
        let book = st.devices.lock().unwrap().clone();
        let name = pick_devices(&[], &book, &SEEN.lock().unwrap())
            .into_iter()
            .find(|d| d.id == id)
            .map(|d| d.name)
            .unwrap_or_else(|| tr("Máy đã chọn".into(), "The chosen device".into()));
        fallback(
            tr(format!("{name} đang không bật"), format!("{name} isn't available")),
            tr(
                "Các mục đã nằm sẵn trong danh sách gửi — chọn máy khác hoặc thử lại khi máy bật.".into(),
                "Your items are in the send list — pick another device or try again when it's on.".into(),
            ),
        );
        return;
    };
    let state = st.clone();
    let to_collect = paths.clone();
    let files = tauri::async_runtime::spawn_blocking(move || crate::transfer::collect_files(&state, &to_collect))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    match files {
        Ok(files) => {
            crate::transfer::start_send_files(st.clone(), peer, files);
        }
        Err(e) => fallback(
            tr(format!("Không gửi được tới {}", peer.info.name), format!("Couldn't send to {}", peer.info.name)),
            e,
        ),
    }
}
