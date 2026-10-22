use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<BetterdropMobile<R>> {
  Ok(BetterdropMobile(std::marker::PhantomData))
}

/// Trên máy tính các API này không cần thiết.
pub struct BetterdropMobile<R: Runtime>(std::marker::PhantomData<fn() -> R>);

impl<R: Runtime> BetterdropMobile<R> {
  pub fn acquire_multicast_lock(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn take_shared(&self) -> crate::Result<(Vec<String>, Option<String>)> {
    Ok((vec![], None))
  }

  pub fn request_local_network(&self) -> crate::Result<bool> {
    Ok(true)
  }

  pub fn file_info(&self, uri: &str) -> crate::Result<FileInfo> {
    let meta = std::fs::metadata(uri)?;
    Ok(FileInfo {
      name: std::path::Path::new(uri)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into()),
      size: meta.len() as i64,
    })
  }

  pub fn download_dir(&self) -> crate::Result<String> {
    Err(std::io::Error::other("không dùng trên máy tính").into())
  }

  pub fn device_name(&self) -> crate::Result<String> {
    Err(std::io::Error::other("không dùng trên máy tính").into())
  }

  pub fn start_bonjour(&self, _id: &str, _name: &str, _os: &str, _fp: &str, _port: u16, _version: &str) -> crate::Result<()> {
    Ok(())
  }

  pub fn bonjour_peers(&self) -> crate::Result<Vec<BonjourPeer>> {
    Ok(vec![])
  }

  pub fn scan_file(&self, _path: &str) -> crate::Result<()> {
    Ok(())
  }
}
