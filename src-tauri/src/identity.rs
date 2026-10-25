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

type CertAndKey = (
    Vec<rustls::pki_types::CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
);

impl Identity {
    /// Chứng chỉ và khoá dạng DER cho rustls.
    pub fn der(&self) -> Result<CertAndKey, String> {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        let certs = CertificateDer::pem_slice_iter(self.cert_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Chứng chỉ hỏng: {e:?}"))?;
        let key = PrivateKeyDer::from_pem_slice(self.key_pem.as_bytes()).map_err(|e| format!("Khoá hỏng: {e:?}"))?;
        Ok((certs, key))
    }
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
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "linux"
    }
}

/// Giá trị mặc định khi tạo cài đặt lần đầu. Trên điện thoại, tên máy và thư mục
/// lưu do hệ điều hành cung cấp (qua plugin), nên được truyền vào từ ngoài.
#[derive(Default)]
pub struct Defaults {
    pub name: Option<String>,
    pub download_dir: Option<PathBuf>,
    /// Luôn dùng `download_dir` ở trên thay cho giá trị đã lưu. Cần cho iOS, vì
    /// đường dẫn thư mục của app đổi sau mỗi lần cập nhật app.
    pub force_download_dir: bool,
}

pub fn default_name() -> String {
    let host = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "Máy tính".into());
    let host = host.trim_end_matches(".local").to_string();
    match profile() {
        Some(p) => format!("{host} ({p})"),
        None => host,
    }
}

pub fn default_download_dir() -> PathBuf {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    match profile() {
        Some(p) => base.join(format!("BetterDrop-{p}")),
        None => base.join("BetterDrop"),
    }
}

pub fn load_settings(dir: &Path, defaults: Defaults) -> Settings {
    let path = dir.join("settings.json");
    if let Some(mut s) = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
    {
        if let (true, Some(d)) = (defaults.force_download_dir, &defaults.download_dir) {
            let d = d.to_string_lossy().into_owned();
            if s.download_dir != d {
                s.download_dir = d;
                let _ = save_settings(dir, &s);
            }
        }
        return s;
    }
    let s = Settings {
        device_id: uuid::Uuid::new_v4().to_string(),
        name: defaults.name.unwrap_or_else(default_name),
        download_dir: defaults
            .download_dir
            .unwrap_or_else(default_download_dir)
            .to_string_lossy()
            .into_owned(),
        receive_enabled: true,
        hide_dock: false,
        language: "auto".into(),
        background_receive: true,
        hotkeys: Default::default(),
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
