//! mySMS crypto: `.mysms.enc` v1 backups (PBKDF2-HMAC-SHA256 + AES-256-GCM,
//! format-compatible with src/engine/backup.ts) and the OTP vault.

#![forbid(unsafe_code)]

pub mod backup;
pub mod jstrim;
mod kdf;
pub mod vault;

pub use backup::{
    BackupError, CloudBackupProvider, EncryptedBackupEnvelope, MySmsBackupPayload,
    build_backup_filename, create_encrypted_backup, restore_encrypted_backup,
};
pub use vault::{EncryptedOtpPayload, OtpVault, VaultError};

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, Payload},
};

/// AES-256-GCM seal: returns ciphertext ‖ 16-byte tag (WebCrypto layout).
pub(crate) fn gcm_seal(key: &[u8; 32], iv: &[u8; 12], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("32-byte key");
    cipher
        .encrypt(
            iv.into(),
            Payload {
                msg: plaintext,
                aad: b"",
            },
        )
        .expect("AES-GCM encryption cannot fail for in-memory buffers")
}

/// AES-256-GCM open with constant-time tag check (aes-gcm). `None` on failure.
pub(crate) fn gcm_open(key: &[u8; 32], iv: &[u8; 12], ciphertext: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("32-byte key");
    cipher
        .decrypt(
            iv.into(),
            Payload {
                msg: ciphertext,
                aad: b"",
            },
        )
        .ok()
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("OS random number generator unavailable");
    b
}
