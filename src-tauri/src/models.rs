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
    /// Phiên bản app (vd "0.3.2"). Thêm từ 0.3.2: bản cũ hơn không gửi trường này
    /// (để trống), và bỏ qua nó khi nhận. Xem "Tương thích giữa các phiên bản" trong README.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

/// Phiên bản của app này.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    /// false khi máy đã lưu nhưng hiện không kết nối được.
    #[serde(default = "default_true")]
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferKind {
    Files,
    Text,
    /// Chia sẻ một nhóm máy (từ 0.3.5). Bản cũ không hiểu loại này và trả lỗi 422.
    Group,
}

/// Một máy trong nhóm được chia sẻ: đủ thông tin để máy nhận kết nối được ngay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMember {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub os: String,
    /// Dấu vân tay chứng chỉ (trống nếu máy gửi chưa từng thấy máy này).
    #[serde(default)]
    pub fp: String,
    #[serde(default)]
    pub addrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedGroup {
    pub name: String,
    pub members: Vec<SharedMember>,
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
    /// Với kind = "group".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<SharedGroup>,
    /// Ảnh thu nhỏ của vài file ảnh/video đầu (từ 0.3.7, xem `thumbs.rs`). Bản cũ không
    /// gửi trường này và bỏ qua nó khi nhận.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbs: Vec<Thumb>,
}

/// Ảnh thu nhỏ của một file trong yêu cầu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumb {
    /// `FileMeta::id` của file ảnh/video.
    pub file: String,
    /// JPEG, mã hoá base64.
    pub jpeg: String,
}

/// Một ô xem trước trong hộp thoại nhận.
#[derive(Debug, Clone, Serialize)]
pub struct MediaPreview {
    pub name: String,
    pub video: bool,
    /// `data:image/jpeg;base64,…`; None: chỉ hiện biểu tượng.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
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
    /// Gửi tiếp từ byte này (từ 0.3.5). Bản cũ không gửi trường này (gửi từ đầu)
    /// và cũng bỏ qua nó khi nhận.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

/// Trả lời `/api/v1/upload-status` (từ 0.3.5): máy nhận đã có bao nhiêu byte của file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStatus {
    pub received: u64,
    pub done: bool,
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
    /// Máy gửi đã chứng minh danh tính nên có thể đánh dấu "Luôn chấp nhận".
    pub can_trust: bool,
    /// Có giá trị khi đây là yêu cầu chia sẻ nhóm (file_count = số máy, preview = tên máy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Vài ảnh/video đầu để xem trước.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaPreview>,
    /// Số ảnh/video còn lại không hiện trong `media`.
    pub media_more: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomingText {
    pub sender_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    /// "auto" (theo ngôn ngữ của máy) | "vi" | "en". Cài mới dùng "auto"; máy đã cài
    /// từ trước (file cài đặt chưa có trường này) giữ tiếng Việt như trước khi có tiếng Anh.
    #[serde(default = "default_language")]
    pub language: String,
    /// Android: vẫn thấy trong mạng và nhận file khi app ở nền hoặc màn hình tắt
    /// (dịch vụ chạy nền kèm thông báo thường trực). Máy tính luôn chạy nền nên bỏ qua.
    #[serde(default = "default_true")]
    pub background_receive: bool,
    /// Phím tắt toàn cục (chỉ máy tính). Thêm sau: file cài đặt cũ không có thì dùng mặc định (bật).
    #[serde(default)]
    pub hotkeys: Hotkeys,
}

/// Phím tắt toàn cục, dạng chữ như "CmdOrCtrl+Shift+D" (CmdOrCtrl = ⌘ trên Mac, Ctrl trên Windows/Linux).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Hotkeys {
    pub enabled: bool,
    /// Gửi clipboard tới máy gần nhất.
    pub send_clipboard: String,
    /// Mở BetterDrop.
    pub open_app: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            enabled: true,
            send_clipboard: "CmdOrCtrl+Shift+D".into(),
            open_app: "CmdOrCtrl+Shift+B".into(),
        }
    }
}

fn default_language() -> String {
    "vi".into()
}

impl Settings {
    /// Giao diện đang dùng tiếng Anh? ("auto": theo ngôn ngữ hệ điều hành.)
    pub fn english(&self) -> bool {
        match self.language.as_str() {
            "en" => true,
            "vi" => false,
            _ => !sys_locale::get_locale().unwrap_or_default().to_lowercase().starts_with("vi"),
        }
    }
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
