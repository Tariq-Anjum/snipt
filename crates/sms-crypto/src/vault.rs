//! OTP vault: AES-256-GCM per item under a 32-byte data key (DEK) held in
//! `Zeroizing`. On Android the DEK is wrapped by an Android Keystore key
//! (Kotlin side); on Linux it is ephemeral per session (as the TS prototype).

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{gcm_open, gcm_seal, random_bytes};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedOtpPayload {
    pub iv_b64: String,
    pub ciphertext_b64: String,
    /// Always `"AES-256-GCM"`.
    pub algorithm: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("Malformed vault payload.")]
    Malformed,
    #[error("Vault item could not be decrypted.")]
    DecryptFailed,
}

pub struct OtpVault {
    dek: Zeroizing<[u8; 32]>,
}

impl OtpVault {
    /// Fresh random DEK (Linux: per session).
    pub fn new_random() -> Self {
        OtpVault {
            dek: Zeroizing::new(random_bytes()),
        }
    }

    /// DEK supplied by the platform (Android: unwrapped by Keystore).
    pub fn from_key(dek: [u8; 32]) -> Self {
        OtpVault {
            dek: Zeroizing::new(dek),
        }
    }

    pub fn encrypt_otp(&self, plain_code: &str) -> EncryptedOtpPayload {
        let iv: [u8; 12] = random_bytes();
        let ct = gcm_seal(&self.dek, &iv, plain_code.as_bytes());
        EncryptedOtpPayload {
            iv_b64: B64.encode(iv),
            ciphertext_b64: B64.encode(ct),
            algorithm: "AES-256-GCM".into(),
        }
    }

    pub fn decrypt_otp(
        &self,
        payload: &EncryptedOtpPayload,
    ) -> Result<Zeroizing<String>, VaultError> {
        if payload.algorithm != "AES-256-GCM" {
            return Err(VaultError::Malformed);
        }
        let iv: [u8; 12] = B64
            .decode(&payload.iv_b64)
            .map_err(|_| VaultError::Malformed)?
            .try_into()
            .map_err(|_| VaultError::Malformed)?;
        let ct = B64
            .decode(&payload.ciphertext_b64)
            .map_err(|_| VaultError::Malformed)?;
        let plain = Zeroizing::new(gcm_open(&self.dek, &iv, &ct).ok_or(VaultError::DecryptFailed)?);
        let s = std::str::from_utf8(&plain).map_err(|_| VaultError::DecryptFailed)?;
        Ok(Zeroizing::new(s.to_string()))
    }
}
