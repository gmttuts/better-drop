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
    })
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
    };
    st.peers
        .lock()
        .unwrap()
        .insert(format!("manual:{}", peer.info.id), peer.clone());
    st.emit_peers();
    Ok(peer)
}
