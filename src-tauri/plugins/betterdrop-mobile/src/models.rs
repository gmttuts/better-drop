use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileInfo {
  pub name: String,
  /// -1 nếu không biết.
  pub size: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct SharedResponse {
  #[serde(default)]
  pub uris: Vec<String>,
  #[serde(default)]
  pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GrantedResponse {
  pub granted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PathResponse {
  pub path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UriArgs<'a> {
  pub uri: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct PathArgs<'a> {
  pub path: &'a str,
}

/// Một máy tìm thấy qua Bonjour trên iOS.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BonjourPeer {
  pub id: String,
  pub name: String,
  pub os: String,
  pub fp: String,
  pub port: u16,
  pub ips: Vec<String>,
  #[serde(default)]
  pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BonjourPeers {
  pub peers: Vec<BonjourPeer>,
}

#[derive(Debug, Serialize)]
pub(crate) struct StartBonjourArgs<'a> {
  pub id: &'a str,
  pub name: &'a str,
  pub os: &'a str,
  pub fp: &'a str,
  pub port: u16,
  pub version: &'a str,
}

#[cfg(mobile)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackgroundArgs<'a> {
  pub title: &'a str,
  pub text: &'a str,
  pub stop_label: &'a str,
  pub channel_name: &'a str,
  pub on_stop: tauri::ipc::Channel<serde_json::Value>,
}

#[cfg(mobile)]
#[derive(Serialize)]
pub(crate) struct ChannelArgs {
  pub channel: tauri::ipc::Channel<serde_json::Value>,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
pub(crate) struct OnArgs {
  pub on: bool,
}

/// Chữ trên thông báo thường trực khi nhận file ở nền (Android).
pub struct BackgroundNotice<'a> {
  pub title: &'a str,
  pub text: &'a str,
  pub stop_label: &'a str,
  /// Tên kênh thông báo trong Cài đặt của Android.
  pub channel_name: &'a str,
}
