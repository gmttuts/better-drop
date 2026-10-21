mod client;
mod devices;
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

/// Xin quyền mạng cục bộ (Android 17+). Được cấp thì dựng lại phần tìm máy ngay.
#[tauri::command]
async fn ensure_local_network(app: tauri::AppHandle) -> bool {
    let a = app.clone();
    let granted = tauri::async_runtime::spawn_blocking(move || platform::request_local_network(&a))
        .await
        .unwrap_or(false);
    if granted && cfg!(mobile) {
        app.state::<Arc<AppState>>()
            .rediscover
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    granted
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

#[derive(serde::Serialize)]
struct DevicesInfo {
    groups: Vec<devices::Group>,
    hidden: Vec<devices::DeviceRef>,
}

fn devices_info(st: &AppState) -> DevicesInfo {
    let book = st.devices.lock().unwrap();
    DevicesInfo {
        groups: book.groups.clone(),
        hidden: book.hidden.clone(),
    }
}

/// Lưu sổ thiết bị sau khi sửa, rồi báo UI cập nhật danh sách máy và nhóm.
fn change_devices(st: &AppState, f: impl FnOnce(&mut devices::DeviceBook)) -> Result<DevicesInfo, String> {
    {
        let mut book = st.devices.lock().unwrap();
        f(&mut book);
        book.save(&st.data_dir).map_err(|e| e.to_string())?;
    }
    st.emit_peers();
    st.emit_devices();
    Ok(devices_info(st))
}

#[tauri::command]
fn get_devices(st: AppStateRef) -> DevicesInfo {
    devices_info(&st)
}

/// Quên một máy: xoá máy đã lưu, và ẩn nó khỏi danh sách (kể cả khi tự tìm thấy lại).
#[tauri::command]
fn forget_peer(st: AppStateRef, id: String) -> Result<DevicesInfo, String> {
    let dev = st
        .peer_list()
        .into_iter()
        .find(|p| p.info.id == id)
        .map(|p| devices::DeviceRef { id: p.info.id, name: p.info.name, os: p.info.os })
        .unwrap_or(devices::DeviceRef { id: id.clone(), name: "Máy không tên".into(), os: String::new() });
    st.peers.lock().unwrap().remove(&format!("manual:{id}"));
    change_devices(&st, |b| b.forget(dev))
}

#[tauri::command]
fn unhide_peer(st: AppStateRef, id: String) -> Result<DevicesInfo, String> {
    change_devices(&st, |b| b.unhide(&id))
}

#[tauri::command]
fn save_group(
    st: AppStateRef,
    id: Option<String>,
    name: String,
    members: Vec<devices::DeviceRef>,
) -> Result<DevicesInfo, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Tên nhóm không được để trống".into());
    }
    if members.is_empty() {
        return Err("Nhóm cần ít nhất một máy".into());
    }
    change_devices(&st, |b| {
        b.save_group(id, name, members);
    })
}

#[tauri::command]
fn delete_group(st: AppStateRef, id: String) -> Result<DevicesInfo, String> {
    change_devices(&st, |b| b.delete_group(&id))
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
            tauri::async_runtime::spawn(discovery::health_check_loop(st.clone()));
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
            ensure_local_network,
            get_peers,
            update_settings,
            add_manual_peer,
            pair_info,
            get_devices,
            forget_peer,
            unhide_peer,
            save_group,
            delete_group,
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
