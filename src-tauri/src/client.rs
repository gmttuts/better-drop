//! HTTP client tới máy khác, xác thực chứng chỉ bằng fingerprint (pinning).

use crate::identity::fingerprint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Cách kiểm tra chứng chỉ của máy bên kia.
#[derive(Debug, Clone)]
pub enum Pin {
    /// Chỉ chấp nhận chứng chỉ có đúng fingerprint này (lấy từ mDNS).
    Fingerprint(String),
    /// Tin ở lần đầu (nhập IP thủ công): chấp nhận và ghi lại fingerprint.
    Capture(Arc<Mutex<Option<String>>>),
}

#[derive(Debug)]
struct PinVerifier {
    pin: Pin,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity.as_ref());
        match &self.pin {
            Pin::Fingerprint(expected) if expected.eq_ignore_ascii_case(&fp) => {
                Ok(ServerCertVerified::assertion())
            }
            Pin::Fingerprint(_) => Err(rustls::Error::General(
                "Chứng chỉ của máy nhận không khớp (fingerprint sai)".into(),
            )),
            Pin::Capture(slot) => {
                *slot.lock().unwrap() = Some(fp);
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Tạo HTTP client tới một máy khác.
///
/// `me`: chứng chỉ của chính máy này. Có thì client trình nó khi kết nối (TLS hai
/// chiều) để máy nhận biết chắc đây là máy nào — cần cho "máy tin cậy". Máy chạy bản
/// cũ không yêu cầu chứng chỉ client nên vẫn kết nối bình thường.
pub fn build_client(pin: Pin, me: Option<&crate::identity::Identity>) -> Result<reqwest::Client, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinVerifier { pin, provider }));
    let tls = match me {
        Some(id) => {
            let (certs, key) = id.der()?;
            builder.with_client_auth_cert(certs, key).map_err(|e| e.to_string())?
        }
        None => builder.with_no_client_auth(),
    };
    reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .connect_timeout(Duration::from_secs(4))
        .tcp_nodelay(true)
        .build()
        .map_err(|e| e.to_string())
}

pub fn api_url(addr: &str, path: &str) -> String {
    format!("https://{addr}/api/v1/{path}")
}

/// Mô tả lỗi reqwest bằng tiếng Việt dễ hiểu.
pub fn describe_error(e: &reqwest::Error) -> String {
    use std::error::Error as _;
    let mut detail = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        detail = s.to_string();
        src = s.source();
    }
    if e.is_connect() {
        format!("Không kết nối được tới máy nhận ({detail})")
    } else if e.is_timeout() {
        "Hết thời gian chờ phản hồi".to_string()
    } else {
        detail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn identity() -> Identity {
        let ck = rcgen::generate_simple_self_signed(vec!["t".into()]).unwrap();
        let der = ck.cert.der().to_vec();
        Identity {
            fingerprint: fingerprint(&der),
            cert_pem: ck.cert.pem(),
            key_pem: ck.key_pair.serialize_pem(),
        }
    }

    async fn serve(id: &Identity) -> String {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let cfg = axum_server::tls_rustls::RustlsConfig::from_pem(
            id.cert_pem.clone().into_bytes(),
            id.key_pem.clone().into_bytes(),
        )
        .await
        .unwrap();
        let app = axum::Router::new().route("/api/v1/info", axum::routing::get(|| async { "ok" }));
        tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, cfg)
                .serve(app.into_make_service())
                .await
                .unwrap();
        });
        addr
    }

    #[tokio::test]
    async fn pinning_accepts_right_and_rejects_wrong_fingerprint() {
        let id = identity();
        let addr = serve(&id).await;

        let good = build_client(Pin::Fingerprint(id.fingerprint.clone()), None).unwrap();
        let body = good.get(api_url(&addr, "info")).send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "ok");

        let bad = build_client(Pin::Fingerprint("00".repeat(32)), None).unwrap();
        assert!(bad.get(api_url(&addr, "info")).send().await.is_err());

        let slot = Arc::new(Mutex::new(None));
        let tofu = build_client(Pin::Capture(slot.clone()), None).unwrap();
        tofu.get(api_url(&addr, "info")).send().await.unwrap();
        assert_eq!(slot.lock().unwrap().as_deref(), Some(id.fingerprint.as_str()));
    }
}
