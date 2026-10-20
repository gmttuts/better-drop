//! Chạy nền: biểu tượng trên thanh menu (macOS) / khay hệ thống (Windows),
//! ẩn cửa sổ thay vì thoát, và thông báo hệ thống khi có người gửi tới.

use crate::state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

/// true khi người dùng chọn "Thoát" — lúc đó đóng cửa sổ là thoát thật.
pub static QUITTING: AtomicBool = AtomicBool::new(false);
/// Chỉ nhắc "vẫn chạy nền" một lần mỗi phiên.
static HINT_SHOWN: AtomicBool = AtomicBool::new(false);

pub struct TrayState {
    pub receive: CheckMenuItem<Wry>,
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
    let open = MenuItem::with_id(app, "open", "Mở BetterDrop", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Mở thư mục nhận", true, None::<&str>)?;
    let receive = CheckMenuItem::with_id(
        app,
        "receive",
        "Cho phép nhận file",
        true,
        receive_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Thoát BetterDrop", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &folder,
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
            _ => {}
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

    app.manage(TrayState { receive });
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
        let where_ = if cfg!(target_os = "macos") { "thanh menu" } else { "khay hệ thống" };
        notify(
            window.app_handle(),
            "BetterDrop vẫn đang chạy nền",
            &format!("Bạn vẫn nhận được file. Mở lại từ biểu tượng trên {where_}."),
        );
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
        "incoming-request" => {
            // Cần người dùng trả lời trong 60 giây: hiện thông báo và đưa cửa sổ lên.
            if !window_active(app) {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                let size = fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0));
                notify(
                    app,
                    &format!("{} muốn gửi cho bạn", s("sender_name")),
                    &format!("{count} file · {size}. Mở BetterDrop để chấp nhận."),
                );
            }
            show_main(app);
        }
        "incoming-text" => {
            if !window_active(app) {
                notify(app, &format!("{} đã gửi văn bản", s("sender_name")), &s("text"));
            }
            show_main(app);
        }
        "transfer-update" => {
            let finished_receive = s("direction") == "receive"
                && s("kind") == "files"
                && s("status") == "done";
            if finished_receive && !window_active(app) {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                notify(
                    app,
                    "Đã nhận xong",
                    &format!("{count} file từ {} đã lưu vào {}", s("peer_name"), s("dir_path")),
                );
            }
        }
        _ => {}
    }
}
