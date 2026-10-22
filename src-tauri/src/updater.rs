//! Tự cập nhật (chỉ trên máy tính): đọc `latest.json` của bản phát hành mới nhất trên GitHub,
//! kiểm tra chữ ký bằng khoá công khai trong `tauri.conf.json`, tải về, cài rồi mở lại app.
//! Điện thoại cập nhật bằng cách cài file APK/IPA mới nên ở đây luôn báo "không có bản mới".

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
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
pub async fn check(_app: &tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    Ok(None)
}

#[cfg(mobile)]
pub async fn install(_app: &tauri::AppHandle) -> Result<(), String> {
    Err("Trên điện thoại hãy tải file cài mới từ trang phát hành".into())
}
