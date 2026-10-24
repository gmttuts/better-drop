//! Chạy nền: biểu tượng trên thanh menu (macOS) / khay hệ thống (Windows),
//! ẩn cửa sổ thay vì thoát, thông báo hệ thống khi có người gửi tới, và gửi nhanh
//! clipboard/file tới máy khác ngay từ menu.

use crate::models::Peer;
use crate::state::AppState;
use crate::transfer;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

/// true khi người dùng chọn "Thoát" — lúc đó đóng cửa sổ là thoát thật.
pub static QUITTING: AtomicBool = AtomicBool::new(false);
/// Chỉ nhắc "vẫn chạy nền" một lần mỗi phiên.
static HINT_SHOWN: AtomicBool = AtomicBool::new(false);

pub struct TrayState {
    pub receive: CheckMenuItem<Wry>,
    open: MenuItem<Wry>,
    folder: MenuItem<Wry>,
    quit: MenuItem<Wry>,
    send_clip: Submenu<Wry>,
    send_file: Submenu<Wry>,
    /// Máy đang hiện trong hai menu con (id, tên), để chỉ dựng lại khi thật sự đổi.
    shown: Mutex<Option<Vec<(String, String)>>>,
    /// Lượt gửi file bắt đầu từ menu: báo kết quả bằng thông báo hệ thống.
    tray_sends: Mutex<HashSet<String>>,
}

const CLIP_PREFIX: &str = "clip:";
const FILE_PREFIX: &str = "file:";

/// Chọn chữ theo ngôn ngữ giao diện trong Cài đặt.
fn t(app: &AppHandle, vi: &str, en: &str) -> String {
    let english = app
        .try_state::<Arc<AppState>>()
        .is_some_and(|st| st.settings.read().unwrap().english());
    if english { en } else { vi }.to_string()
}

/// Đổi chữ trên menu sau khi đổi ngôn ngữ.
pub fn apply_language(app: &AppHandle) {
    if let Some(ts) = app.try_state::<TrayState>() {
        let _ = ts.open.set_text(t(app, "Mở BetterDrop", "Open BetterDrop"));
        let _ = ts.folder.set_text(t(app, "Mở thư mục nhận", "Open received folder"));
        let _ = ts.receive.set_text(t(app, "Cho phép nhận file", "Allow receiving files"));
        let _ = ts.quit.set_text(t(app, "Thoát BetterDrop", "Quit BetterDrop"));
        let _ = ts.send_clip.set_text(t(app, "Gửi clipboard tới", "Send clipboard to"));
        let _ = ts.send_file.set_text(t(app, "Gửi file tới", "Send files to"));
        // Mục "Chưa thấy máy nào" cũng cần đổi chữ: dựng lại hai menu con.
        let peers = ts.shown.lock().unwrap().take().unwrap_or_default();
        fill_peer_menus(app, &ts, &peers);
    }
}

/// Máy hiện trong menu gửi nhanh: chỉ máy đang bật, theo thứ tự của `peer_list`.
pub fn tray_peers(peers: &[Peer]) -> Vec<(String, String)> {
    peers
        .iter()
        .filter(|p| p.online)
        .map(|p| (p.info.id.clone(), p.info.name.clone()))
        .collect()
}

/// Dựng lại hai menu con theo danh sách máy (không làm gì nếu không đổi).
fn fill_peer_menus(app: &AppHandle, ts: &TrayState, peers: &[(String, String)]) {
    let mut shown = ts.shown.lock().unwrap();
    if shown.as_deref() == Some(peers) {
        return;
    }
    for (menu, prefix) in [(&ts.send_clip, CLIP_PREFIX), (&ts.send_file, FILE_PREFIX)] {
        if let Ok(items) = menu.items() {
            for item in items {
                let _ = menu.remove(&item);
            }
        }
        if peers.is_empty() {
            if let Ok(empty) = MenuItem::new(app, t(app, "Chưa thấy máy nào", "No devices found"), false, None::<&str>) {
                let _ = menu.append(&empty);
            }
        }
        for (id, name) in peers {
            if let Ok(item) = MenuItem::with_id(app, format!("{prefix}{id}"), name, true, None::<&str>) {
                let _ = menu.append(&item);
            }
        }
    }
    *shown = Some(peers.to_vec());
}

/// Cập nhật menu gửi nhanh khi danh sách máy đổi. Chạy trên luồng chính vì
/// event có thể được phát từ bất kỳ luồng nào.
fn refresh_peers(app: &AppHandle, peers: Vec<(String, String)>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(ts) = handle.try_state::<TrayState>() {
            fill_peer_menus(&handle, &ts, &peers);
        }
    });
}

/// Đổi lỗi tiếng Việt từ phần gửi sang tiếng Anh khi cần (giống i18n.ts).
fn tr_error(app: &AppHandle, err: &str) -> String {
    let en = match err {
        "Máy nhận đang tắt chế độ nhận file" => "That device has receiving turned off",
        "Máy nhận đã từ chối" => "The receiver declined",
        "Không tìm thấy máy nhận" => "Device not found",
        e if e.starts_with("Máy nhận trả lỗi") => "The receiver returned an error",
        e if e.starts_with("Không kết nối được") => "Could not connect to that device",
        e => e,
    };
    t(app, err, en)
}

/// "Gửi clipboard tới › Máy A": gửi văn bản trong clipboard.
fn send_clipboard(app: &AppHandle, peer_id: &str) {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let Some(peer) = st.find_peer(peer_id) else {
        notify(app, &t(app, "Không gửi được", "Couldn't send"), &tr_error(app, "Không tìm thấy máy nhận"));
        return;
    };
    let text = app.clipboard().read_text().unwrap_or_default();
    if text.trim().is_empty() {
        notify(
            app,
            &t(app, "Clipboard trống", "Clipboard is empty"),
            &t(app, "Hãy sao chép một đoạn văn bản rồi thử lại.", "Copy some text first, then try again."),
        );
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let name = peer.info.name.clone();
        match transfer::send_text(st, peer, text).await {
            Ok(()) => notify(&app, &t(&app, &format!("Đã gửi tới {name}"), &format!("Sent to {name}")), &t(&app, "Văn bản trong clipboard", "Clipboard text")),
            Err(e) => notify(&app, &t(&app, &format!("Không gửi được tới {name}"), &format!("Couldn't send to {name}")), &tr_error(&app, &e)),
        }
    });
}

/// "Gửi file tới › Máy A": chọn file rồi gửi như từ cửa sổ chính.
fn pick_and_send(app: &AppHandle, peer_id: &str) {
    let peer_id = peer_id.to_string();
    let handle = app.clone();
    app.dialog()
        .file()
        .set_title(t(app, "Chọn file để gửi", "Choose files to send"))
        .pick_files(move |picked| {
            let Some(picked) = picked else { return };
            let paths: Vec<String> = picked
                .into_iter()
                .filter_map(|p| p.into_path().ok())
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if paths.is_empty() {
                return;
            }
            tauri::async_runtime::spawn(async move {
                let app = &handle;
                let st = app.state::<Arc<AppState>>().inner().clone();
                let fail = |e: &str| notify(app, &t(app, "Không gửi được", "Couldn't send"), &tr_error(app, e));
                let Some(peer) = st.find_peer(&peer_id) else {
                    return fail("Không tìm thấy máy nhận");
                };
                let state = st.clone();
                let files = match tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&state, &paths)).await {
                    Ok(Ok(files)) => files,
                    Ok(Err(e)) => return fail(&e),
                    Err(e) => return fail(&e.to_string()),
                };
                let (count, name) = (files.len(), peer.info.name.clone());
                let id = transfer::start_send_files(st, peer, files);
                app.state::<TrayState>().tray_sends.lock().unwrap().insert(id);
                notify(
                    app,
                    &t(app, &format!("Đang gửi tới {name}"), &format!("Sending to {name}")),
                    &t(app, &format!("{count} file. Chờ máy nhận chấp nhận…"), &format!("{count} file(s). Waiting for the receiver to accept…")),
                );
            });
        });
}

/// Kết quả lượt gửi file bắt đầu từ menu.
fn on_tray_send_update(app: &AppHandle, payload: &serde_json::Value) {
    let Some(ts) = app.try_state::<TrayState>() else { return };
    let id = payload["id"].as_str().unwrap_or_default();
    let status = payload["status"].as_str().unwrap_or_default();
    if !["done", "error", "rejected", "cancelled"].contains(&status) || !ts.tray_sends.lock().unwrap().remove(id) {
        return;
    }
    let name = payload["peer_name"].as_str().unwrap_or_default();
    let count = payload["file_count"].as_u64().unwrap_or(0);
    let err = payload["error"].as_str().unwrap_or_default();
    match status {
        "done" => notify(
            app,
            &t(app, &format!("Đã gửi tới {name}"), &format!("Sent to {name}")),
            &t(app, &format!("{count} file · {}", fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0))), &format!("{count} file(s) · {}", fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0)))),
        ),
        "cancelled" => notify(app, &t(app, "Đã huỷ gửi", "Sending cancelled"), name),
        _ => notify(
            app,
            &t(app, &format!("Không gửi được tới {name}"), &format!("Couldn't send to {name}")),
            &tr_error(app, if err.is_empty() { "Máy nhận đã từ chối" } else { err }),
        ),
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn window_active(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
        .unwrap_or(false)
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn setup(app: &AppHandle, receive_enabled: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", t(app, "Mở BetterDrop", "Open BetterDrop"), true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", t(app, "Mở thư mục nhận", "Open received folder"), true, None::<&str>)?;
    let receive = CheckMenuItem::with_id(
        app,
        "receive",
        t(app, "Cho phép nhận file", "Allow receiving files"),
        true,
        receive_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", t(app, "Thoát BetterDrop", "Quit BetterDrop"), true, None::<&str>)?;
    let send_clip = Submenu::with_id(app, "send-clip", t(app, "Gửi clipboard tới", "Send clipboard to"), true)?;
    let send_file = Submenu::with_id(app, "send-file", t(app, "Gửi file tới", "Send files to"), true)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &folder,
            &PredefinedMenuItem::separator(app)?,
            &send_clip,
            &send_file,
            &PredefinedMenuItem::separator(app)?,
            &receive,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // macOS: icon đơn sắc dạng "template" để tự đổi màu theo thanh menu sáng/tối.
    let (icon, template) = if cfg!(target_os = "macos") {
        (Image::from_bytes(include_bytes!("../icons/tray-template.png"))?, true)
    } else {
        (
            app.default_window_icon()
                .cloned()
                .unwrap_or(Image::from_bytes(include_bytes!("../icons/32x32.png"))?),
            false,
        )
    };

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(template)
        .tooltip("BetterDrop")
        .menu(&menu)
        // macOS: bấm vào icon thì hiện menu. Windows: bấm trái mở cửa sổ, bấm phải hiện menu.
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "folder" => {
                let st = app.state::<Arc<AppState>>();
                let dir = st.settings.read().unwrap().download_dir.clone();
                let _ = std::fs::create_dir_all(&dir);
                let _ = app.opener().open_path(dir, None::<&str>);
            }
            "receive" => {
                let checked = app
                    .state::<TrayState>()
                    .receive
                    .is_checked()
                    .unwrap_or(true);
                let _ = crate::set_receive(app, checked);
            }
            "quit" => {
                QUITTING.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            id => {
                if let Some(peer) = id.strip_prefix(CLIP_PREFIX) {
                    send_clipboard(app, peer);
                } else if let Some(peer) = id.strip_prefix(FILE_PREFIX) {
                    pick_and_send(app, peer);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if cfg!(not(target_os = "macos")) {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main(tray.app_handle());
                }
            }
        })
        .build(app)?;

    let ts = TrayState {
        receive,
        open,
        folder,
        quit,
        send_clip,
        send_file,
        shown: Mutex::new(None),
        tray_sends: Mutex::new(HashSet::new()),
    };
    let peers = app.try_state::<Arc<AppState>>().map(|st| st.peer_list()).unwrap_or_default();
    fill_peer_menus(app, &ts, &tray_peers(&peers));
    app.manage(ts);
    Ok(())
}

/// Đóng cửa sổ = ẩn xuống thanh menu/khay hệ thống (trừ khi đang thoát thật).
pub fn on_close_requested(window: &tauri::Window, api: &tauri::CloseRequestApi) {
    if QUITTING.load(Ordering::SeqCst) {
        return;
    }
    api.prevent_close();
    let _ = window.hide();
    if !HINT_SHOWN.swap(true, Ordering::SeqCst) {
        let app = window.app_handle();
        let body = if cfg!(target_os = "macos") {
            t(app, "Bạn vẫn nhận được file. Mở lại từ biểu tượng trên thanh menu.", "You can still receive files. Reopen it from the menu bar icon.")
        } else {
            t(app, "Bạn vẫn nhận được file. Mở lại từ biểu tượng trên khay hệ thống.", "You can still receive files. Reopen it from the system tray icon.")
        };
        notify(app, &t(app, "BetterDrop vẫn đang chạy nền", "BetterDrop is still running"), &body);
    }
}

fn fmt_bytes(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if v < 1024.0 {
            return if unit == "B" { format!("{n} B") } else { format!("{v:.1} {unit}") };
        }
        v /= 1024.0;
    }
    format!("{v:.1} TB")
}

/// Gọi cho mọi event gửi lên UI: bật cửa sổ / hiện thông báo khi cần.
pub fn on_event(app: &AppHandle, event: &str, payload: &serde_json::Value) {
    let s = |k: &str| payload[k].as_str().unwrap_or_default().to_string();
    match event {
        "peers-changed" => {
            if let Ok(peers) = serde_json::from_value::<Vec<Peer>>(payload.clone()) {
                refresh_peers(app, tray_peers(&peers));
            }
        }
        "incoming-request" => {
            // Cần người dùng trả lời trong 60 giây: hiện thông báo và đưa cửa sổ lên.
            if !window_active(app) {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                let size = fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0));
                notify(
                    app,
                    &t(app, &format!("{} muốn gửi cho bạn", s("sender_name")), &format!("{} wants to send you files", s("sender_name"))),
                    &t(app, &format!("{count} file · {size}. Mở BetterDrop để chấp nhận."), &format!("{count} file(s) · {size}. Open BetterDrop to accept.")),
                );
            }
            show_main(app);
        }
        "auto-accepted" => {
            if !window_active(app) {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                let size = fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0));
                notify(
                    app,
                    &t(app, &format!("Đang nhận từ {} (máy tin cậy)", s("sender_name")), &format!("Receiving from {} (trusted device)", s("sender_name"))),
                    &format!("{count} file · {size}"),
                );
            }
        }
        "incoming-text" => {
            if !window_active(app) {
                notify(app, &t(app, &format!("{} đã gửi văn bản", s("sender_name")), &format!("{} sent you text", s("sender_name"))), &s("text"));
            }
            show_main(app);
        }
        "transfer-update" => {
            if s("direction") == "send" {
                on_tray_send_update(app, payload);
            }
            let finished_receive = s("direction") == "receive"
                && s("kind") == "files"
                && s("status") == "done";
            if finished_receive && !window_active(app) {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                notify(
                    app,
                    &t(app, "Đã nhận xong", "Received"),
                    &t(
                        app,
                        &format!("{count} file từ {} đã lưu vào {}", s("peer_name"), s("dir_path")),
                        &format!("{count} file(s) from {} saved to {}", s("peer_name"), s("dir_path")),
                    ),
                );
            }
        }
        _ => {}
    }
}
