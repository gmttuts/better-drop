mod client;
mod devices;
mod history;
mod discovery;
mod identity;
mod models;
mod online;
mod paths;
mod platform;
mod server;
mod state;
mod transfer;
mod updater;
#[cfg(desktop)]
mod tray;
mod web;

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
fn set_language(app: tauri::AppHandle, language: String) -> Result<SelfInfo, String> {
    if !["auto", "vi", "en"].contains(&language.as_str()) {
        return Err("Ngôn ngữ không hợp lệ".into());
    }
    let info = change_settings(&app, |s| s.language = language)?;
    #[cfg(desktop)]
    tray::apply_language(&app);
    Ok(info)
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    updater::check(&app).await
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    updater::install(&app).await
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

/// Lấy (và xoá) các mục được chia sẻ vào từ app khác.
#[tauri::command]
async fn take_shared_items(app: tauri::AppHandle) -> state::SharedItems {
    let a = app.clone();
    // Android: lấy thêm từ menu Chia sẻ (plugin giữ lại intent).
    let _ = tauri::async_runtime::spawn_blocking(move || platform::pull_shared(&a)).await;
    let st = app.state::<Arc<AppState>>();
    let items = std::mem::take(&mut *st.shared.lock().unwrap());
    items
}

/// Đường dẫn file/thư mục trong tham số dòng lệnh (Windows "Send to", mở bằng app…).
pub(crate) fn paths_from_args<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .filter(|a| std::path::Path::new(a).exists())
        .collect()
}

/// Văn bản trong đường dẫn `betterdrop://share?text=…` (Share Extension trên iOS mở app bằng đường dẫn này).
#[cfg_attr(not(any(target_os = "macos", target_os = "ios", test)), allow(dead_code))]
pub(crate) fn shared_text_from_url(url: &tauri::Url) -> Option<String> {
    if url.scheme() != discovery::LINK_SCHEME || url.host_str() != Some("share") {
        return None;
    }
    url.query_pairs().find(|(k, _)| k == "text").map(|(_, v)| v.into_owned()).filter(|t| !t.is_empty())
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
    trusted: Vec<devices::TrustedDevice>,
}

fn devices_info(st: &AppState) -> DevicesInfo {
    let book = st.devices.lock().unwrap();
    DevicesInfo {
        groups: book.groups.clone(),
        hidden: book.hidden.clone(),
        trusted: book.trusted.clone(),
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

// ---- Chia sẻ qua trình duyệt ----

fn local_ips(st: &AppState) -> Vec<String> {
    local_addrs(st.port)
        .into_iter()
        .filter_map(|a| a.rsplit_once(':').map(|(ip, _)| ip.to_string()))
        .collect()
}

fn web_info(st: &AppState) -> Option<web::WebInfo> {
    web::info(st, &local_ips(st))
}

#[tauri::command]
fn web_share_status(st: AppStateRef) -> Option<web::WebInfo> {
    web_info(&st)
}

#[tauri::command]
async fn web_share_start(st: AppStateRef<'_>) -> Result<web::WebInfo, String> {
    web::start(st.inner().clone()).await?;
    web_info(&st).ok_or_else(|| "Không bật được chia sẻ qua trình duyệt".into())
}

#[tauri::command]
fn web_share_stop(st: AppStateRef) {
    web::stop(&st);
}

#[tauri::command]
fn web_share_new_pin(st: AppStateRef) -> Option<web::WebInfo> {
    web::renew_pin(&st);
    web_info(&st)
}

/// Chia sẻ các file/thư mục đã chọn cho trình duyệt tải về (thay danh sách cũ).
#[tauri::command]
async fn web_share_set_files(st: AppStateRef<'_>, paths: Vec<String>) -> Result<Option<web::WebInfo>, String> {
    let state = st.inner().clone();
    let files = if paths.is_empty() {
        Vec::new()
    } else {
        tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&state, &paths))
            .await
            .map_err(|e| e.to_string())??
    };
    web::set_files(&st, files);
    Ok(web_info(&st))
}

/// Thêm file/thư mục vào danh sách chia sẻ (giữ các file đã chia sẻ trước đó).
#[tauri::command]
async fn web_share_add_files(st: AppStateRef<'_>, paths: Vec<String>) -> Result<Option<web::WebInfo>, String> {
    let state = st.inner().clone();
    let files = tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&state, &paths))
        .await
        .map_err(|e| e.to_string())??;
    web::add_files(&st, files);
    Ok(web_info(&st))
}

#[tauri::command]
fn web_share_remove_file(st: AppStateRef, id: String) -> Option<web::WebInfo> {
    web::remove_file(&st, &id);
    web_info(&st)
}

#[tauri::command]
fn web_share_set_text(st: AppStateRef, text: Option<String>) -> Option<web::WebInfo> {
    web::set_text(&st, text);
    web_info(&st)
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

/// Trả lời yêu cầu nhận. `trust`: đồng thời đánh dấu "Luôn chấp nhận từ máy này"
/// (chỉ có tác dụng khi máy gửi đã chứng minh danh tính).
#[tauri::command]
fn respond_request(st: AppStateRef, request_id: String, accept: bool, trust: Option<bool>) {
    let Some(p) = st.pending.lock().unwrap().remove(&request_id) else { return };
    if accept && trust == Some(true) {
        if let Some(dev) = p.trust_candidate {
            let _ = change_devices(&st, |b| b.trust(dev));
        }
    }
    let _ = p.tx.send(accept);
}

/// Chia sẻ nhóm `group_id` cho các máy `peer_ids`. Trả về lỗi của từng máy (nếu có).
#[tauri::command]
async fn share_group(st: AppStateRef<'_>, group_id: String, peer_ids: Vec<String>) -> Result<Vec<String>, String> {
    let group = st
        .devices
        .lock()
        .unwrap()
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .cloned()
        .ok_or("Không tìm thấy nhóm")?;
    let peers = st.peer_list();
    let members = group
        .members
        .iter()
        .map(|m| {
            let p = peers.iter().find(|p| p.info.id == m.id);
            models::SharedMember {
                id: m.id.clone(),
                name: p.map(|p| p.info.name.clone()).unwrap_or_else(|| m.name.clone()),
                os: m.os.clone(),
                fp: p.map(|p| p.info.fp.clone()).unwrap_or_default(),
                addrs: p.map(|p| p.addrs.clone()).unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();
    let shared = models::SharedGroup { name: group.name.clone(), members };
    let tasks = peer_ids.into_iter().map(|id| {
        let st = st.inner().clone();
        let shared = shared.clone();
        async move {
            let peer = st.find_peer(&id).ok_or_else(|| "Không tìm thấy máy nhận".to_string())?;
            let name = peer.info.name.clone();
            transfer::share_group(st, peer, shared).await.map_err(|e| format!("{name}: {e}"))
        }
    });
    let results = futures_util::future::join_all(tasks).await;
    Ok(results.into_iter().filter_map(Result::err).collect())
}

#[tauri::command]
fn untrust_device(st: AppStateRef, id: String) -> Result<DevicesInfo, String> {
    change_devices(&st, |b| b.untrust(&id))
}

#[tauri::command]
fn cancel_transfer(st: AppStateRef, id: String) {
    transfer::cancel(&st, &id);
}

// ---- Lịch sử ----

#[tauri::command]
fn get_history(st: AppStateRef, limit: Option<usize>) -> Vec<history::HistoryEntry> {
    st.history.lock().unwrap().list(limit.unwrap_or(500))
}

#[tauri::command]
fn delete_history_item(st: AppStateRef, id: String) -> Result<(), String> {
    st.history.lock().unwrap().delete(&id).map_err(|e| e.to_string())
}

/// Chỉ xoá bản ghi lịch sử, không xoá file đã gửi/nhận.
#[tauri::command]
fn clear_history(st: AppStateRef) -> Result<(), String> {
    st.history.lock().unwrap().clear().map_err(|e| e.to_string())
}

/// Hiện file trong Finder/Explorer (chọn sẵn file đó).
#[tauri::command]
fn reveal_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !std::path::Path::new(&path).exists() {
        return Err("File không còn ở vị trí cũ (đã bị xoá hoặc di chuyển)".into());
    }
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Gửi lại một lượt trong lịch sử (file gốc hoặc văn bản) tới một máy.
#[tauri::command]
async fn resend_history(st: AppStateRef<'_>, id: String, peer_id: String) -> Result<(), String> {
    let entry = st
        .history
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or("Không tìm thấy mục này trong lịch sử")?;
    let peer = st.find_peer(&peer_id).ok_or("Không tìm thấy máy nhận")?;
    if let Some(text) = entry.text {
        return transfer::send_text(st.inner().clone(), peer, text).await;
    }
    let paths: Vec<String> = entry
        .files
        .iter()
        .filter_map(|f| f.path.clone())
        .filter(|p| p.starts_with("content://") || p.starts_with("file://") || std::path::Path::new(p).exists())
        .collect();
    if paths.is_empty() {
        return Err("Các file gốc không còn ở vị trí cũ".into());
    }
    let state = st.inner().clone();
    let files = tauri::async_runtime::spawn_blocking(move || transfer::collect_files(&state, &paths))
        .await
        .map_err(|e| e.to_string())??;
    transfer::start_send_files(st.inner().clone(), peer, files);
    Ok(())
}

// ---- Gửi qua Internet ----

#[tauri::command]
fn online_providers() -> Vec<online::providers::ProviderInfo> {
    online::providers()
}

#[tauri::command]
async fn online_send(
    st: AppStateRef<'_>,
    paths: Vec<String>,
    provider: online::providers::Provider,
) -> Result<String, String> {
    online::send(st.inner().clone(), paths, provider).await
}

#[tauri::command]
async fn online_receive_link(st: AppStateRef<'_>, link: String) -> Result<online::ReceiveStart, String> {
    online::receive_link(st.inner().clone(), link).await
}

#[tauri::command]
async fn online_receive_file(st: AppStateRef<'_>, link: String, path: String) -> Result<String, String> {
    online::receive_file(st.inner().clone(), link, path).await
}

#[tauri::command]
fn online_qr(link: String) -> Result<String, String> {
    online::qr_svg(&link)
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
            set_language,
            check_update,
            install_update,
            take_shared_items,
            ensure_local_network,
            get_peers,
            update_settings,
            add_manual_peer,
            pair_info,
            web_share_status,
            web_share_start,
            web_share_stop,
            web_share_new_pin,
            web_share_set_files,
            web_share_set_text,
            web_share_add_files,
            web_share_remove_file,
            get_devices,
            forget_peer,
            unhide_peer,
            save_group,
            delete_group,
            describe_paths,
            send_files,
            send_text,
            respond_request,
            untrust_device,
            share_group,
            cancel_transfer,
            open_folder,
            get_history,
            delete_history_item,
            clear_history,
            reveal_path,
            resend_history,
            online_providers,
            online_send,
            online_receive_link,
            online_receive_file,
            online_qr,
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
            // macOS: "Mở bằng BetterDrop" / thả file vào biểu tượng Dock.
            // iOS: "Mở bằng BetterDrop" trong menu Chia sẻ (giữ dạng file:// để iOS cho đọc).
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            tauri::RunEvent::Opened { urls } => {
                let items: Vec<String> = urls
                    .iter()
                    .filter(|u| u.scheme() == "file")
                    .filter_map(|u| {
                        if cfg!(target_os = "ios") {
                            Some(u.to_string())
                        } else {
                            u.to_file_path().ok().map(|p| p.to_string_lossy().into_owned())
                        }
                    })
                    .collect();
                // iOS: Share Extension không có App Group thì gửi văn bản/link qua betterdrop://share?text=…
                let text = urls.iter().find_map(shared_text_from_url);
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    st.add_shared(items, text);
                }
                #[cfg(desktop)]
                tray::show_main(app);
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests;
