//! Android: every method forwards to the Kotlin `TelephonyPlugin` (`@Command`
//! of the same camelCase name). `run_mobile_plugin` blocks until Kotlin
//! resolves, so callers run these on a blocking thread.

use serde::de::DeserializeOwned;
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

use crate::models::*;

const PLUGIN_IDENTIFIER: &str = "app.snipt.telephony";

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Telephony<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "TelephonyPlugin")?;
    Ok(Telephony(handle))
}

pub struct Telephony<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Telephony<R> {
    fn call<T: DeserializeOwned>(
        &self,
        cmd: &str,
        payload: impl serde::Serialize,
    ) -> crate::Result<T> {
        Ok(self.0.run_mobile_plugin(cmd, payload)?)
    }

    pub fn status(&self) -> crate::Result<Status> {
        self.call("status", ())
    }

    pub fn request_default_sms_role(&self) -> crate::Result<RoleResult> {
        self.call("requestDefaultSmsRole", ())
    }

    pub fn send(&self, req: SendRequest) -> crate::Result<Done> {
        self.call("send", req)
    }

    pub fn copy_sensitive(&self, req: CopyRequest) -> crate::Result<Done> {
        self.call("copySensitive", req)
    }

    pub fn set_secure(&self, req: SecureRequest) -> crate::Result<Done> {
        self.call("setSecure", req)
    }

    pub fn display_boost(&self, req: ToggleRequest) -> crate::Result<Done> {
        self.call("displayBoost", req)
    }

    pub fn set_system_bars(&self, req: SystemBarsRequest) -> crate::Result<Done> {
        self.call("setSystemBars", req)
    }

    pub fn update_widgets(&self, req: WidgetRequest) -> crate::Result<Done> {
        self.call("updateWidgets", req)
    }

    pub fn set_back_handler(&self, req: BackRequest) -> crate::Result<Done> {
        self.call("setBackHandler", req)
    }

    pub fn power_state(&self) -> crate::Result<PowerState> {
        self.call("powerState", ())
    }

    pub fn provider_mark_read(&self, ids: ProviderIds) -> crate::Result<ProviderResults> {
        self.call("providerMarkRead", ids)
    }

    pub fn provider_delete(&self, ids: ProviderIds) -> crate::Result<ProviderResults> {
        self.call("providerDelete", ids)
    }

    pub fn import_inbox(&self) -> crate::Result<ImportResult> {
        self.call("importInbox", ())
    }

    pub fn take_compose(&self) -> crate::Result<Compose> {
        self.call("takeCompose", ())
    }

    pub fn saf_save(&self, req: SafSaveRequest) -> crate::Result<SafSaveResult> {
        self.call("safSave", req)
    }

    pub fn saf_open(&self) -> crate::Result<SafOpenResult> {
        self.call("safOpen", ())
    }
}
