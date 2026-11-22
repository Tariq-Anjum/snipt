//! Native file pickers for the encrypted backup (PLAN S14).
//!
//! Android: Storage Access Framework through the telephony plugin
//! (`ACTION_CREATE_DOCUMENT` / `ACTION_OPEN_DOCUMENT`); the ciphertext is
//! streamed by Kotlin through `ContentResolver`. Linux: `tauri-plugin-dialog`
//! called from Rust, so the webview needs no dialog or fs permission at all.
//! Both run on a blocking thread and never see the passphrase.

use tauri::{AppHandle, Runtime};

#[cfg(not(target_os = "android"))]
use crate::core::MAX_BACKUP_BYTES;
use crate::error::{CmdError, CmdResult};

/// Ask the user where to save `content`. `Ok(false)` = cancelled.
pub fn save<R: Runtime>(app: &AppHandle<R>, name: &str, content: &str) -> CmdResult<bool> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_telephony::{TelephonyExt, models::SafSaveRequest};
        let r = app
            .telephony()
            .saf_save(SafSaveRequest {
                name: name.to_owned(),
                content: content.to_owned(),
            })
            .map_err(|e| CmdError(e.to_string()))?;
        Ok(!r.cancelled)
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name(name)
            .add_filter("Snipt encrypted backup", &["enc"])
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = path
            .into_path()
            .map_err(|_| CmdError("could not use the chosen location".into()))?;
        write_private(&path, content)?;
        Ok(true)
    }
}

/// Ask the user for a backup file. `Ok(None)` = cancelled; else (name, text).
pub fn open<R: Runtime>(app: &AppHandle<R>) -> CmdResult<Option<(String, String)>> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_telephony::TelephonyExt;
        let r = app
            .telephony()
            .saf_open()
            .map_err(|e| CmdError(e.to_string()))?;
        Ok((!r.cancelled).then_some((r.name, r.content)))
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let Some(path) = app
            .dialog()
            .file()
            .add_filter("Snipt encrypted backup", &["enc", "json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(|_| CmdError("could not use the chosen file".into()))?;
        let text = read_capped(&path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some((name, text)))
    }
}

#[cfg(not(target_os = "android"))]
fn write_private(path: &std::path::Path, content: &str) -> CmdResult<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    o.mode(0o600);
    let mut f = o
        .open(path)
        .map_err(|_| CmdError("could not write the backup file".into()))?;
    f.write_all(content.as_bytes())
        .and_then(|()| f.sync_all())
        .map_err(|_| CmdError("could not write the backup file".into()))
}

#[cfg(not(target_os = "android"))]
fn read_capped(path: &std::path::Path) -> CmdResult<String> {
    use std::io::Read;
    let f =
        std::fs::File::open(path).map_err(|_| CmdError("could not read the backup file".into()))?;
    let mut buf = String::new();
    f.take(MAX_BACKUP_BYTES as u64 + 1)
        .read_to_string(&mut buf)
        .map_err(|_| CmdError("could not read the backup file".into()))?;
    if buf.len() > MAX_BACKUP_BYTES {
        return Err(CmdError::invalid("backup too large"));
    }
    Ok(buf)
}
