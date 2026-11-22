//! Webview-facing commands (`plugin:telephony|<name>`). Each runs the
//! (blocking) native call off the IPC thread.

use tauri::{AppHandle, Runtime, command};

use crate::models::*;
use crate::{Error, Result, TelephonyExt};

async fn blocking<R, T, F>(app: AppHandle<R>, f: F) -> Result<T>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(&AppHandle<R>) -> Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|_| Error::Unsupported)?
}

#[command]
pub(crate) async fn request_default_sms_role<R: Runtime>(app: AppHandle<R>) -> Result<RoleResult> {
    blocking(app, |a| a.telephony().request_default_sms_role()).await
}

#[command]
pub(crate) async fn status<R: Runtime>(app: AppHandle<R>) -> Result<Status> {
    blocking(app, |a| a.telephony().status()).await
}

#[command]
pub(crate) async fn copy_sensitive<R: Runtime>(
    app: AppHandle<R>,
    text: String,
    clear_after_ms: Option<u64>,
) -> Result<Done> {
    blocking(app, move |a| {
        a.telephony().copy_sensitive(CopyRequest {
            text,
            clear_after_ms,
        })
    })
    .await
}

#[command]
pub(crate) async fn set_secure<R: Runtime>(
    app: AppHandle<R>,
    scope: SecureScope,
    on: bool,
) -> Result<Done> {
    blocking(app, move |a| {
        a.telephony().set_secure(SecureRequest { scope, on })
    })
    .await
}

#[command]
pub(crate) async fn display_boost<R: Runtime>(app: AppHandle<R>, on: bool) -> Result<Done> {
    blocking(app, move |a| {
        a.telephony().display_boost(ToggleRequest { on })
    })
    .await
}

#[command]
pub(crate) async fn take_compose<R: Runtime>(app: AppHandle<R>) -> Result<Compose> {
    blocking(app, |a| a.telephony().take_compose()).await
}

#[command]
pub(crate) async fn import_inbox<R: Runtime>(app: AppHandle<R>) -> Result<ImportResult> {
    blocking(app, |a| a.telephony().import_inbox()).await
}
