//! Desktop (Linux backup viewer): no radio. Queries report "nothing
//! available"; display/back/secure toggles are harmless no-ops; anything that
//! would touch SMS returns `Error::Unsupported`.

use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use tauri::{AppHandle, Runtime, plugin::PluginApi};

use crate::Error;
use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Telephony<R>> {
    Ok(Telephony(PhantomData))
}

pub struct Telephony<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> Telephony<R> {
    pub fn status(&self) -> crate::Result<Status> {
        Ok(Status::default())
    }

    pub fn request_default_sms_role(&self) -> crate::Result<RoleResult> {
        Err(Error::Unsupported)
    }

    pub fn send(&self, _req: SendRequest) -> crate::Result<Done> {
        Err(Error::Unsupported)
    }

    pub fn copy_sensitive(&self, _req: CopyRequest) -> crate::Result<Done> {
        Err(Error::Unsupported)
    }

    pub fn set_secure(&self, _req: SecureRequest) -> crate::Result<Done> {
        Ok(Done { ok: false })
    }

    pub fn display_boost(&self, _req: ToggleRequest) -> crate::Result<Done> {
        Ok(Done { ok: false })
    }

    pub fn set_system_bars(&self, _req: SystemBarsRequest) -> crate::Result<Done> {
        Ok(Done { ok: false })
    }

    pub fn update_widgets(&self, _req: WidgetRequest) -> crate::Result<Done> {
        Ok(Done { ok: false })
    }

    pub fn set_back_handler(&self, _req: BackRequest) -> crate::Result<Done> {
        Ok(Done { ok: false })
    }

    pub fn power_state(&self) -> crate::Result<PowerState> {
        Ok(PowerState::default())
    }

    pub fn provider_mark_read(&self, _ids: ProviderIds) -> crate::Result<ProviderResults> {
        Err(Error::Unsupported)
    }

    pub fn provider_delete(&self, _ids: ProviderIds) -> crate::Result<ProviderResults> {
        Err(Error::Unsupported)
    }

    pub fn import_inbox(&self) -> crate::Result<ImportResult> {
        Err(Error::Unsupported)
    }

    pub fn take_compose(&self) -> crate::Result<Compose> {
        Ok(Compose::default())
    }

    /// Desktop uses native file dialogs from the app crate instead.
    pub fn saf_save(&self, _req: SafSaveRequest) -> crate::Result<SafSaveResult> {
        Err(Error::Unsupported)
    }

    pub fn saf_open(&self) -> crate::Result<SafOpenResult> {
        Err(Error::Unsupported)
    }
}
