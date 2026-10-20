//! Phần khác nhau giữa máy tính (macOS/Windows/Linux) và điện thoại (Android/iOS).
//! `lib.rs` chỉ gọi các hàm ở đây, nên không phải rải `#[cfg]` khắp nơi.

pub use imp::*;

#[cfg(desktop)]
mod imp {
    use crate::identity::{self, Defaults};
    use crate::models::Settings;
    use crate::state::Hooks;
    use crate::tray;
    use std::path::PathBuf;
    use tauri::{AppHandle, Builder, Wry};
    use tauri_plugin_autostart::ManagerExt as _;

    const HIDDEN_ARG: &str = "--hidden";

    pub fn plugins(mut builder: Builder<Wry>) -> Builder<Wry> {
        // Mở app lần nữa thì chỉ hiện lại cửa sổ cũ. Bỏ qua khi chạy thử nhiều bản bằng profile.
        if identity::profile().is_none() {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                tray::show_main(app);
            }));
        }
        builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
    }

    pub fn dirs(_app: &AppHandle) -> tauri::Result<(PathBuf, Defaults)> {
        Ok((identity::data_dir(), Defaults::default()))
    }

    pub fn hooks(_app: &AppHandle) -> Hooks {
        Hooks::default()
    }

    pub fn on_event(app: &AppHandle, event: &str, payload: &serde_json::Value) {
        tray::on_event(app, event, payload);
    }

    pub fn before_discovery(_app: &AppHandle) {}

    pub fn after_setup(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
        tray::setup(app, settings.receive_enabled)?;
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
    use std::sync::Arc;
    use tauri::{AppHandle, Builder, Manager, Wry};
    use tauri_plugin_betterdrop_mobile::BetterdropMobileExt;
    use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

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
            let _ = a.betterdrop_mobile().scan_file(&path.to_string_lossy());
        });
        Hooks {
            open_uri: Some(open_uri),
            uri_info: Some(uri_info),
            file_saved: Some(file_saved),
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
                .start_bonjour(&me.id, &me.name, &me.os, &me.fp, me.port)
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
                    },
                    manual: false,
                })
                .collect()
        }
    }

    pub fn on_event(_app: &AppHandle, _event: &str, _payload: &serde_json::Value) {}

    pub fn before_discovery(app: &AppHandle) {
        let _ = app.betterdrop_mobile().acquire_multicast_lock();
    }

    pub fn after_setup(_app: &AppHandle, _settings: &Settings) -> tauri::Result<()> {
        Ok(())
    }

    pub fn autostart_enabled(_app: &AppHandle) -> bool {
        false
    }

    pub fn set_autostart(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
        Err("Điện thoại không hỗ trợ tự mở cùng máy".into())
    }

    pub fn apply_dock(_app: &AppHandle, _hide: bool) {}
}
