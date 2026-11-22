//! Wire types shared by the Rust commands and the Kotlin plugin (camelCase JSON).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub sub_id: i32,
    /// 0-based SIM slot.
    pub slot: i32,
    pub carrier: String,
}

/// What the Android shell can do right now.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Snipt holds `android.app.role.SMS`.
    pub default_sms_role: bool,
    /// Telephony feature present and SEND_SMS granted.
    pub can_send: bool,
    /// Active SIM count (0 when READ_PHONE_STATE is not granted).
    pub sim_slots: u8,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    /// MMS download is S15; false until then.
    pub mms: bool,
    /// POST_NOTIFICATIONS granted (always true below API 33).
    pub notifications: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoleResult {
    pub held: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    /// Store id of the queued outgoing message (`queue_outgoing`).
    pub id: i64,
    pub address: String,
    pub body: String,
    pub sim_slot: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CopyRequest {
    pub text: String,
    /// Wipe delay; clamped to 5..=120 s by the shell, default 30 s.
    #[serde(default)]
    pub clear_after_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SecureScope {
    /// A sensitive screen (OTP vault) is showing.
    Vault,
    /// User preference: secure every screen.
    Global,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecureRequest {
    pub scope: SecureScope,
    pub on: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToggleRequest {
    pub on: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PowerState {
    pub power_save: bool,
    /// `PowerManager.getCurrentThermalStatus()` (0 none .. 6 shutdown).
    pub thermal: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderIds {
    pub provider_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: i64,
    pub skipped: i64,
}

/// Draft handed over by another app (SENDTO sms:/smsto:, SEND text/plain).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Compose {
    pub present: bool,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    #[serde(default)]
    pub ok: bool,
}

/// Storage Access Framework save (`ACTION_CREATE_DOCUMENT`): the user picks
/// the destination (local folder, Drive, OneDrive, Dropbox, any provider), then
/// the shell streams `content` into it through `ContentResolver`. The content
/// is the already-encrypted backup envelope; it is never logged.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafSaveRequest {
    /// Suggested file name (`EXTRA_TITLE`).
    pub name: String,
    pub content: String,
}

// `content` is ciphertext, but keep it out of any `{:?}` log regardless.
impl std::fmt::Debug for SafSaveRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafSaveRequest")
            .field("name", &self.name)
            .field("content", &format_args!("<{} bytes>", self.content.len()))
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafSaveResult {
    /// The user dismissed the picker; nothing was written.
    pub cancelled: bool,
}

/// Result of `ACTION_OPEN_DOCUMENT`: the chosen file's name and text.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafOpenResult {
    pub cancelled: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
}

impl std::fmt::Debug for SafOpenResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafOpenResult")
            .field("cancelled", &self.cancelled)
            .field("name", &self.name)
            .field("content", &format_args!("<{} bytes>", self.content.len()))
            .finish()
    }
}
