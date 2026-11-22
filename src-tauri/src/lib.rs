//! Snipt: Tauri shell. All logic lives in the `sms-*` crates; this crate only
//! owns the database handle, input validation and the command surface.

pub mod commands;
pub mod core;
pub mod error;
#[cfg(target_os = "android")]
pub mod jni_bridge;
pub mod picker;

use tauri::Manager;

use crate::core::AppState;

/// Database file inside the app data dir. On Android that dir is
/// `Context.getDataDir()`; the Kotlin receivers open the same path.
pub const DB_FILE: &str = "snipt.db";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_telephony::init());
    // Linux file dialogs: used from Rust (`picker.rs`) only. No capability
    // grants the webview any `dialog:*` permission.
    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    let builder = builder.setup(|app| {
        let dir = app.path().app_data_dir()?;
        // Linux: the plaintext DB lives here, so the dir is 0700 (created or
        // tightened) and `Store::open` makes the DB/WAL/SHM 0600 before
        // SQLite touches them. Android's app data dir is already private.
        #[cfg(not(target_os = "android"))]
        sms_store::fsperm::create_private_dir(&dir)?;
        #[cfg(target_os = "android")]
        std::fs::create_dir_all(&dir)?;
        #[cfg(target_os = "android")]
        let state = {
            use tauri::Emitter;
            // Same core as the JNI receiver path (one connection, one vault).
            let s = AppState::shared(dir.join(DB_FILE))?;
            jni_bridge::set_state(s.clone());
            let handle = app.handle().clone();
            jni_bridge::set_emitter(Box::new(move |event, payload| {
                let _ = handle.emit(event, payload);
            }));
            s
        };
        #[cfg(not(target_os = "android"))]
        let state = AppState::open(dir.join(DB_FILE))?;
        app.manage(state);
        Ok(())
    });
    commands::register(builder)
        .run(tauri::generate_context!())
        .expect("error while running Snipt");
}
