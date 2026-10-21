use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_betterdrop_mobile);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<BetterdropMobile<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.plugin.betterdrop_mobile", "BetterDropPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_betterdrop_mobile)?;
  Ok(BetterdropMobile(handle))
}

/// Các API riêng của điện thoại mà BetterDrop cần.
pub struct BetterdropMobile<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> BetterdropMobile<R> {
  /// Android: giữ khoá multicast để nhận được gói tin mDNS.
  pub fn acquire_multicast_lock(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("acquireMulticastLock", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  /// Android 17+: xin quyền truy cập mạng cục bộ. Chờ người dùng trả lời.
  pub fn request_local_network(&self) -> crate::Result<bool> {
    self
      .0
      .run_mobile_plugin::<GrantedResponse>("requestLocalNetwork", ())
      .map(|r| r.granted)
      .map_err(Into::into)
  }

  /// Tên và kích thước của file do bộ chọn file trả về (content:// hoặc file://).
  pub fn file_info(&self, uri: &str) -> crate::Result<FileInfo> {
    self
      .0
      .run_mobile_plugin("fileInfo", UriArgs { uri })
      .map_err(Into::into)
  }

  /// Thư mục lưu file nhận được: Download (Android) / Documents (iOS).
  pub fn download_dir(&self) -> crate::Result<String> {
    self
      .0
      .run_mobile_plugin::<PathResponse>("downloadDir", ())
      .map(|r| r.path)
      .map_err(Into::into)
  }

  /// Tên thiết bị để hiển thị cho máy khác.
  pub fn device_name(&self) -> crate::Result<String> {
    self
      .0
      .run_mobile_plugin::<PathResponse>("deviceName", ())
      .map(|r| r.path)
      .map_err(Into::into)
  }

  /// iOS: quảng bá máy này qua Bonjour và bắt đầu tìm máy khác.
  pub fn start_bonjour(&self, id: &str, name: &str, os: &str, fp: &str, port: u16, version: &str) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("startBonjour", StartBonjourArgs { id, name, os, fp, port, version })
      .map(|_| ())
      .map_err(Into::into)
  }

  /// iOS: các máy đã tìm thấy qua Bonjour.
  pub fn bonjour_peers(&self) -> crate::Result<Vec<BonjourPeer>> {
    self
      .0
      .run_mobile_plugin::<BonjourPeers>("bonjourPeers", ())
      .map(|r| r.peers)
      .map_err(Into::into)
  }

  /// Android: báo hệ thống có file mới để hiện ngay trong Files/Ảnh.
  pub fn scan_file(&self, path: &str) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("scanFile", PathArgs { path })
      .map(|_| ())
      .map_err(Into::into)
  }
}
