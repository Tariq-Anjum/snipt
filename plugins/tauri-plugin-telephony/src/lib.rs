//! `tauri-plugin-telephony`: the Android default-SMS shell for Snipt.
//!
//! The Kotlin side (`android/`) owns the manifest entries (SMS_DELIVER,
//! WAP_PUSH_DELIVER, RESPOND_VIA_MESSAGE, SENDTO), the radio, the Telephony
//! provider, notifications, the secure clipboard and window flags. Background
//! receipt does not go through this plugin: the receiver calls the Rust core
//! over JNI (`src-tauri/src/jni_bridge.rs`), so it works with no WebView.
//! On desktop every method is a no-op or `Error::Unsupported`.

use tauri::{
    Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

mod commands;
mod error;
pub mod models;

#[cfg(not(target_os = "android"))]
mod desktop;
#[cfg(target_os = "android")]
mod mobile;

#[cfg(not(target_os = "android"))]
pub use desktop::Telephony;
pub use error::{Error, Result};
#[cfg(target_os = "android")]
pub use mobile::Telephony;

/// Access the plugin from any `Manager` (app handle, window, ...).
pub trait TelephonyExt<R: Runtime> {
    fn telephony(&self) -> &Telephony<R>;
}

impl<R: Runtime, T: Manager<R>> TelephonyExt<R> for T {
    fn telephony(&self) -> &Telephony<R> {
        self.state::<Telephony<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("telephony")
        .invoke_handler(tauri::generate_handler![
            commands::request_default_sms_role,
            commands::status,
            commands::copy_sensitive,
            commands::set_secure,
            commands::display_boost,
            commands::import_inbox,
            commands::take_compose,
        ])
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let t = mobile::init(app, api)?;
            #[cfg(not(target_os = "android"))]
            let t = desktop::init(app, api)?;
            app.manage(t);
            Ok(())
        })
        .build()
}
