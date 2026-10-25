//! Phần khác nhau giữa máy tính (macOS/Windows/Linux) và điện thoại (Android/iOS).
//! `lib.rs` chỉ gọi các hàm ở đây, nên không phải rải `#[cfg]` khắp nơi.

pub use imp::*;

#[cfg(desktop)]
mod imp {
    use crate::identity::{self, Defaults};
    use crate::models::Settings;
    use crate::state::Hooks;
    use crate::{context_menu, tray};
    use std::path::PathBuf;
    use tauri::{AppHandle, Builder, Manager, Wry};
    use tauri_plugin_autostart::ManagerExt as _;

    const HIDDEN_ARG: &str = "--hidden";

    pub fn plugins(mut builder: Builder<Wry>) -> Builder<Wry> {
        // Mở app lần nữa thì chỉ hiện lại cửa sổ cũ. Bỏ qua khi chạy thử nhiều bản bằng profile.
        if identity::profile().is_none() {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                // Mở lại app (vd "Send to" khi app đang chạy): lấy file trong tham số.
                open_with_args(app, args);
                tray::show_main(app);
            }));
        }
        builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec![HIDDEN_ARG]),
            ))
            .plugin(crate::hotkey::plugin())
    }

    /// File trong tham số dòng lệnh: đưa vào danh sách chờ gửi, hoặc gửi luôn khi có
    /// `--send-to <id>` (menu chuột phải Windows › tên máy).
    fn open_with_args(app: &AppHandle, args: impl IntoIterator<Item = String>) {
        let a = crate::launch_args(args);
        match a.send_to {
            Some(id) if cfg!(windows) && !a.paths.is_empty() => context_menu::queue_send(app, id, a.paths),
            _ => {
                if let Some(st) = app.try_state::<std::sync::Arc<crate::state::AppState>>() {
                    st.add_shared(a.paths, None);
                }
            }
        }
    }

    pub fn dirs(_app: &AppHandle) -> tauri::Result<(PathBuf, Defaults)> {
        Ok((identity::data_dir(), Defaults::default()))
    }

    pub fn hooks(_app: &AppHandle) -> Hooks {
        Hooks::default()
    }

    pub fn on_event(app: &AppHandle, event: &str, payload: &serde_json::Value) {
        tray::on_event(app, event, payload);
        if matches!(event, "peers-changed" | "devices-changed") {
            context_menu::refresh(app);
        }
    }

    pub fn before_discovery(_app: &AppHandle) {}

    /// Máy tính luôn chạy nền (khay hệ thống), không có gì để bật/tắt. Đổi ngôn ngữ thì
    /// viết lại chữ trên menu chuột phải (Windows).
    pub fn settings_changed(app: &AppHandle) {
        context_menu::refresh(app);
    }

    pub fn request_notifications(_app: &AppHandle) -> bool {
        true
    }

    /// Cửa sổ trên máy tính không bị tạo lại nên không cần gửi lại yêu cầu đang chờ.
    pub fn pending_requests() -> Vec<serde_json::Value> {
        vec![]
    }

    pub fn after_setup(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
        // Mở app kèm file (Windows "Send to"/menu chuột phải, Linux "Mở bằng"): đưa vào danh sách chờ gửi.
        open_with_args(app, std::env::args());
        context_menu::refresh(app);
        tray::setup(app, settings.receive_enabled)?;
        // Tổ hợp bị app khác chiếm thì bỏ qua: lỗi hiện trong Cài đặt.
        let _ = crate::hotkey::apply(app, &settings.hotkeys);
        apply_dock(app, settings.hide_dock);
        // Khi tự mở cùng máy tính thì chạy nền luôn, không hiện cửa sổ.
        if !std::env::args().any(|a| a == HIDDEN_ARG) {
            tray::show_main(app);
        }
        Ok(())
    }

    pub fn autostart_enabled(app: &AppHandle) -> bool {
        app.autolaunch().is_enabled().unwrap_or(false)
    }

    pub fn pull_shared(_app: &AppHandle) {}

    pub fn request_local_network(_app: &AppHandle) -> bool {
        true
    }

    pub fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
        let al = app.autolaunch();
        if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())
    }

    #[cfg(target_os = "macos")]
    pub fn apply_dock(app: &AppHandle, hide: bool) {
        let _ = app.set_dock_visibility(!hide);
    }
    #[cfg(not(target_os = "macos"))]
    pub fn apply_dock(_app: &AppHandle, _hide: bool) {}
}

#[cfg(mobile)]
mod imp {
    use crate::identity::Defaults;
    use crate::models::{DeviceInfo, Peer, Settings};
    use crate::state::{Hooks, NativeDiscovery};
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tauri::{AppHandle, Builder, Manager, Wry};
    use tauri_plugin_betterdrop_mobile::{BackgroundNotice, BetterdropMobileExt};
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};
    use tauri_plugin_notification::NotificationExt;

    pub fn plugins(builder: Builder<Wry>) -> Builder<Wry> {
        builder
            .plugin(tauri_plugin_barcode_scanner::init())
            .plugin(tauri_plugin_betterdrop_mobile::init())
    }

    /// Thử ghi vào thư mục để biết có dùng được không.
    fn writable(dir: &Path) -> bool {
        let probe = dir.join(".betterdrop-probe");
        let ok = std::fs::create_dir_all(dir).is_ok() && std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(probe);
        ok
    }

    pub fn dirs(app: &AppHandle) -> tauri::Result<(PathBuf, Defaults)> {
        let data_dir = app.path().app_data_dir()?;
        let mobile = app.betterdrop_mobile();
        // Android: Download/BetterDrop (hiện trong app Files). iOS: Documents của app (app Tệp).
        let download_dir = mobile
            .download_dir()
            .ok()
            .map(PathBuf::from)
            .map(|d| if cfg!(target_os = "android") { d.join("BetterDrop") } else { d })
            .filter(|d| writable(d))
            .unwrap_or_else(|| data_dir.join("Received"));
        Ok((
            data_dir,
            Defaults {
                name: mobile.device_name().ok().filter(|n| !n.trim().is_empty()),
                download_dir: Some(download_dir),
                force_download_dir: true,
            },
        ))
    }

    pub fn hooks(app: &AppHandle) -> Hooks {
        let a = app.clone();
        let open_uri = Arc::new(move |uri: &str| {
            let path = FilePath::from_str(uri).map_err(std::io::Error::other)?;
            let mut opts = OpenOptions::new();
            opts.read(true);
            a.fs().open(path, opts)
        });
        let a = app.clone();
        let uri_info = Arc::new(move |uri: &str| {
            let info = a
                .betterdrop_mobile()
                .file_info(uri)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok((info.name, u64::try_from(info.size).ok()))
        });
        let a = app.clone();
        let file_saved = Arc::new(move |path: &Path| {
            // Activity đã bị huỷ (app chạy nền) thì plugin panic: bỏ qua, file vẫn đã lưu.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.betterdrop_mobile().scan_file(&path.to_string_lossy())));
        });
        Hooks {
            open_uri: Some(open_uri),
            uri_info: Some(uri_info),
            file_saved: Some(file_saved),
            before_mdns: {
                let a = app.clone();
                Some(Arc::new(move || {
                    // Khi chạy nền, dịch vụ đã giữ sẵn một khoá multicast.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.betterdrop_mobile().acquire_multicast_lock()));
                }))
            },
            native_discovery: if cfg!(target_os = "ios") {
                Some(Arc::new(Bonjour(app.clone())))
            } else {
                None
            },
        }
    }

    /// iOS: tìm máy qua Bonjour của hệ thống (xem BetterDropPlugin.swift).
    struct Bonjour(AppHandle);

    impl NativeDiscovery for Bonjour {
        fn announce(&self, me: &DeviceInfo) -> Result<(), String> {
            self.0
                .betterdrop_mobile()
                .start_bonjour(&me.id, &me.name, &me.os, &me.fp, me.port, &me.version)
                .map_err(|e| e.to_string())
        }

        fn peers(&self) -> Vec<Peer> {
            self.0
                .betterdrop_mobile()
                .bonjour_peers()
                .unwrap_or_default()
                .into_iter()
                .map(|p| Peer {
                    addrs: p.ips.iter().map(|ip| format!("{ip}:{}", p.port)).collect(),
                    info: DeviceInfo {
                        id: p.id,
                        name: p.name,
                        os: p.os,
                        fp: p.fp,
                        port: p.port,
                        version: p.version,
                    },
                    manual: false,
                    online: true,
                })
                .collect()
        }
    }

    // ---- Nhận file khi chạy nền (Android) ----

    /// App đang hiện trên màn hình (Kotlin báo qua `watch_lifecycle`).
    static FOREGROUND: AtomicBool = AtomicBool::new(true);
    /// Dịch vụ chạy nền đang chạy với chữ tiếng Anh hay Việt; None = đang tắt.
    static BACKGROUND: Mutex<Option<bool>> = Mutex::new(None);
    /// Yêu cầu nhận đang chờ trả lời. Activity bị Android tạo lại (giao diện nạp lại từ đầu)
    /// thì giao diện lấy lại từ đây.
    static PENDING: Mutex<Vec<(Instant, serde_json::Value)>> = Mutex::new(Vec::new());
    /// Các lượt nhận đang chạy: còn lượt nào thì giữ CPU thức (màn hình tắt vẫn nhận tiếp).
    static RECEIVING: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Gọi plugin trên luồng riêng: lệnh plugin chờ luồng giao diện của Android, mà event có
    /// thể phát ra từ chính luồng đó. Khi Activity đã bị huỷ, wry panic ("no available activity").
    fn spawn_plugin_call(f: impl FnOnce() + Send + 'static) {
        std::thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        });
    }

    fn english(app: &AppHandle) -> bool {
        app.try_state::<Arc<crate::state::AppState>>()
            .map(|st| st.settings.read().unwrap().english())
            .unwrap_or(false)
    }

    fn t(app: &AppHandle, vi: &str, en: &str) -> String {
        if english(app) { en } else { vi }.to_string()
    }

    fn notify(app: &AppHandle, title: String, body: String) {
        let a = app.clone();
        spawn_plugin_call(move || {
            let _ = a.notification().builder().title(title).body(body).auto_cancel().show();
        });
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

    /// Gọi cho mọi event gửi lên UI: khi app không ở màn hình thì hiện thông báo (bấm vào
    /// để mở app và trả lời); giữ CPU thức trong lúc nhận file.
    pub fn on_event(app: &AppHandle, event: &str, payload: &serde_json::Value) {
        let s = |k: &str| payload[k].as_str().unwrap_or_default().to_string();
        let hidden = !FOREGROUND.load(Ordering::SeqCst);
        match event {
            "incoming-request" => {
                PENDING.lock().unwrap().push((Instant::now(), payload.clone()));
                if hidden {
                    let count = payload["file_count"].as_u64().unwrap_or(0);
                    let size = fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0));
                    notify(
                        app,
                        t(app, &format!("{} muốn gửi cho bạn", s("sender_name")), &format!("{} wants to send you files", s("sender_name"))),
                        t(app, &format!("{count} file · {size}. Chạm để chấp nhận hoặc từ chối."), &format!("{count} file(s) · {size}. Tap to accept or decline.")),
                    );
                }
            }
            "request-closed" => PENDING.lock().unwrap().retain(|(_, p)| p["request_id"] != payload["request_id"]),
            "auto-accepted" if hidden => {
                let count = payload["file_count"].as_u64().unwrap_or(0);
                let size = fmt_bytes(payload["total_bytes"].as_u64().unwrap_or(0));
                notify(
                    app,
                    t(app, &format!("Đang nhận từ {} (máy tin cậy)", s("sender_name")), &format!("Receiving from {} (trusted device)", s("sender_name"))),
                    format!("{count} file · {size}"),
                );
            }
            "incoming-text" if hidden => {
                notify(app, t(app, &format!("{} đã gửi văn bản", s("sender_name")), &format!("{} sent you text", s("sender_name"))), s("text"));
            }
            "transfer-update" if s("direction") == "receive" => {
                let status = s("status");
                let finished = !matches!(status.as_str(), "waiting" | "transferring");
                let changed = {
                    let mut r = RECEIVING.lock().unwrap();
                    let before = r.is_empty();
                    let id = s("id");
                    if finished {
                        r.retain(|x| *x != id);
                    } else if !r.contains(&id) {
                        r.push(id);
                    }
                    before != r.is_empty()
                };
                if changed && cfg!(target_os = "android") {
                    let a = app.clone();
                    spawn_plugin_call(move || {
                        // Giữ khoá trong lúc gọi để các lần bật/tắt không đến Kotlin sai thứ tự.
                        let r = RECEIVING.lock().unwrap();
                        let _ = a.betterdrop_mobile().keep_awake(!r.is_empty());
                    });
                }
                if hidden && status == "done" && s("kind") == "files" {
                    let count = payload["file_count"].as_u64().unwrap_or(0);
                    notify(
                        app,
                        t(app, "Đã nhận xong", "Received"),
                        t(
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

    /// Yêu cầu nhận đang chờ, với số giây còn lại để trả lời.
    pub fn pending_requests() -> Vec<serde_json::Value> {
        PENDING
            .lock()
            .unwrap()
            .iter()
            .map(|(at, p)| {
                let mut p = p.clone();
                let left = p["timeout_secs"].as_u64().unwrap_or(0).saturating_sub(at.elapsed().as_secs());
                p["timeout_secs"] = left.into();
                p
            })
            .collect()
    }

    /// Bật/tắt dịch vụ chạy nền theo cài đặt (chạy khi bật cả "Cho phép nhận file" lẫn
    /// "Nhận file khi chạy nền"); bật lại khi đổi ngôn ngữ để đổi chữ trên thông báo.
    pub fn settings_changed(app: &AppHandle) {
        if !cfg!(target_os = "android") {
            return;
        }
        let a = app.clone();
        spawn_plugin_call(move || {
            let Some(st) = a.try_state::<Arc<crate::state::AppState>>() else { return };
            // Giữ khoá trong lúc gọi plugin để hai lần đổi liền nhau không đến Kotlin sai thứ tự.
            let mut cur = BACKGROUND.lock().unwrap();
            let s = st.settings.read().unwrap().clone();
            let want = (s.background_receive && s.receive_enabled).then(|| s.english());
            if *cur == want {
                return;
            }
            let mobile = a.betterdrop_mobile();
            let ok = match want {
                Some(en) => {
                    let tr = |vi: &'static str, e: &'static str| if en { e } else { vi };
                    let notice = BackgroundNotice {
                        title: tr("BetterDrop đang sẵn sàng nhận file", "BetterDrop is ready to receive files"),
                        text: tr("Máy khác trong mạng Wi‑Fi vẫn thấy điện thoại này.", "Other devices on your Wi‑Fi can still see this phone."),
                        stop_label: tr("Tắt", "Turn off"),
                        channel_name: tr("Nhận file khi chạy nền", "Receive in the background"),
                    };
                    let app = a.clone();
                    mobile
                        .start_background(&notice, move || {
                            // Người dùng bấm "Tắt" trên thông báo: tắt luôn cài đặt (dịch vụ đã tự dừng).
                            let app = app.clone();
                            std::thread::spawn(move || {
                                *BACKGROUND.lock().unwrap() = None;
                                let _ = crate::change_settings(&app, |s| s.background_receive = false);
                            });
                        })
                        .is_ok()
                }
                None => mobile.stop_background().is_ok(),
            };
            if ok {
                *cur = want;
            }
        });
    }

    /// Android 13+: xin quyền hiện thông báo (chặn tới khi người dùng trả lời).
    pub fn request_notifications(app: &AppHandle) -> bool {
        !cfg!(target_os = "android") || app.betterdrop_mobile().request_notifications().unwrap_or(false)
    }

    pub fn before_discovery(_app: &AppHandle) {}

    pub fn after_setup(app: &AppHandle, _settings: &Settings) -> tauri::Result<()> {
        if cfg!(target_os = "android") {
            let _ = app
                .betterdrop_mobile()
                .watch_lifecycle(|fg| FOREGROUND.store(fg, Ordering::SeqCst));
            settings_changed(app);
        }
        Ok(())
    }

    pub fn autostart_enabled(_app: &AppHandle) -> bool {
        false
    }

    /// Android: lấy file/văn bản từ menu Chia sẻ (plugin giữ lại intent).
    pub fn pull_shared(app: &AppHandle) {
        if let Ok((paths, text, link)) = app.betterdrop_mobile().take_shared() {
            if let Some(st) = app.try_state::<Arc<crate::state::AppState>>() {
                st.add_shared(paths, text);
                if let Some(link) = link {
                    st.set_shared_link(link);
                }
            }
        }
    }

    /// Android 17+: xin quyền mạng cục bộ (chặn tới khi người dùng trả lời).
    pub fn request_local_network(app: &AppHandle) -> bool {
        app.betterdrop_mobile().request_local_network().unwrap_or(false)
    }

    pub fn set_autostart(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
        Err("Điện thoại không hỗ trợ tự mở cùng máy".into())
    }

    pub fn apply_dock(_app: &AppHandle, _hide: bool) {}
}
