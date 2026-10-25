//! Phím tắt toàn cục (chỉ máy tính): gửi clipboard tới máy gần nhất, mở BetterDrop.
//! Đọc/kiểm tra tổ hợp phím và chọn máy nhận là hàm thuần để test được.

use crate::history::HistoryEntry;
use crate::models::{Direction, Hotkeys, TransferStatus};
use crate::state::AppState;
use crate::tray::{self, t};
use std::sync::{Arc, Mutex};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// id của hai phím tắt đang đăng ký: (gửi clipboard, mở app).
static ACTIVE: Mutex<Option<(u32, u32)>> = Mutex::new(None);
/// Lỗi lần đăng ký gần nhất (vd tổ hợp bị app khác chiếm), hiện trong Cài đặt.
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Máy nhận khi bấm "Gửi clipboard tới máy gần nhất".
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Peer(String),
    /// Nhiều máy đang bật mà chưa gửi tới máy nào: để người dùng chọn.
    Choose,
    NoPeers,
}

/// Máy đang bật mà mình gửi tới gần đây nhất; chưa từng gửi thì máy đang bật duy nhất.
/// `online`: (id, tên) của máy đang bật. `history`: lịch sử, mới nhất trước.
pub fn pick_target(online: &[(String, String)], history: &[HistoryEntry]) -> Target {
    let recent = history
        .iter()
        .filter(|e| e.direction == Direction::Send && e.status == TransferStatus::Done)
        .filter_map(|e| e.peer_id.as_deref())
        .find(|id| online.iter().any(|(o, _)| o == id));
    match (recent, online) {
        (Some(id), _) => Target::Peer(id.to_string()),
        (None, []) => Target::NoPeers,
        (None, [(id, _)]) => Target::Peer(id.clone()),
        (None, _) => Target::Choose,
    }
}

/// Đọc tổ hợp phím ("CmdOrCtrl+Shift+D"). Phải có Ctrl, Alt hoặc ⌘ để không chặn mất phím gõ chữ.
pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    let sc: Shortcut = s.parse().map_err(|_| format!("Tổ hợp phím không hợp lệ: {s}"))?;
    if !sc.mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER) {
        return Err("Phím tắt cần có ít nhất một phím Ctrl, Alt hoặc ⌘".into());
    }
    Ok(sc)
}

/// Tên tổ hợp để hiện cho người dùng: "⌘⇧D" trên Mac, "Ctrl+Shift+D" trên Windows/Linux.
pub fn label(s: &str, mac: bool) -> String {
    let parts: Vec<String> = s
        .split('+')
        .map(|p| {
            let k = match p.to_lowercase().as_str() {
                "cmdorctrl" | "commandorcontrol" | "cmdorcontrol" | "commandorctrl" => ["⌘", "Ctrl"],
                "cmd" | "command" | "super" | "meta" => ["⌘", "Win"],
                "ctrl" | "control" => ["⌃", "Ctrl"],
                "alt" | "option" => ["⌥", "Alt"],
                "shift" => ["⇧", "Shift"],
                _ => {
                    let key = p.strip_prefix("Key").or_else(|| p.strip_prefix("Digit")).unwrap_or(p);
                    return key.to_string();
                }
            };
            k[usize::from(!mac)].to_string()
        })
        .collect();
    parts.join(if mac { "" } else { "+" })
}

/// Hai tổ hợp (gửi clipboard, mở app) cần đăng ký; None khi đang tắt phím tắt.
pub fn validate(hk: &Hotkeys) -> Result<Option<(Shortcut, Shortcut)>, String> {
    if !hk.enabled {
        return Ok(None);
    }
    let send = parse_shortcut(&hk.send_clipboard)?;
    let open = parse_shortcut(&hk.open_app)?;
    if send == open {
        return Err("Hai phím tắt đang trùng nhau".into());
    }
    Ok(Some((send, open)))
}

fn busy(s: &str) -> String {
    format!("Tổ hợp {} đang được app khác dùng. Hãy chọn tổ hợp khác.", label(s, cfg!(target_os = "macos")))
}

/// Đăng ký lại phím tắt theo cài đặt. Lỗi thì không phím tắt nào được giữ lại.
pub fn apply(app: &AppHandle, hk: &Hotkeys) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    *ACTIVE.lock().unwrap() = None;
    let res = (|| {
        let Some((send, open)) = validate(hk)? else { return Ok(()) };
        gs.register(send).map_err(|_| busy(&hk.send_clipboard))?;
        if gs.register(open).is_err() {
            let _ = gs.unregister(send);
            return Err(busy(&hk.open_app));
        }
        *ACTIVE.lock().unwrap() = Some((send.id(), open.id()));
        Ok(())
    })();
    *LAST_ERROR.lock().unwrap() = res.as_ref().err().cloned();
    res
}

/// Tạm bỏ phím tắt trong lúc người dùng bấm tổ hợp mới trong Cài đặt.
pub fn pause(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    *ACTIVE.lock().unwrap() = None;
}

pub fn last_error() -> Option<String> {
    LAST_ERROR.lock().unwrap().clone()
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            match *ACTIVE.lock().unwrap() {
                Some((send, _)) if shortcut.id() == send => send_nearest(app),
                Some((_, open)) if shortcut.id() == open => tray::show_main(app),
                _ => {}
            }
        })
        .build()
}

/// Gửi clipboard tới máy gần nhất; không biết gửi máy nào thì mở cửa sổ ở tab Văn bản.
fn send_nearest(app: &AppHandle) {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let online = tray::tray_peers(&st.peer_list());
    let history = st.history.lock().unwrap().list(200);
    match pick_target(&online, &history) {
        Target::Peer(id) => tray::send_clipboard(app, &id),
        Target::NoPeers => tray::notify(
            app,
            &t(app, "Chưa thấy máy nào", "No devices found"),
            &t(app, "Mở BetterDrop trên máy kia và kết nối cùng một mạng Wi‑Fi/LAN.", "Open BetterDrop on the other device and join the same Wi‑Fi/LAN."),
        ),
        Target::Choose => {
            let text = app.clipboard().read_text().unwrap_or_default();
            if !text.trim().is_empty() {
                st.add_shared(vec![], Some(text));
            }
            tray::show_main(app);
        }
    }
}
