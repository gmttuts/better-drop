//! Tự tìm các máy chạy BetterDrop trong cùng mạng LAN qua mDNS (Bonjour).

use crate::client::{api_url, build_client, describe_error, Pin};
use crate::models::{DeviceInfo, Peer};
use crate::state::AppState;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const SERVICE_TYPE: &str = "_betterdrop._tcp.local.";

pub fn start(st: Arc<AppState>) -> Result<(), String> {
    if let Some(native) = st.hooks.native_discovery.clone() {
        return start_native(st, native);
    }
    let (daemon, rx) = start_mdns(&st)?;
    std::thread::spawn(move || watch_mdns(st, daemon, rx));
    Ok(())
}

/// Dựng daemon mDNS mới: quảng bá máy này và bắt đầu tìm máy khác.
fn start_mdns(st: &AppState) -> Result<(ServiceDaemon, mdns_sd::Receiver<ServiceEvent>), String> {
    if let Some(hook) = &st.hooks.before_mdns {
        hook();
    }
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    register(&daemon, &st.device_info())?;
    let rx = daemon.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
    *st.mdns.lock().unwrap() = Some(daemon.clone());
    Ok((daemon, rx))
}

fn watch_mdns(st: Arc<AppState>, mut daemon: ServiceDaemon, mut rx: mdns_sd::Receiver<ServiceEvent>) {
    let my_id = st.device_info().id;
    let mut ips = local_ipv4();
    // Số lần còn phải dựng lại sau khi mạng thay đổi. Phải lặp vài lần vì lúc vừa
    // phát hiện thay đổi, Wi‑Fi có thể chưa kết nối xong.
    let mut retries = 0u8;
    loop {
        match rx.recv_timeout(Duration::from_secs(4)) {
            Ok(ServiceEvent::ServiceResolved(svc)) => {
                let Some(mut peer) = peer_from(&svc) else { continue };
                if peer.info.id == my_id {
                    continue;
                }
                let mut peers = st.peers.lock().unwrap();
                // Giữ kết quả kiểm tra kết nối cũ nếu địa chỉ không đổi (tránh nhấp nháy).
                if let Some(old) = peers.get(svc.get_fullname()) {
                    if old.addrs == peer.addrs {
                        peer.online = old.online;
                    }
                }
                peers.insert(svc.get_fullname().to_string(), peer);
                drop(peers);
                st.emit_peers();
            }
            Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                if st.peers.lock().unwrap().remove(&fullname).is_some() {
                    st.emit_peers();
                }
            }
            Ok(_) => {}
            Err(e) => {
                let disconnected = matches!(e, mdns_sd::RecvTimeoutError::Disconnected);
                if disconnected && st.mdns.lock().unwrap().is_none() {
                    return; // App đang thoát.
                }
                let now = local_ipv4();
                if st.rediscover.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    retries = retries.max(2);
                }
                if now != ips {
                    // Mạng thay đổi (đổi Wi‑Fi, Wi‑Fi kết nối lại, cắm/rút dây):
                    // bỏ máy của mạng cũ, rồi dựng lại mDNS như lúc mới mở app.
                    ips = now;
                    retries = 3;
                    st.peers.lock().unwrap().retain(|k, _| k.starts_with("manual:") || k.starts_with("native:"));
                    st.emit_peers();
                }
                if disconnected {
                    retries = retries.max(1);
                }
                if retries > 0 {
                    retries -= 1;
                    let _ = daemon.shutdown();
                    match start_mdns(&st) {
                        Ok((d, r)) => {
                            daemon = d;
                            rx = r;
                        }
                        // Chưa có mạng: rx cũ đã đóng nên lượt sau sẽ thử lại.
                        Err(_) => std::thread::sleep(Duration::from_secs(3)),
                    }
                }
            }
        }
    }
}

/// Các địa chỉ IPv4 hiện có của máy này (để phát hiện mạng thay đổi).
fn local_ipv4() -> Vec<Ipv4Addr> {
    let mut ips: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
        .collect();
    ips.sort();
    ips.dedup();
    ips
}

/// Tìm máy bằng dịch vụ của hệ điều hành: hỏi lại danh sách mỗi 2 giây.
fn start_native(st: Arc<AppState>, native: Arc<dyn crate::state::NativeDiscovery>) -> Result<(), String> {
    let me = st.device_info();
    native.announce(&me)?;
    std::thread::spawn(move || loop {
        let found: HashMap<String, Peer> = native
            .peers()
            .into_iter()
            .filter(|p| p.info.id != me.id)
            .map(|p| (format!("native:{}", p.info.id), p))
            .collect();
        let changed = {
            let mut peers = st.peers.lock().unwrap();
            let before: Vec<String> = peers
                .iter()
                .filter(|(k, _)| k.starts_with("native:"))
                .map(|(k, p)| format!("{k}{}{:?}", p.info.name, p.addrs))
                .collect();
            peers.retain(|k, _| !k.starts_with("native:"));
            peers.extend(found);
            let mut after: Vec<String> = peers
                .iter()
                .filter(|(k, _)| k.starts_with("native:"))
                .map(|(k, p)| format!("{k}{}{:?}", p.info.name, p.addrs))
                .collect();
            let mut before = before;
            before.sort();
            after.sort();
            before != after
        };
        if changed {
            st.emit_peers();
        }
        std::thread::sleep(Duration::from_secs(2));
    });
    Ok(())
}

/// Cập nhật thông tin quảng bá sau khi đổi tên máy.
pub fn reannounce(st: &AppState) -> Result<(), String> {
    if let Some(native) = &st.hooks.native_discovery {
        return native.announce(&st.device_info());
    }
    if let Some(d) = st.mdns.lock().unwrap().as_ref() {
        register(d, &st.device_info())?;
    }
    Ok(())
}

/// Quảng bá (hoặc cập nhật) thông tin máy này.
pub fn register(daemon: &ServiceDaemon, me: &DeviceInfo) -> Result<(), String> {
    let props: HashMap<String, String> = [
        ("id", me.id.clone()),
        ("name", me.name.clone()),
        ("os", me.os.clone()),
        ("fp", me.fp.clone()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let host = format!("bd-{}.local.", &me.id[..8.min(me.id.len())]);
    let info = ServiceInfo::new(SERVICE_TYPE, &me.id, &host, "", me.port, props)
        .map_err(|e| e.to_string())?
        .enable_addr_auto();
    daemon.register(info).map_err(|e| e.to_string())
}

pub fn shutdown(st: &AppState) {
    if let Some(d) = st.mdns.lock().unwrap().take() {
        let fullname = format!("{}.{}", st.device_info().id, SERVICE_TYPE);
        if let Ok(rx) = d.unregister(&fullname) {
            let _ = rx.recv_timeout(Duration::from_millis(500));
        }
        let _ = d.shutdown();
    }
}

/// Ưu tiên địa chỉ LAN thông thường (192.168.x, 10.x, 172.16-31.x).
fn sort_addrs(addrs: &mut [Ipv4Addr]) {
    addrs.sort_by_key(|ip| (!ip.is_private(), ip.is_link_local(), *ip));
}

fn peer_from(svc: &mdns_sd::ResolvedService) -> Option<Peer> {
    let prop = |k: &str| svc.get_property_val_str(k).map(str::to_string);
    let mut ips: Vec<Ipv4Addr> = svc
        .get_addresses_v4()
        .into_iter()
        .filter(|ip| !ip.is_loopback())
        .collect();
    if ips.is_empty() {
        return None;
    }
    sort_addrs(&mut ips);
    let port = svc.get_port();
    Some(Peer {
        info: DeviceInfo {
            id: prop("id")?,
            name: prop("name").unwrap_or_else(|| "Không tên".into()),
            os: prop("os").unwrap_or_default(),
            fp: prop("fp")?,
            port,
        },
        addrs: ips.iter().map(|ip| format!("{ip}:{port}")).collect(),
        manual: false,
        online: true,
    })
}

/// Định kỳ kiểm tra máy nào kết nối được, để hiện mờ máy đang tắt hoặc không tới
/// được (vd mDNS quảng bá địa chỉ của mạng ảo/VPN) thay vì chỉ báo lỗi khi gửi.
/// Máy tìm qua Bonjour trên iOS được làm mới liên tục nên không cần kiểm tra ở đây.
pub async fn health_check_loop(st: Arc<AppState>) {
    loop {
        let manual: Vec<(String, Peer)> = st
            .peers
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| !k.starts_with("native:"))
            .map(|(k, p)| (k.clone(), p.clone()))
            .collect();
        let mut changed = false;
        for (key, peer) in manual {
            let online = ping(&peer).await;
            if let Some(p) = st.peers.lock().unwrap().get_mut(&key) {
                if p.online != online {
                    p.online = online;
                    changed = true;
                }
            }
        }
        if changed {
            st.emit_peers();
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

async fn ping(peer: &Peer) -> bool {
    let Ok(client) = build_client(Pin::Fingerprint(peer.info.fp.clone())) else {
        return false;
    };
    for addr in &peer.addrs {
        let ok = client
            .get(api_url(addr, "info"))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success());
        if ok {
            return true;
        }
    }
    false
}

/// Thêm máy bằng địa chỉ IP (khi mạng chặn mDNS). Tin chứng chỉ ở lần đầu.
pub async fn add_manual(st: &AppState, addr: &str) -> Result<Peer, String> {
    let addr = addr.trim();
    let addr = if addr.contains(':') {
        addr.to_string()
    } else {
        format!("{addr}:{}", crate::server::DEFAULT_PORT)
    };
    let slot = Arc::new(Mutex::new(None));
    let client = build_client(Pin::Capture(slot.clone()))?;
    let info: DeviceInfo = client
        .get(api_url(&addr, "info"))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .map_err(|e| describe_error(&e))?
        .json()
        .await
        .map_err(|_| "Địa chỉ này không phải BetterDrop".to_string())?;
    let seen_fp = slot.lock().unwrap().clone().unwrap_or_default();
    if !seen_fp.eq_ignore_ascii_case(&info.fp) {
        return Err("Chứng chỉ không khớp với thông tin máy trả về".into());
    }
    if info.id == st.device_info().id {
        return Err("Đây là chính máy này".into());
    }
    let peer = Peer {
        info: DeviceInfo { fp: seen_fp, ..info },
        addrs: vec![addr],
        manual: true,
        online: true,
    };
    st.remember_peer(peer.clone());
    Ok(peer)
}

pub const LINK_SCHEME: &str = "betterdrop";

/// Liên kết ghép nối mã hoá trong QR: chứa id, tên, fingerprint chứng chỉ và các
/// địa chỉ của máy này. Vì có sẵn fingerprint nên máy quét kiểm tra được chứng chỉ
/// ngay từ lần kết nối đầu (an toàn hơn nhập IP tay).
pub fn pair_link(me: &DeviceInfo, addrs: &[String]) -> String {
    let mut params: Vec<(&str, &str)> = vec![
        ("id", &me.id),
        ("name", &me.name),
        ("os", &me.os),
        ("fp", &me.fp),
    ];
    params.extend(addrs.iter().map(|a| ("a", a.as_str())));
    let url = reqwest::Url::parse_with_params(&format!("{LINK_SCHEME}://pair"), params)
        .expect("URL cố định luôn hợp lệ");
    url.to_string()
}

pub fn pair_svg(link: &str) -> Result<String, String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(link.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

/// Thêm máy từ liên kết ghép nối (quét QR hoặc dán vào ô nhập).
pub async fn add_from_link(st: &AppState, link: &str) -> Result<Peer, String> {
    let url = reqwest::Url::parse(link.trim()).map_err(|_| "Liên kết không hợp lệ".to_string())?;
    if url.scheme() != LINK_SCHEME {
        return Err("Đây không phải mã QR của BetterDrop".into());
    }
    let get = |k: &str| {
        url.query_pairs()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.into_owned())
    };
    let (Some(id), Some(fp)) = (get("id"), get("fp")) else {
        return Err("Mã QR thiếu thông tin".into());
    };
    if id == st.device_info().id {
        return Err("Đây là mã QR của chính máy này".into());
    }
    let addrs: Vec<String> = url
        .query_pairs()
        .filter(|(k, _)| k == "a")
        .map(|(_, v)| v.into_owned())
        .collect();
    if addrs.is_empty() {
        return Err("Máy kia không có địa chỉ mạng nào".into());
    }

    // Thử từng địa chỉ, chỉ chấp nhận đúng chứng chỉ ghi trong QR.
    let client = build_client(Pin::Fingerprint(fp.clone()))?;
    let mut reachable = Vec::new();
    let mut info: Option<DeviceInfo> = None;
    let mut last_err = String::new();
    for addr in &addrs {
        let res = client
            .get(api_url(addr, "info"))
            .timeout(Duration::from_secs(4))
            .send()
            .await;
        match res {
            Ok(r) => match r.json::<DeviceInfo>().await {
                Ok(i) if i.id == id => {
                    reachable.push(addr.clone());
                    info.get_or_insert(i);
                }
                _ => last_err = "Máy trả lời không khớp mã QR".into(),
            },
            Err(e) => last_err = describe_error(&e),
        }
    }
    let Some(info) = info else {
        return Err(format!(
            "Không kết nối được tới máy trong mã QR. Hai máy có cùng mạng Wi‑Fi không? ({last_err})"
        ));
    };
    let peer = Peer {
        info: DeviceInfo { fp, ..info },
        addrs: reachable,
        manual: true,
        online: true,
    };
    st.remember_peer(peer.clone());
    Ok(peer)
}
