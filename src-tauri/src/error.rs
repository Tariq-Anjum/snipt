//! Command error: serialises to a plain string for the frontend. Messages are
//! generic by construction (no SMS bodies, OTPs or passphrases).

use serde::{Serialize, Serializer};

#[derive(Debug, PartialEq, Eq)]
pub struct CmdError(pub String);

pub type CmdResult<T> = Result<T, CmdError>;

impl CmdError {
    pub fn invalid(msg: &str) -> Self {
        CmdError(format!("invalid input: {msg}"))
    }
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CmdError {}

impl Serialize for CmdError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl From<sms_store::StoreError> for CmdError {
    fn from(e: sms_store::StoreError) -> Self {
        CmdError(e.to_string())
    }
}

impl From<sms_crypto::BackupError> for CmdError {
    fn from(e: sms_crypto::BackupError) -> Self {
        CmdError(e.to_string())
    }
}

impl From<sms_crypto::VaultError> for CmdError {
    fn from(e: sms_crypto::VaultError) -> Self {
        CmdError(e.to_string())
    }
}
