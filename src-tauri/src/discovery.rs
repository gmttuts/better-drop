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
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    register(&daemon, &st.device_info())?;
    let rx = daemon.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
    *st.mdns.lock().unwrap() = Some(daemon);

    let my_id = st.device_info().id;
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            match ev {
                ServiceEvent::ServiceResolved(svc) => {
                    let Some(peer) = peer_from(&svc) else { continue };
                    if peer.info.id == my_id {
                        continue;
                    }
                    st.peers.lock().unwrap().insert(svc.get_fullname().to_string(), peer);
                    st.emit_peers();
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    if st.peers.lock().unwrap().remove(&fullname).is_some() {
                        st.emit_peers();
                    }
                }
                _ => {}
            }
        }
    });
    Ok(())
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

/// Định kỳ kiểm tra các máy thêm thủ công còn bật không (để hiện mờ máy đang tắt).
pub async fn health_check_loop(st: Arc<AppState>) {
    loop {
        let manual: Vec<(String, Peer)> = st
            .peers
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with("manual:"))
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
