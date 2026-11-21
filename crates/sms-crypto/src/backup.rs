//! `.mysms.enc` v1 backup envelope — reader/writer compatible with backup.ts.
//!
//! Envelope: JSON `{format, cipher, kdf, iterations, createdAtMs, provider,
//! messageCount, saltB64, ivB64, ciphertextB64}`; key = PBKDF2-HMAC-SHA256(JS-
//! trimmed passphrase as UTF-8, salt, iterations, 32 B); ciphertext = AES-256-GCM
//! ciphertext‖tag; base64 = standard alphabet with padding.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sms_types::{BudgetLimits, PaymentCycleConfig, RawSms, time::is_valid_timestamp_ms};
use zeroize::{Zeroize, Zeroizing};

use crate::{gcm_open, gcm_seal, jstrim::js_trim, random_bytes};

pub const FORMAT_V1: &str = "mySMS-encrypted-backup-v1";
/// TS default / fallback when the envelope's iteration count is missing or < 10 000.
pub const TS_DEFAULT_ITERATIONS: u32 = 150_000;
/// Rust writer default (OWASP guidance for PBKDF2-HMAC-SHA256); still v1.
pub const WRITER_ITERATIONS: u32 = 600_000;
/// Reader DoS guard (ARCHITECTURE §5).
pub const MAX_ITERATIONS: u32 = 10_000_000;
const MIN_ITERATIONS: f64 = 10_000.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloudBackupProvider {
    #[default]
    DevicePicker,
    GoogleDrive,
    Onedrive,
    Dropbox,
}

fn schema_v1() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MySmsBackupPayload {
    #[serde(default = "schema_v1")]
    pub schema_version: u32,
    pub app: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub provider: CloudBackupProvider,
    pub messages: Vec<RawSms>,
    #[serde(default)]
    pub archived_thread_ids: Vec<i64>,
    #[serde(default)]
    pub blocked_senders: Vec<String>,
    #[serde(default)]
    pub completed_reminder_sms_ids: Vec<i64>,
    #[serde(default)]
    pub cycle_config: PaymentCycleConfig,
    #[serde(default)]
    pub budget_limits: BudgetLimits,
    #[serde(default)]
    pub scan_schedule_day: String,
    #[serde(default)]
    pub auto_translate_to_english: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedBackupEnvelope {
    pub format: String,
    pub cipher: String,
    pub kdf: String,
    pub iterations: u32,
    pub created_at_ms: i64,
    pub provider: CloudBackupProvider,
    pub message_count: usize,
    pub salt_b64: String,
    pub iv_b64: String,
    pub ciphertext_b64: String,
}

/// Error texts equal the TS prototype's messages (UI shows them verbatim).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BackupError {
    #[error("Passphrase must be at least 6 characters long.")]
    PassphraseTooShort,
    #[error("Enter your backup passphrase to decrypt this file.")]
    PassphraseMissing,
    #[error("Invalid backup file format. Expected a .mysms.enc vault file.")]
    InvalidFormat,
    #[error("Unsupported or corrupted mySMS backup file.")]
    Unsupported,
    #[error("Backup iteration count exceeds the supported maximum.")]
    IterationsTooHigh,
    #[error("Incorrect passphrase or tampered backup file. Could not decrypt.")]
    DecryptFailed,
    #[error("Decrypted payload is not a valid mySMS archive.")]
    NotAnArchive,
}

fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(passphrase.as_bytes(), salt, iterations, &mut key[..]);
    key
}

/// Encrypt a payload. Returns the envelope and its pretty JSON
/// (`JSON.stringify(envelope, null, 2)` shape).
pub fn create_encrypted_backup(
    payload: &MySmsBackupPayload,
    passphrase: &str,
    iterations: u32,
) -> Result<(EncryptedBackupEnvelope, String), BackupError> {
    let pass = Zeroizing::new(js_trim(passphrase).to_string());
    if sms_types::js::utf16_len(&pass) < 6 {
        return Err(BackupError::PassphraseTooShort);
    }
    if !(10_000..=MAX_ITERATIONS).contains(&iterations) {
        return Err(BackupError::IterationsTooHigh);
    }
    let salt: [u8; 16] = random_bytes();
    let iv: [u8; 12] = random_bytes();
    let key = derive_key(&pass, &salt, iterations);

    let plaintext = Zeroizing::new(serde_json::to_vec(payload).expect("payload serialises"));
    let ciphertext = gcm_seal(&key, &iv, &plaintext);

    let envelope = EncryptedBackupEnvelope {
        format: FORMAT_V1.into(),
        cipher: "AES-256-GCM".into(),
        kdf: "PBKDF2-SHA256".into(),
        iterations,
        created_at_ms: payload.created_at_ms,
        provider: payload.provider,
        message_count: payload.messages.len(),
        salt_b64: B64.encode(salt),
        iv_b64: B64.encode(iv),
        ciphertext_b64: B64.encode(&ciphertext),
    };
    let serialized = serde_json::to_string_pretty(&envelope).expect("envelope serialises");
    Ok((envelope, serialized))
}

fn non_empty_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Iteration rule of backup.ts (`>= 10 000` else 150 000) plus the DoS cap.
fn effective_iterations(v: Option<&Value>) -> Result<u32, BackupError> {
    match v.and_then(Value::as_f64) {
        Some(n) if n >= MIN_ITERATIONS => {
            if n > f64::from(MAX_ITERATIONS) {
                Err(BackupError::IterationsTooHigh)
            } else if n.fract() != 0.0 {
                Err(BackupError::Unsupported)
            } else {
                Ok(n as u32)
            }
        }
        _ => Ok(TS_DEFAULT_ITERATIONS),
    }
}

/// Decrypt and validate a `.mysms.enc` file.
pub fn restore_encrypted_backup(
    serialized: &str,
    passphrase: &str,
) -> Result<MySmsBackupPayload, BackupError> {
    let pass = Zeroizing::new(js_trim(passphrase).to_string());
    if pass.is_empty() {
        return Err(BackupError::PassphraseMissing);
    }
    let parsed: Value = serde_json::from_str(serialized).map_err(|_| BackupError::InvalidFormat)?;
    if parsed.get("format").and_then(Value::as_str) != Some(FORMAT_V1) {
        return Err(BackupError::Unsupported);
    }
    let (Some(salt_b64), Some(iv_b64), Some(ct_b64)) = (
        non_empty_str(&parsed, "saltB64"),
        non_empty_str(&parsed, "ivB64"),
        non_empty_str(&parsed, "ciphertextB64"),
    ) else {
        return Err(BackupError::Unsupported);
    };
    let salt = B64.decode(salt_b64).map_err(|_| BackupError::Unsupported)?;
    let iv: [u8; 12] = B64
        .decode(iv_b64)
        .map_err(|_| BackupError::Unsupported)?
        .try_into()
        .map_err(|_| BackupError::Unsupported)?;
    let ciphertext = B64.decode(ct_b64).map_err(|_| BackupError::Unsupported)?;
    let iterations = effective_iterations(parsed.get("iterations"))?;

    let key = derive_key(&pass, &salt, iterations);
    let mut plain =
        Zeroizing::new(gcm_open(&key, &iv, &ciphertext).ok_or(BackupError::DecryptFailed)?);

    let decoded: Value = serde_json::from_slice(&plain).map_err(|_| BackupError::DecryptFailed)?;
    plain.zeroize();
    if decoded.get("app").and_then(Value::as_str) != Some("mySMS")
        || !decoded.get("messages").is_some_and(Value::is_array)
    {
        return Err(BackupError::NotAnArchive);
    }
    let payload: MySmsBackupPayload =
        serde_json::from_value(decoded).map_err(|_| BackupError::NotAnArchive)?;
    // Restore boundary: reject timestamps a JS Date cannot represent.
    if !is_valid_timestamp_ms(payload.created_at_ms)
        || !payload
            .messages
            .iter()
            .all(|m| is_valid_timestamp_ms(m.timestamp_ms))
    {
        return Err(BackupError::NotAnArchive);
    }
    Ok(payload)
}

/// `mysms-backup-YYYY-MM-DD.mysms.enc` in local time (backup.ts).
pub fn build_backup_filename(now_ms: i64, tz_offset_min: i32) -> String {
    let (y, m, d) = sms_types::time::local_date(now_ms, tz_offset_min);
    format!("mysms-backup-{y}-{m:02}-{d:02}.mysms.enc")
}
