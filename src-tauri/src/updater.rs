//! Tự cập nhật (chỉ trên máy tính): đọc `latest.json` của bản phát hành mới nhất trên GitHub,
//! kiểm tra chữ ký bằng khoá công khai trong `tauri.conf.json`, tải về, cài rồi mở lại app.
//! Điện thoại không tự cài được (APK ngoài Play, IPA cài bằng Sideloadly) nên chỉ hỏi GitHub API
//! xem bản phát hành mới nhất, báo có bản mới và mở link tải.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    /// Chỉ trên điện thoại: link file APK (Android) hoặc trang phát hành (iOS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// So phiên bản dạng "1.2.3" (bỏ chữ "v" đầu và phần "-beta"…): `latest` có mới hơn `current` không.
#[cfg_attr(desktop, allow(dead_code))]
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let v = v.split(['-', '+']).next().unwrap_or("");
        v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(desktop)]
pub async fn check(app: &tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Không kiểm tra được bản mới: {e}"))?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone(),
        url: None,
    }))
}

#[cfg(desktop)]
pub async fn install(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Không kiểm tra được bản mới: {e}"))?
        .ok_or("Đang dùng bản mới nhất")?;
    let mut downloaded: u64 = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.emit(
                    "update-progress",
                    serde_json::json!({ "downloaded": downloaded, "total": total }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("Không cài được bản mới: {e}"))?;
    app.restart()
}

#[cfg(mobile)]
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/gmttuts/better-drop/releases/latest";

#[cfg(mobile)]
pub async fn check(_app: &tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        body: Option<String>,
        #[serde(default)]
        assets: Vec<Asset>,
    }

    let err = |e: reqwest::Error| {
        let why = if e.is_connect() || e.is_timeout() {
            "không kết nối được Internet".to_string()
        } else {
            crate::client::describe_error(&e)
        };
        format!("Không kiểm tra được bản mới: {why}")
    };
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let client = reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .user_agent(format!("BetterDrop/{}", crate::models::APP_VERSION))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let release: Release = client
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?
        .json()
        .await
        .map_err(err)?;

    let version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    if !is_newer(&version, crate::models::APP_VERSION) {
        return Ok(None);
    }
    let apk = release
        .assets
        .iter()
        .find(|a| cfg!(target_os = "android") && a.name == "BetterDrop-android.apk")
        .map(|a| a.browser_download_url.clone());
    Ok(Some(UpdateInfo {
        version,
        current: crate::models::APP_VERSION.to_string(),
        notes: release.body,
        url: Some(apk.unwrap_or(release.html_url)),
    }))
}

/// Mở link tải bản mới (Android tải APK rồi người dùng tự cài; iOS mở trang phát hành).
#[cfg(mobile)]
pub async fn install(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = check(app)
        .await?
        .and_then(|u| u.url)
        .ok_or("Đang dùng bản mới nhất")?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
