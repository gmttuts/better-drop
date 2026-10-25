//! Đồng bộ clipboard giữa các máy tin cậy.
//!
//! Máy tính đọc clipboard mỗi giây; có văn bản mới do người dùng chép thì gửi tới các máy
//! tin cậy đã chọn qua kênh gửi văn bản sẵn có, đánh dấu `clipboard: true`. Máy nhận tin
//! cậy máy gửi (đúng chứng chỉ) và đã bật đồng bộ thì ghi thẳng vào clipboard, không hiện
//! hộp thoại; không thì xử lý như văn bản thường.
//!
//! Điện thoại không đọc được clipboard ở nền nên chỉ nhận, và gửi bằng nút "Gửi clipboard".
//! Các lượt đồng bộ không ghi vào Lịch sử: mỗi lần chép là một dòng thì Lịch sử thành rác,
//! mà nội dung đã nằm sẵn trong clipboard.

use crate::client::{api_url, build_client, describe_error, Pin};
use crate::models::*;
use crate::state::AppState;
use reqwest::StatusCode;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

/// Văn bản dài hơn thì không đồng bộ (vẫn gửi được bằng tab Văn bản).
pub const MAX_CLIP_BYTES: usize = 100 * 1024;
/// Đọc clipboard mỗi chừng này.
pub const POLL: Duration = Duration::from_secs(1);
/// Bản đầu tiên hiểu cờ `clipboard`. Máy bản cũ hơn sẽ hiện mỗi lần chép thành một hộp
/// thoại văn bản, nên không tự gửi tới đó.
const SINCE: &str = "0.3.7";

#[derive(Default)]
pub struct ClipState {
    /// Đã đọc clipboard lần đầu (nội dung có sẵn lúc mở app thì không gửi).
    primed: bool,
    /// Băm văn bản đọc được lần trước (bỏ qua chuỗi giống hệt lần trước).
    last_seen: Option<u64>,
    /// Băm văn bản vừa nhận và ghi vào clipboard (không gửi ngược lại).
    last_written: Option<u64>,
}

fn hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Văn bản đáng đồng bộ: không trống, không quá dài.
fn syncable(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MAX_CLIP_BYTES
}

/// Máy chạy bản `version` có hiểu cờ `clipboard` không. Bản đang phát triển (số phiên bản
/// chưa tăng) coi các máy cùng phiên bản là hiểu.
fn supports(version: &str) -> bool {
    let need = if crate::updater::is_newer(SINCE, APP_VERSION) { APP_VERSION } else { SINCE };
    !version.is_empty() && !crate::updater::is_newer(need, version)
}

/// Gọi mỗi lần đọc clipboard. Trả về văn bản cần gửi đi (mới, do người dùng chép, và đang
/// bật đồng bộ), hoặc None.
pub fn observe(st: &AppState, current: Option<&str>) -> Option<String> {
    let text = current?;
    let h = hash(text);
    let mut c = st.clip.lock().unwrap();
    let first = !std::mem::replace(&mut c.primed, true);
    if c.last_seen == Some(h) {
        return None;
    }
    c.last_seen = Some(h);
    if first || c.last_written == Some(h) || !syncable(text) {
        return None;
    }
    drop(c);
    st.devices.lock().unwrap().clipboard.enabled.then(|| text.to_string())
}

/// Các máy tin cậy đã chọn đồng bộ đang bật, ghim theo dấu vân tay đã tin cậy.
pub fn targets(st: &AppState) -> Vec<Peer> {
    let (enabled, chosen) = {
        let book = st.devices.lock().unwrap();
        let chosen: Vec<(String, String)> = book
            .trusted
            .iter()
            .filter(|t| book.clipboard.devices.contains(&t.id))
            .map(|t| (t.id.clone(), t.fp.clone()))
            .collect();
        (book.clipboard.enabled, chosen)
    };
    if !enabled {
        return vec![];
    }
    st.peer_list()
        .into_iter()
        .filter(|p| p.online && supports(&p.info.version))
        .filter_map(|p| {
            let (_, fp) = chosen.iter().find(|(id, _)| *id == p.info.id)?;
            Some(Peer { info: DeviceInfo { fp: fp.clone(), ..p.info }, ..p })
        })
        .collect()
}

/// Gửi văn bản tới một máy, đánh dấu là clipboard. Không ghi Lịch sử.
pub async fn send(st: &AppState, peer: &Peer, text: &str) -> Result<(), String> {
    let client = build_client(Pin::Fingerprint(peer.info.fp.clone()), Some(&st.identity))?;
    let req = PrepareRequest {
        sender: st.device_info(),
        kind: TransferKind::Text,
        files: vec![],
        text: Some(text.to_string()),
        group: None,
        thumbs: vec![],
        clipboard: true,
    };
    let mut last_err = "Máy nhận không có địa chỉ nào".to_string();
    for addr in &peer.addrs {
        let res = client
            .post(api_url(addr, "prepare"))
            .json(&req)
            .timeout(Duration::from_secs(10))
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) if r.status() == StatusCode::SERVICE_UNAVAILABLE => {
                return Err("Máy nhận đang tắt chế độ nhận file".into())
            }
            Ok(r) => return Err(format!("Máy nhận trả lỗi {}", r.status())),
            Err(e) => last_err = describe_error(&e),
        }
    }
    Err(last_err)
}

/// Gửi tới mọi máy đồng bộ; trả về số máy đã nhận.
pub async fn push(st: &Arc<AppState>, text: &str) -> usize {
    let peers = targets(st);
    let results = futures_util::future::join_all(peers.iter().map(|p| send(st, p, text))).await;
    let ok = results.iter().filter(|r| r.is_ok()).count();
    if !peers.is_empty() {
        st.emit("clipboard-sent", serde_json::json!({ "peers": ok, "chars": text.chars().count() }));
    }
    ok
}

/// Máy tính: đọc clipboard mỗi giây và gửi văn bản mới đi. `read` chạy trên luồng blocking.
pub fn spawn_watcher(st: Arc<AppState>, read: impl Fn() -> Option<String> + Send + Sync + 'static) {
    let read = Arc::new(read);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL).await;
            // Không bật đồng bộ thì vẫn đọc (để biết nội dung đang có), nhưng chỉ khi đã
            // bật mới cần đọc mỗi giây; tắt thì thưa hơn cho đỡ tốn.
            let enabled = st.devices.lock().unwrap().clipboard.enabled;
            if !enabled && st.clip.lock().unwrap().primed {
                tokio::time::sleep(POLL * 4).await;
            }
            let r = read.clone();
            let current = tokio::task::spawn_blocking(move || r()).await.ok().flatten();
            if let Some(text) = observe(&st, current.as_deref()) {
                let st = st.clone();
                tauri::async_runtime::spawn(async move {
                    push(&st, &text).await;
                });
            }
        }
    });
}

/// Văn bản đánh dấu clipboard từ máy này có được ghi thẳng vào clipboard không: máy gửi
/// phải chứng minh đúng chứng chỉ (TLS hai chiều), là máy tin cậy đã chọn đồng bộ.
pub fn accepts(st: &AppState, sender: &DeviceInfo, client_fp: Option<&str>) -> bool {
    let proven = client_fp.is_some_and(|fp| fp.eq_ignore_ascii_case(&sender.fp));
    proven && st.devices.lock().unwrap().clipboard_allowed(&sender.id, &sender.fp)
}

/// Ghi văn bản nhận được vào clipboard và báo nhỏ lên giao diện. Trả về false nếu không
/// ghi được (quá dài, không có clipboard): khi đó xử lý như văn bản thường.
pub async fn receive(st: &AppState, sender_name: &str, text: &str) -> bool {
    if !syncable(text) {
        return false;
    }
    let Some(write) = st.hooks.write_clipboard.clone() else { return false };
    let h = hash(text);
    {
        let c = st.clip.lock().unwrap();
        // Giống hệt nội dung vừa ghi: đã có trong clipboard rồi.
        if c.last_written == Some(h) && c.last_seen == Some(h) {
            return true;
        }
    }
    // Ghi trước khi ghi clipboard: vòng đọc kế tiếp có thể chạy ngay sau khi ghi.
    st.clip.lock().unwrap().last_written = Some(h);
    let owned = text.to_string();
    let ok = tokio::task::spawn_blocking(move || write(&owned)).await.is_ok_and(|r| r.is_ok());
    if !ok {
        st.clip.lock().unwrap().last_written = None;
        return false;
    }
    st.emit(
        "clipboard-received",
        serde_json::json!({ "sender_name": sender_name, "chars": text.chars().count() }),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_gate() {
        assert!(supports(APP_VERSION));
        assert!(supports("9.0.0"));
        assert!(!supports(""));
        assert!(!supports("0.3.5"));
    }
}
