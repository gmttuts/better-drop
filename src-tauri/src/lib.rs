mod client;
mod discovery;
mod identity;
mod models;
mod paths;
mod server;
mod state;
mod transfer;
mod tray;

use models::{Peer, SelfInfo, Settings};
use state::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt;

type AppStateRef<'a> = State<'a, Arc<AppState>>;

fn local_addrs(port: u16) -> Vec<String> {
    let mut ips: Vec<std::net::Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(ip),
            _ => None,
        })
        .collect();
    ips.sort_by_key(|ip| (!ip.is_private(), *ip));
    ips.dedup();
    ips.into_iter().map(|ip| format!("{ip}:{port}")).collect()
}

fn self_info(app: &tauri::AppHandle) -> SelfInfo {
    let st = app.state::<Arc<AppState>>();
    let settings = st.settings.read().unwrap().clone();
    SelfInfo {
        info: st.device_info(),
        settings,
        addrs: local_addrs(st.port),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
    }
}

/// Sửa cài đặt, lưu ra đĩa và báo UI.
fn change_settings(app: &tauri::AppHandle, f: impl FnOnce(&mut Settings)) -> Result<SelfInfo, String> {
    let st = app.state::<Arc<AppState>>();
    let new = {
        let mut s = st.settings.write().unwrap();
        f(&mut s);
        s.clone()
    };
    identity::save_settings(&st.data_dir, &new).map_err(|e| e.to_string())?;
    let info = self_info(app);
    let _ = app.emit("settings-changed", &info);
    Ok(info)
}

/// Bật/tắt nhận file (dùng chung cho Cài đặt và menu khay hệ thống).
pub(crate) fn set_receive(app: &tauri::AppHandle, enabled: bool) -> Result<SelfInfo, String> {
    if let Some(t) = app.try_state::<tray::TrayState>() {
        let _ = t.receive.set_checked(enabled);
    }
    change_settings(app, |s| s.receive_enabled = enabled)
}

#[cfg(target_os = "macos")]
fn apply_dock(app: &tauri::AppHandle, hide: bool) {
    let _ = app.set_dock_visibility(!hide);
}
#[cfg(not(target_os = "macos"))]
fn apply_dock(_app: &tauri::AppHandle, _hide: bool) {}

#[tauri::command]
fn get_self_info(app: tauri::AppHandle) -> SelfInfo {
    self_info(&app)
}

#[tauri::command]
fn set_receive_enabled(app: tauri::AppHandle, enabled: bool) -> Result<SelfInfo, String> {
    set_receive(&app, enabled)
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<SelfInfo, String> {
    let al = app.autolaunch();
    if enabled { al.enable() } else { al.disable() }.map_err(|e| e.to_string())?;
    change_settings(&app, |_| {})
}

#[tauri::command]
fn set_hide_dock(app: tauri::AppHandle, hide: bool) -> Result<SelfInfo, String> {
    apply_dock(&app, hide);
    change_settings(&app, |s| s.hide_dock = hide)
}

#[tauri::command]
fn get_peers(st: AppStateRef) -> Vec<Peer> {
    st.peer_list()
}

#[tauri::command]
fn update_settings(app: tauri::AppHandle, name: String, download_dir: String) -> Result<SelfInfo, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Tên máy không được để trống".into());
    }
    if download_dir.trim().is_empty() {
        return Err("Chưa chọn thư mục lưu".into());
    }
    let info = change_settings(&app, |s| {
        s.name = name;
        s.download_dir = download_dir;
    })?;
    let st = app.state::<Arc<AppState>>();
    if let Some(d) = st.mdns.lock().unwrap().as_ref() {
        discovery::register(d, &st.device_info())?;
    }
    Ok(info)
}

#[tauri::command]
async fn add_manual_peer(st: AppStateRef<'_>, addr: String) -> Result<Peer, String> {
    discovery::add_manual(&st, &addr).await
}

#[tauri::command]
async fn send_files(st: AppStateRef<'_>, peer_id: String, paths: Vec<String>) -> Result<String, String> {
    let peer = st.find_peer(&peer_id).ok_or("Không tìm thấy máy nhận")?;
    let files = tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&paths))
        .await
        .map_err(|e| e.to_string())??;
    Ok(transfer::start_send_files(st.inner().clone(), peer, files))
}

#[tauri::command]
async fn send_text(st: AppStateRef<'_>, peer_id: String, text: String) -> Result<(), String> {
    let peer = st.find_peer(&peer_id).ok_or("Không tìm thấy máy nhận")?;
    transfer::send_text(st.inner().clone(), peer, text).await
}

#[tauri::command]
fn respond_request(st: AppStateRef, request_id: String, accept: bool) {
    if let Some(tx) = st.pending.lock().unwrap().remove(&request_id) {
        let _ = tx.send(accept);
    }
}

#[tauri::command]
fn cancel_transfer(st: AppStateRef, id: String) {
    transfer::cancel(&st, &id);
}

#[tauri::command]
fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

const HIDDEN_ARG: &str = "--hidden";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut builder = tauri::Builder::default();
    // Mở app lần nữa thì chỉ hiện lại cửa sổ cũ. Bỏ qua khi chạy thử nhiều bản bằng profile.
    if identity::profile().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = identity::data_dir();
            let settings = identity::load_settings(&data_dir);
            let ident = identity::load_or_create_identity(&data_dir)?;
            let listener = server::bind()?;
            let port = listener.local_addr()?.port();

            let handle = app.handle().clone();
            let emit: state::EmitFn = Arc::new(move |event, payload| {
                tray::on_event(&handle, event, &payload);
                let _ = handle.emit(event, payload);
            });
            let receive_enabled = settings.receive_enabled;
            let hide_dock = settings.hide_dock;
            let st = Arc::new(AppState::new(emit, data_dir, ident, port, settings));
            app.manage(st.clone());

            let server_state = st.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server::serve(server_state.clone(), listener).await {
                    server_state.emit("fatal-error", format!("Máy chủ nhận file dừng: {e}"));
                }
            });
            if let Err(e) = discovery::start(st.clone()) {
                st.emit("fatal-error", format!("Không bật được tự tìm máy: {e}"));
            }

            tray::setup(app.handle(), receive_enabled)?;
            apply_dock(app.handle(), hide_dock);
            // Khi tự mở cùng máy tính thì chạy nền luôn, không hiện cửa sổ.
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                tray::on_close_requested(window, api);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_self_info,
            set_receive_enabled,
            set_autostart,
            set_hide_dock,
            get_peers,
            update_settings,
            add_manual_peer,
            send_files,
            send_text,
            respond_request,
            cancel_transfer,
            open_folder,
        ])
        .build(tauri::generate_context!())
        .expect("không khởi động được BetterDrop")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    discovery::shutdown(&st);
                }
            }
            // macOS: bấm vào biểu tượng trên Dock khi cửa sổ đang ẩn.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            _ => {}
        });
}

#[cfg(test)]
mod tests;
