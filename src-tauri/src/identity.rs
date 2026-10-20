//! Danh tính của máy: thư mục dữ liệu, cài đặt và chứng chỉ TLS tự ký.

use crate::models::Settings;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
    pub fingerprint: String,
}

/// Cho phép chạy nhiều bản trên một máy (để thử nghiệm) bằng `BETTERDROP_PROFILE=b`.
pub fn profile() -> Option<String> {
    std::env::var("BETTERDROP_PROFILE")
        .ok()
        .filter(|p| !p.trim().is_empty())
}

pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let name = match profile() {
        Some(p) => format!("BetterDrop-{p}"),
        None => "BetterDrop".to_string(),
    };
    base.join(name)
}

pub fn fingerprint(der: &[u8]) -> String {
    hex::encode(Sha256::digest(der))
}

pub fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn default_name() -> String {
    let host = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "Máy tính".into());
    let host = host.trim_end_matches(".local").to_string();
    match profile() {
        Some(p) => format!("{host} ({p})"),
        None => host,
    }
}

fn default_download_dir() -> PathBuf {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    match profile() {
        Some(p) => base.join(format!("BetterDrop-{p}")),
        None => base.join("BetterDrop"),
    }
}

pub fn load_settings(dir: &Path) -> Settings {
    let path = dir.join("settings.json");
    if let Some(s) = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
    {
        return s;
    }
    let s = Settings {
        device_id: uuid::Uuid::new_v4().to_string(),
        name: default_name(),
        download_dir: default_download_dir().to_string_lossy().into_owned(),
    };
    let _ = save_settings(dir, &s);
    s
}

pub fn save_settings(dir: &Path, s: &Settings) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("settings.json"),
        serde_json::to_vec_pretty(s).expect("Settings luôn serialize được"),
    )
}

/// Đọc chứng chỉ đã lưu, hoặc tạo mới ở lần chạy đầu.
pub fn load_or_create_identity(dir: &Path) -> Result<Identity, String> {
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let der_path = dir.join("cert.der");

    if let (Ok(cert_pem), Ok(key_pem), Ok(cert_der)) = (
        fs::read_to_string(&cert_path),
        fs::read_to_string(&key_path),
        fs::read(&der_path),
    ) {
        return Ok(Identity {
            fingerprint: fingerprint(&cert_der),
            cert_pem,
            key_pem,
        });
    }

    let ck = rcgen::generate_simple_self_signed(vec!["betterdrop.local".to_string()])
        .map_err(|e| format!("Không tạo được chứng chỉ: {e}"))?;
    let cert_pem = ck.cert.pem();
    let key_pem = ck.key_pair.serialize_pem();
    let cert_der = ck.cert.der().to_vec();

    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(&cert_path, &cert_pem).map_err(|e| e.to_string())?;
    fs::write(&key_path, &key_pem).map_err(|e| e.to_string())?;
    fs::write(&der_path, &cert_der).map_err(|e| e.to_string())?;

    Ok(Identity {
        fingerprint: fingerprint(&cert_der),
        cert_pem,
        key_pem,
    })
}
