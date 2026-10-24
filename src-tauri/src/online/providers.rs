//! Các dịch vụ lưu file tạm (không cần tài khoản). Thêm dịch vụ mới: thêm một biến
//! thể vào `Provider`, điền các thông số trong `info`, viết hàm upload (và download nếu
//! dịch vụ cho tải thẳng), rồi thêm vào trang nhận `website/nhan/`.
//!
//! Kết quả kiểm chứng từng dịch vụ: `docs/GUI-QUA-INTERNET.md`.

use crate::state::Tracker;
use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Litterbox,
    StorageTo,
    TempSh,
}

/// Thông số hiển thị trong ô chọn dịch vụ.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: Provider,
    pub name: &'static str,
    /// Dung lượng tối đa (sau khi mã hoá) cho một lượt gửi.
    pub max_bytes: u64,
    /// Số ngày giữ file.
    pub days: u32,
    /// App BetterDrop tự tải được từ link (không thì người nhận tải file .bdrop từ trang
    /// của dịch vụ rồi mở bằng app hoặc trang nhận).
    pub app_download: bool,
    /// Trang nhận trên website tự tải được (dịch vụ cho phép CORS).
    pub browser_download: bool,
}

pub const ALL: &[Provider] = &[Provider::Litterbox, Provider::StorageTo, Provider::TempSh];
const GB: u64 = 1_000_000_000;

impl Provider {
    pub fn info(self) -> ProviderInfo {
        match self {
            Provider::Litterbox => ProviderInfo {
                id: self,
                name: "Litterbox",
                max_bytes: GB,
                days: 3,
                app_download: true,
                browser_download: true,
            },
            Provider::StorageTo => ProviderInfo {
                id: self,
                name: "storage.to",
                max_bytes: 25 * GB,
                days: 7,
                app_download: false,
                browser_download: false,
            },
            Provider::TempSh => ProviderInfo {
                id: self,
                name: "temp.sh",
                max_bytes: 4 * GB,
                days: 3,
                app_download: true,
                browser_download: false,
            },
        }
    }

    /// Mã ngắn trong link.
    pub fn code(self) -> &'static str {
        match self {
            Provider::Litterbox => "lb",
            Provider::StorageTo => "st",
            Provider::TempSh => "ts",
        }
    }

    pub fn from_code(c: &str) -> Option<Provider> {
        ALL.iter().copied().find(|p| p.code() == c)
    }

    /// Mã file hợp lệ của dịch vụ (chặn link giả chèn đường dẫn lạ).
    pub fn valid_id(self, id: &str) -> bool {
        let ok = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        match self {
            Provider::StorageTo => (6..=32).contains(&id.len()) && id.chars().all(ok),
            // "abc123.bdrop": tên ngẫu nhiên + đuôi file.
            Provider::Litterbox => match id.split_once('.') {
                Some((name, ext)) => {
                    (4..=32).contains(&name.len())
                        && name.chars().all(|c| c.is_ascii_alphanumeric())
                        && !ext.is_empty()
                        && ext.len() <= 10
                        && ext.chars().all(|c| c.is_ascii_alphanumeric())
                }
                None => false,
            },
            Provider::TempSh => match id.split_once('/') {
                Some((code, name)) => {
                    !code.is_empty()
                        && code.chars().all(ok)
                        && !name.is_empty()
                        && !name.starts_with('.')
                        && name.chars().all(|c| ok(c) || c == '.')
                }
                None => false,
            },
        }
    }

    /// Trang tải của dịch vụ (người nhận tự bấm tải file .bdrop ở đây).
    pub fn page_url(self, id: &str) -> String {
        match self {
            Provider::Litterbox => format!("https://litter.catbox.moe/{id}"),
            Provider::StorageTo => format!("https://storage.to/{id}"),
            Provider::TempSh => format!("https://temp.sh/{id}"),
        }
    }
}

/// Tên file đã mã hoá đưa lên dịch vụ (không lộ tên thật).
pub const UPLOAD_NAME: &str = "betterdrop.bdrop";
const CONTENT_TYPE: &str = "application/octet-stream";
const RETRIES: u32 = 4;

pub type Source = Arc<Mutex<Box<dyn Read + Send>>>;

/// Địa chỉ gốc của dịch vụ. Trong test được thay bằng máy chủ giả lập (`test_base`).
fn base(default: &str) -> String {
    #[cfg(test)]
    if let Some(b) = test_base::get() {
        return b;
    }
    default.to_string()
}

#[cfg(test)]
pub mod test_base {
    use std::sync::Mutex;
    static BASE: Mutex<Option<String>> = Mutex::new(None);
    pub fn set(b: Option<String>) {
        *BASE.lock().unwrap() = b;
    }
    pub fn get() -> Option<String> {
        BASE.lock().unwrap().clone()
    }
}

pub struct Upload {
    pub client: reqwest::Client,
    /// Dữ liệu đã mã hoá.
    pub source: Source,
    pub len: u64,
    pub tracker: Arc<Tracker>,
    /// Mã khách cố định của máy này (storage.to tính hạn mức theo mã này).
    pub visitor: String,
}

pub struct Uploaded {
    pub id: String,
    /// Hết hạn lúc (giây kể từ 1970).
    pub expires_at: u64,
}

fn cancelled_err() -> String {
    "Đã huỷ".to_string()
}

fn net_err(e: reqwest::Error) -> String {
    if e.is_connect() || e.is_timeout() {
        format!("Mất kết nối Internet ({})", crate::client::describe_error(&e))
    } else {
        crate::client::describe_error(&e)
    }
}

/// Client ra Internet (chứng chỉ gốc webpki, như phần kiểm tra bản mới).
pub fn internet_client() -> Result<reqwest::Client, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .user_agent(format!("BetterDrop/{}", crate::models::APP_VERSION))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(120))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

/// Đọc `n` byte tiếp theo của nguồn (chạy trên luồng riêng vì đọc đĩa và mã hoá).
async fn read_block(source: &Source, n: usize) -> Result<Vec<u8>, String> {
    let source = source.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut buf = vec![0u8; n];
        let mut got = 0;
        let mut src = source.lock().unwrap();
        while got < n {
            match src.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(e) => return Err(e.to_string()),
            }
        }
        buf.truncate(got);
        Ok(buf)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Thân yêu cầu từ một khối đã có trong RAM: báo tiến trình và dừng khi bị huỷ.
/// `counted` cộng dồn số byte đã tính vào tiến trình (để trừ lại nếu phải gửi lại).
fn progress_body(data: Bytes, tracker: Arc<Tracker>, counted: Arc<AtomicU64>) -> reqwest::Body {
    const STEP: usize = 256 * 1024;
    let stream = futures_util::stream::iter((0..data.len()).step_by(STEP).map(move |i| {
        if tracker.is_cancelled() {
            return Err(std::io::Error::other("Đã huỷ"));
        }
        let piece = data.slice(i..(i + STEP).min(data.len()));
        tracker.add(piece.len() as u64);
        counted.fetch_add(piece.len() as u64, Ordering::Relaxed);
        Ok::<Bytes, std::io::Error>(piece)
    }));
    reqwest::Body::wrap_stream(stream)
}

/// PUT một khối lên URL ký sẵn, thử lại khi rớt mạng. Trả về ETag.
/// `refresh`: xin URL mới (URL ký sẵn có hạn).
async fn put_with_retry<F, Fut>(u: &Upload, mut url: String, data: Vec<u8>, mut refresh: F) -> Result<String, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let data = Bytes::from(data);
    let mut last_err = String::new();
    for attempt in 0..=RETRIES {
        if u.tracker.is_cancelled() {
            return Err(cancelled_err());
        }
        if attempt > 0 {
            u.tracker.set_note(Some(format!("Mất kết nối, đang thử lại ({attempt}/{RETRIES})…")));
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
        }
        let counted = Arc::new(AtomicU64::new(0));
        let res = u
            .client
            .put(&url)
            .header(reqwest::header::CONTENT_LENGTH, data.len())
            .header(reqwest::header::CONTENT_TYPE, CONTENT_TYPE)
            .body(progress_body(data.clone(), u.tracker.clone(), counted.clone()))
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => {
                if attempt > 0 {
                    u.tracker.set_note(None);
                }
                let etag = r
                    .headers()
                    .get(reqwest::header::ETAG)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                return Ok(etag);
            }
            Ok(r) => {
                u.tracker.sub(counted.load(Ordering::Relaxed));
                last_err = format!("Dịch vụ từ chối (HTTP {})", r.status().as_u16());
                // URL ký sẵn hết hạn: xin URL mới rồi thử lại.
                if r.status() == reqwest::StatusCode::FORBIDDEN {
                    url = refresh().await?;
                } else if r.status().is_client_error() {
                    return Err(last_err);
                }
            }
            Err(e) => {
                u.tracker.sub(counted.load(Ordering::Relaxed));
                if u.tracker.is_cancelled() {
                    return Err(cancelled_err());
                }
                last_err = net_err(e);
            }
        }
    }
    Err(last_err)
}

pub async fn upload(p: Provider, u: &Upload) -> Result<Uploaded, String> {
    match p {
        Provider::Litterbox => litterbox::upload(u).await,
        Provider::StorageTo => storage_to::upload(u).await,
        Provider::TempSh => temp_sh::upload(u).await,
    }
}

/// Tải file đã mã hoá về (chỉ với dịch vụ có `app_download`). Trả về luồng byte và
/// độ dài (nếu biết).
pub async fn download(
    p: Provider,
    client: &reqwest::Client,
    id: &str,
) -> Result<(reqwest::Response, Option<u64>), String> {
    match p {
        Provider::Litterbox => litterbox::download(client, id).await,
        Provider::TempSh => temp_sh::download(client, id).await,
        Provider::StorageTo => Err("storage.to không cho tải thẳng".into()),
    }
}

/// Đổi "2026-04-15T12:00:00Z" (hoặc có phần lẻ giây / múi giờ) ra giây kể từ 1970.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Ngày kể từ 1970 ("days from civil").
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut secs = days * 86_400 + h * 3600 + mi * 60 + se;
    // Múi giờ "+07:00" / "-05:00" ở cuối.
    let rest = &s[19..];
    if let Some(pos) = rest.find(['+', '-']) {
        let tz = &rest[pos..];
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let hh = tz.get(1..3)?.parse::<i64>().ok()?;
        let mm = tz.get(4..6).and_then(|m| m.parse::<i64>().ok()).unwrap_or(0);
        secs -= sign * (hh * 3600 + mm * 60);
    }
    u64::try_from(secs).ok()
}

mod storage_to {
    //! https://storage.to/docs/api — init → PUT lên URL ký sẵn (chia phần nếu > 50 MB)
    //! → confirm. Mỗi phần được giữ trong RAM nên rớt mạng thì gửi lại đúng phần đó.
    use super::*;
    use std::collections::HashMap;

    /// Cỡ phần lớn nhất chấp nhận (tránh giữ quá nhiều trong RAM).
    const MAX_PART: u64 = 128 << 20;

    #[derive(Deserialize)]
    struct Init {
        #[serde(default)]
        success: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(default)]
        upload_url: Option<String>,
        r2_key: Option<String>,
        #[serde(default)]
        upload_id: Option<String>,
        #[serde(default)]
        part_size: Option<u64>,
        #[serde(default)]
        total_parts: Option<u64>,
        #[serde(default)]
        initial_urls: HashMap<String, String>,
        #[serde(default)]
        owner_token: Option<String>,
    }

    #[derive(Deserialize)]
    struct PartUrl {
        #[serde(rename = "partNumber")]
        part_number: u64,
        url: String,
    }

    #[derive(Deserialize)]
    struct Parts {
        #[serde(default)]
        part_urls: Vec<PartUrl>,
        #[serde(default)]
        urls: HashMap<String, String>,
        #[serde(default)]
        error: Option<String>,
    }

    #[derive(Deserialize)]
    struct FileInfo {
        id: String,
        #[serde(default)]
        expires_at: Option<String>,
    }

    #[derive(Deserialize)]
    struct Confirm {
        #[serde(default)]
        success: bool,
        #[serde(default)]
        error: Option<String>,
        file: Option<FileInfo>,
    }

    fn api_err(status: reqwest::StatusCode, error: Option<String>) -> String {
        match (status.as_u16(), error) {
            (429, e) => format!(
                "storage.to: đã dùng hết hạn mức hôm nay{}",
                e.map(|e| format!(" ({e})")).unwrap_or_default()
            ),
            (_, Some(e)) => format!("storage.to: {e}"),
            (s, None) => format!("storage.to trả lỗi HTTP {s}"),
        }
    }

    async fn post<T: serde::de::DeserializeOwned>(
        u: &Upload,
        path: &str,
        owner: Option<&str>,
        body: serde_json::Value,
    ) -> Result<(reqwest::StatusCode, T), String> {
        let mut req = u
            .client
            .post(format!("{}/api{path}", base("https://storage.to")))
            .header("X-Visitor-Token", &u.visitor)
            .timeout(Duration::from_secs(60))
            .json(&body);
        if let Some(o) = owner {
            req = req.header("X-Owner-Token", o);
        }
        let r = req.send().await.map_err(net_err)?;
        let status = r.status();
        let text = r.text().await.map_err(net_err)?;
        let v = serde_json::from_str(&text).map_err(|_| api_err(status, None))?;
        Ok((status, v))
    }

    pub async fn upload(u: &Upload) -> Result<Uploaded, String> {
        let (status, init): (_, Init) = post(
            u,
            "/upload/init",
            None,
            serde_json::json!({ "filename": UPLOAD_NAME, "content_type": CONTENT_TYPE, "size": u.len }),
        )
        .await?;
        if !init.success {
            return Err(api_err(status, init.error));
        }
        let r2_key = init.r2_key.clone().ok_or_else(|| api_err(status, None))?;
        u.tracker.set_status(crate::models::TransferStatus::Transferring, None);

        if init.kind == "multipart" {
            let upload_id = init.upload_id.clone().ok_or_else(|| api_err(status, None))?;
            let owner = init.owner_token.clone();
            let res = multipart(u, &init, &upload_id, owner.as_deref()).await;
            if let Err(e) = res {
                // Dọn phần đã tải lên dở.
                let _ = post::<serde_json::Value>(
                    u,
                    "/upload/abort",
                    owner.as_deref(),
                    serde_json::json!({ "upload_id": upload_id }),
                )
                .await;
                return Err(e);
            }
        } else {
            let url = init.upload_url.clone().ok_or_else(|| api_err(status, None))?;
            let data = read_block(&u.source, u.len as usize).await?;
            if data.len() as u64 != u.len {
                return Err("Dữ liệu đọc ra không đủ".into());
            }
            let fixed = url.clone();
            put_with_retry(u, url, data, || {
                let fixed = fixed.clone();
                async move { Ok(fixed) }
            })
            .await?;
        }

        let (status, c): (_, Confirm) = post(
            u,
            "/upload/confirm",
            init.owner_token.as_deref(),
            serde_json::json!({
                "filename": UPLOAD_NAME,
                "content_type": CONTENT_TYPE,
                "size": u.len,
                "r2_key": r2_key,
                "expiry_days": Provider::StorageTo.info().days,
            }),
        )
        .await?;
        let file = match c.file {
            Some(f) if c.success => f,
            _ => return Err(api_err(status, c.error)),
        };
        let expires_at = file
            .expires_at
            .as_deref()
            .and_then(parse_rfc3339)
            .unwrap_or_else(|| crate::history::now_secs() + 7 * 86_400);
        Ok(Uploaded { id: file.id, expires_at })
    }

    async fn part_url(u: &Upload, upload_id: &str, owner: Option<&str>, part: u64) -> Result<String, String> {
        let (status, p): (_, Parts) = post(
            u,
            "/upload/parts",
            owner,
            serde_json::json!({ "upload_id": upload_id, "part_numbers": [part] }),
        )
        .await?;
        p.part_urls
            .into_iter()
            .find(|x| x.part_number == part)
            .map(|x| x.url)
            .or_else(|| p.urls.get(&part.to_string()).cloned())
            .ok_or_else(|| api_err(status, p.error))
    }

    async fn multipart(u: &Upload, init: &Init, upload_id: &str, owner: Option<&str>) -> Result<(), String> {
        let part_size = init.part_size.filter(|s| *s > 0 && *s <= MAX_PART).ok_or("storage.to: cỡ phần không hợp lệ")?;
        let total = init.total_parts.unwrap_or_else(|| u.len.div_ceil(part_size));
        if total.saturating_mul(part_size) < u.len {
            return Err("storage.to: số phần không khớp".into());
        }
        let mut parts = Vec::with_capacity(total as usize);
        for n in 1..=total {
            if u.tracker.is_cancelled() {
                return Err(cancelled_err());
            }
            let size = part_size.min(u.len - (n - 1) * part_size) as usize;
            let data = read_block(&u.source, size).await?;
            if data.len() != size {
                return Err("Dữ liệu đọc ra không đủ".into());
            }
            let url = match init.initial_urls.get(&n.to_string()) {
                Some(url) => url.clone(),
                None => part_url(u, upload_id, owner, n).await?,
            };
            let etag = put_with_retry(u, url, data, || part_url(u, upload_id, owner, n)).await?;
            parts.push(serde_json::json!({ "partNumber": n, "etag": etag }));
        }
        let (status, done): (_, serde_json::Value) = post(
            u,
            "/upload/complete-multipart",
            owner,
            serde_json::json!({ "upload_id": upload_id, "parts": parts }),
        )
        .await?;
        if done.get("success").and_then(|s| s.as_bool()) != Some(true) {
            let e = done.get("error").and_then(|e| e.as_str()).map(String::from);
            return Err(api_err(status, e));
        }
        Ok(())
    }
}

/// Phần "file" của một yêu cầu multipart: đọc + mã hoá từng 1 MB trên luồng riêng, đẩy
/// dần lên mạng (không giữ cả file trong RAM). Không gửi lại được khi rớt mạng.
fn streamed_part(u: &Upload) -> Result<reqwest::multipart::Part, String> {
    let source = u.source.clone();
    let tracker = u.tracker.clone();
    // Đọc + mã hoá từng 1 MB trên luồng riêng, đẩy dần lên mạng.
    let stream = futures_util::stream::unfold((source, tracker), |(source, tracker)| async move {
        if tracker.is_cancelled() {
            return Some((Err(std::io::Error::other("Đã huỷ")), (source, tracker)));
        }
        match read_block(&source, 1 << 20).await {
            Ok(b) if b.is_empty() => None,
            Ok(b) => {
                tracker.add(b.len() as u64);
                Some((Ok(Bytes::from(b)), (source, tracker)))
            }
            Err(e) => Some((Err(std::io::Error::other(e)), (source, tracker))),
        }
    });
    reqwest::multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), u.len)
        .file_name(UPLOAD_NAME)
        .mime_str(CONTENT_TYPE)
        .map_err(|e| e.to_string())
}

async fn send_form(u: &Upload, url: String, form: reqwest::multipart::Form) -> Result<reqwest::Response, String> {
    u.tracker.set_status(crate::models::TransferStatus::Transferring, None);
    u.client.post(url).multipart(form).send().await.map_err(|e| {
        if u.tracker.is_cancelled() {
            cancelled_err()
        } else {
            net_err(e)
        }
    })
}

/// Tải file đã mã hoá bằng GET/POST tới `url`; báo lỗi rõ khi hết hạn hoặc nhận về trang web.
async fn fetch_file(req: reqwest::RequestBuilder, name: &str) -> Result<(reqwest::Response, Option<u64>), String> {
    let r = req.send().await.map_err(net_err)?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("File không còn trên {name} (đã hết hạn hoặc bị xoá)"));
    }
    if !r.status().is_success() {
        return Err(format!("{name} trả lỗi HTTP {}", r.status().as_u16()));
    }
    let html = r
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if html {
        return Err(format!("{name} trả về trang web thay vì file"));
    }
    let len = r.content_length();
    Ok((r, len))
}

/// Đầu thông báo lỗi khi Litterbox từ chối (tường lửa của họ chặn một số mạng); app tự chuyển
/// sang storage.to khi gặp lỗi này.
pub const LB_REFUSED: &str = "Litterbox từ chối tải lên";

mod litterbox {
    //! https://litterbox.catbox.moe/tools.php — một yêu cầu multipart (`reqtype=fileupload`,
    //! `time=72h`, `fileToUpload`), trả về link tải trực tiếp dạng văn bản. Link tải có CORS `*`
    //! nên trang nhận trên website tự tải được.
    use super::*;

    pub async fn upload(u: &Upload) -> Result<Uploaded, String> {
        let form = reqwest::multipart::Form::new()
            .text("reqtype", "fileupload")
            .text("time", "72h")
            .part("fileToUpload", streamed_part(u)?);
        let url = format!("{}/resources/internals/api.php", base("https://litterbox.catbox.moe"));
        let r = send_form(u, url, form).await?;
        let status = r.status();
        let text = r.text().await.map_err(net_err)?;
        if !status.is_success() {
            return Err(if status.as_u16() == 413 {
                "Litterbox: file quá lớn (tối đa 1 GB)".into()
            } else {
                format!("{LB_REFUSED} (HTTP {})", status.as_u16())
            });
        }
        let id = text
            .trim()
            .strip_prefix("https://litter.catbox.moe/")
            .filter(|id| Provider::Litterbox.valid_id(id))
            .ok_or_else(|| format!("Litterbox trả về kết quả lạ: {}", text.chars().take(80).collect::<String>()))?;
        Ok(Uploaded { id: id.to_string(), expires_at: crate::history::now_secs() + 3 * 86_400 })
    }

    pub async fn download(client: &reqwest::Client, id: &str) -> Result<(reqwest::Response, Option<u64>), String> {
        fetch_file(client.get(format!("{}/{id}", base("https://litter.catbox.moe"))), "Litterbox").await
    }
}

mod temp_sh {
    //! https://temp.sh — một yêu cầu multipart `file=@…`, trả về link dạng văn bản.
    use super::*;

    pub async fn upload(u: &Upload) -> Result<Uploaded, String> {
        let form = reqwest::multipart::Form::new().part("file", streamed_part(u)?);
        let r = send_form(u, format!("{}/upload", base("https://temp.sh")), form).await?;
        let status = r.status();
        let text = r.text().await.map_err(net_err)?;
        if !status.is_success() {
            return Err(if status.as_u16() == 413 {
                "temp.sh: file quá lớn".into()
            } else {
                format!("temp.sh trả lỗi HTTP {}", status.as_u16())
            });
        }
        let id = text
            .trim()
            .strip_prefix("https://temp.sh/")
            .filter(|id| Provider::TempSh.valid_id(id))
            .ok_or_else(|| format!("temp.sh trả về kết quả lạ: {}", text.chars().take(80).collect::<String>()))?;
        Ok(Uploaded { id: id.to_string(), expires_at: crate::history::now_secs() + 3 * 86_400 })
    }

    pub async fn download(client: &reqwest::Client, id: &str) -> Result<(reqwest::Response, Option<u64>), String> {
        // Trang tải của temp.sh tải file bằng POST tới chính link đó.
        fetch_file(client.post(format!("{}/{id}", base("https://temp.sh"))), "temp.sh").await
    }
}

/// Luồng byte của phản hồi HTTP thành `Read` đồng bộ (dùng trong spawn_blocking).
pub fn response_reader(r: reqwest::Response) -> impl Read + Send {
    let stream = r.bytes_stream().map(|b| b.map_err(std::io::Error::other));
    tokio_util::io::SyncIoBridge::new(tokio_util::io::StreamReader::new(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-04-15T12:00:00Z"), Some(1_776_254_400));
        assert_eq!(parse_rfc3339("2026-04-15T12:00:00.123456Z"), Some(1_776_254_400));
        assert_eq!(parse_rfc3339("2026-04-15T19:00:00+07:00"), Some(1_776_254_400));
        assert_eq!(parse_rfc3339("rác"), None);
    }

    #[test]
    fn ids_are_validated() {
        assert!(Provider::StorageTo.valid_id("FQxyz1234"));
        assert!(!Provider::StorageTo.valid_id("../x"));
        assert!(!Provider::StorageTo.valid_id("a?b=c"));
        assert!(Provider::Litterbox.valid_id("x7k2p9.bdrop"));
        assert!(!Provider::Litterbox.valid_id("x7k2p9"));
        assert!(!Provider::Litterbox.valid_id("../x.bdrop"));
        assert!(!Provider::Litterbox.valid_id("abcd.bd/rop"));
        assert!(Provider::TempSh.valid_id("AbCdE/betterdrop.bdrop"));
        assert!(!Provider::TempSh.valid_id("AbCdE/../x"));
        assert!(!Provider::TempSh.valid_id("AbCdE"));
        assert!(!Provider::TempSh.valid_id("a/b/c"));
    }
}
