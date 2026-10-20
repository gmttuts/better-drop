use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::BetterdropMobile;
#[cfg(mobile)]
use mobile::BetterdropMobile;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the betterdrop-mobile APIs.
pub trait BetterdropMobileExt<R: Runtime> {
  fn betterdrop_mobile(&self) -> &BetterdropMobile<R>;
}

impl<R: Runtime, T: Manager<R>> crate::BetterdropMobileExt<R> for T {
  fn betterdrop_mobile(&self) -> &BetterdropMobile<R> {
    self.state::<BetterdropMobile<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("betterdrop-mobile")
    .setup(|app, api| {
      #[cfg(mobile)]
      let betterdrop_mobile = mobile::init(app, api)?;
      #[cfg(desktop)]
      let betterdrop_mobile = desktop::init(app, api)?;
      app.manage(betterdrop_mobile);
      Ok(())
    })
    .build()
}
