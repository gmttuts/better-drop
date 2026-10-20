mod client;
mod discovery;
mod identity;
mod models;
mod paths;
mod platform;
mod server;
mod state;
mod transfer;
#[cfg(desktop)]
mod tray;

use models::{PairInfo, Peer, SelfInfo, Settings, StagedItem};
use state::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

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
        autostart: platform::autostart_enabled(app),
        mobile: cfg!(mobile),
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
    #[cfg(desktop)]
    if let Some(t) = app.try_state::<tray::TrayState>() {
        let _ = t.receive.set_checked(enabled);
    }
    change_settings(app, |s| s.receive_enabled = enabled)
}

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
    platform::set_autostart(&app, enabled)?;
    change_settings(&app, |_| {})
}

#[tauri::command]
fn set_hide_dock(app: tauri::AppHandle, hide: bool) -> Result<SelfInfo, String> {
    platform::apply_dock(&app, hide);
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
    discovery::reannounce(&app.state::<Arc<AppState>>())?;
    Ok(info)
}

/// Nhận cả địa chỉ IP lẫn liên kết `betterdrop://` (dán từ QR).
#[tauri::command]
async fn add_manual_peer(st: AppStateRef<'_>, addr: String) -> Result<Peer, String> {
    if addr.trim().starts_with(discovery::LINK_SCHEME) {
        discovery::add_from_link(&st, &addr).await
    } else {
        discovery::add_manual(&st, &addr).await
    }
}

#[tauri::command]
fn pair_info(app: tauri::AppHandle) -> Result<PairInfo, String> {
    let st = app.state::<Arc<AppState>>();
    let link = discovery::pair_link(&st.device_info(), &local_addrs(st.port));
    let svg = discovery::pair_svg(&link)?;
    Ok(PairInfo { link, svg })
}

#[tauri::command]
async fn describe_paths(st: AppStateRef<'_>, paths: Vec<String>) -> Result<Vec<StagedItem>, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || transfer::describe(&st, &paths))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_files(st: AppStateRef<'_>, peer_id: String, paths: Vec<String>) -> Result<String, String> {
    let peer = st.find_peer(&peer_id).ok_or("Không tìm thấy máy nhận")?;
    let state = st.inner().clone();
    let files = tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&state, &paths))
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
    use tauri_plugin_opener::OpenerExt;
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    platform::plugins(tauri::Builder::default())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let (data_dir, defaults) = platform::dirs(app.handle())?;
            let settings = identity::load_settings(&data_dir, defaults);
            let ident = identity::load_or_create_identity(&data_dir)?;
            let listener = server::bind()?;
            let port = listener.local_addr()?.port();

            let handle = app.handle().clone();
            let emit: state::EmitFn = Arc::new(move |event, payload| {
                platform::on_event(&handle, event, &payload);
                let _ = handle.emit(event, payload);
            });
            let mut st = AppState::new(emit, data_dir, ident, port, settings.clone());
            st.hooks = platform::hooks(app.handle());
            let st = Arc::new(st);
            app.manage(st.clone());

            let server_state = st.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = server::serve(server_state.clone(), listener).await {
                    server_state.emit("fatal-error", format!("Máy chủ nhận file dừng: {e}"));
                }
            });
            platform::before_discovery(app.handle());
            if let Err(e) = discovery::start(st.clone()) {
                st.emit("fatal-error", format!("Không bật được tự tìm máy: {e}"));
            }
            platform::after_setup(app.handle(), &settings)?;
            Ok(())
        })
        .on_window_event(|_window, _event| {
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = _event {
                tray::on_close_requested(_window, api);
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
            pair_info,
            describe_paths,
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
