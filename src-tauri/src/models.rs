use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Thông tin một máy, được quảng bá qua mDNS và trả về ở `/api/v1/info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// "mac" | "windows" | "linux" | "android" | "ios"
    pub os: String,
    /// SHA-256 (hex) của chứng chỉ TLS.
    pub fp: String,
    pub port: u16,
}

/// Một máy khác trong mạng mà ta có thể gửi tới.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    #[serde(flatten)]
    pub info: DeviceInfo,
    /// Các địa chỉ "ip:port" thử lần lượt khi kết nối.
    pub addrs: Vec<String>,
    /// true nếu được thêm thủ công bằng IP.
    #[serde(default)]
    pub manual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferKind {
    Files,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub id: String,
    /// Đường dẫn tương đối, phân tách bằng "/".
    pub rel_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareRequest {
    pub sender: DeviceInfo,
    pub kind: TransferKind,
    #[serde(default)]
    pub files: Vec<FileMeta>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrepareResponse {
    pub session_id: String,
    /// file_id -> token
    pub tokens: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadQuery {
    pub session: String,
    pub file: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionQuery {
    pub session: String,
}

/// Yêu cầu nhận file đang chờ người dùng bấm Chấp nhận / Từ chối.
#[derive(Debug, Clone, Serialize)]
pub struct IncomingRequest {
    pub request_id: String,
    pub sender_name: String,
    pub sender_os: String,
    pub file_count: usize,
    pub total_bytes: u64,
    /// Vài tên file đầu để hiển thị.
    pub preview: Vec<String>,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomingText {
    pub sender_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Waiting,
    Transferring,
    Done,
    Rejected,
    Cancelled,
    Error,
}

impl TransferStatus {
    pub fn is_final(self) -> bool {
        !matches!(self, TransferStatus::Waiting | TransferStatus::Transferring)
    }
}

/// Trạng thái một lượt chuyển, gửi lên UI qua event `transfer-update`.
#[derive(Debug, Clone, Serialize)]
pub struct TransferUpdate {
    pub id: String,
    pub direction: Direction,
    pub peer_name: String,
    pub kind: TransferKind,
    pub file_count: usize,
    pub total_bytes: u64,
    pub transferred: u64,
    pub speed_bps: f64,
    pub status: TransferStatus,
    pub error: Option<String>,
    /// Thư mục chứa file đã nhận (chỉ với chiều nhận).
    pub dir_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub device_id: String,
    pub name: String,
    pub download_dir: String,
    /// Tắt đi thì mọi yêu cầu gửi tới đều bị từ chối ngay.
    #[serde(default = "default_true")]
    pub receive_enabled: bool,
    /// Chỉ trên macOS: ẩn biểu tượng trên Dock, chỉ còn biểu tượng trên thanh menu.
    #[serde(default)]
    pub hide_dock: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfInfo {
    pub info: DeviceInfo,
    pub settings: Settings,
    /// Các địa chỉ "ip:port" của máy này, để nhập thủ công ở máy kia.
    pub addrs: Vec<String>,
    /// Đang bật "Mở cùng máy tính".
    pub autostart: bool,
    /// Đang chạy trên điện thoại/tablet.
    pub mobile: bool,
}

/// Một mục đã chọn để gửi, hiển thị trong danh sách chờ gửi.
#[derive(Debug, Clone, Serialize)]
pub struct StagedItem {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    /// Kích thước file; None với thư mục hoặc khi không biết.
    pub size: Option<u64>,
}

/// Mã QR để điện thoại quét và kết nối với máy này.
#[derive(Debug, Clone, Serialize)]
pub struct PairInfo {
    pub link: String,
    pub svg: String,
}
