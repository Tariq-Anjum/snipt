//! Snipt: Tauri shell. All logic lives in the `sms-*` crates; this crate only
//! owns the database handle, input validation and the command surface.

pub mod commands;
pub mod core;
pub mod error;

use tauri::Manager;

use crate::core::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().setup(|app| {
        let dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&dir)?;
        let state = AppState::open(dir.join("snipt.db"))?;
        app.manage(state);
        Ok(())
    });
    commands::register(builder)
        .run(tauri::generate_context!())
        .expect("error while running Snipt");
}
