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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    #[serde(default)]
    pub ok: bool,
}
