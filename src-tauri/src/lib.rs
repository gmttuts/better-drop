mod client;
mod discovery;
mod identity;
mod models;
mod paths;
mod server;
mod state;
mod transfer;

use models::{Peer, SelfInfo, Settings};
use state::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
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

fn self_info(st: &AppState) -> SelfInfo {
    SelfInfo {
        info: st.device_info(),
        settings: st.settings.read().unwrap().clone(),
        addrs: local_addrs(st.port),
    }
}

#[tauri::command]
fn get_self_info(st: AppStateRef) -> SelfInfo {
    self_info(&st)
}

#[tauri::command]
fn get_peers(st: AppStateRef) -> Vec<Peer> {
    st.peer_list()
}

#[tauri::command]
fn update_settings(st: AppStateRef, name: String, download_dir: String) -> Result<SelfInfo, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Tên máy không được để trống".into());
    }
    if download_dir.trim().is_empty() {
        return Err("Chưa chọn thư mục lưu".into());
    }
    let new = {
        let mut s = st.settings.write().unwrap();
        *s = Settings {
            device_id: s.device_id.clone(),
            name,
            download_dir,
        };
        s.clone()
    };
    identity::save_settings(&st.data_dir, &new).map_err(|e| e.to_string())?;
    if let Some(d) = st.mdns.lock().unwrap().as_ref() {
        discovery::register(d, &st.device_info())?;
    }
    Ok(self_info(&st))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
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
                let _ = handle.emit(event, payload);
            });
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_self_info,
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
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    discovery::shutdown(&st);
                }
            }
        });
}

#[cfg(test)]
mod tests;
