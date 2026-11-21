//! Port of TS test 7 (OTP vault AES-GCM round-trip) + negative cases.

use sms_crypto::vault::*;

#[test]
fn roundtrip() {
    let v = OtpVault::new_random();
    let e = v.encrypt_otp("849201");
    assert_eq!(e.algorithm, "AES-256-GCM");
    assert_ne!(e.ciphertext_b64, "849201");
    assert_eq!(v.decrypt_otp(&e).unwrap().as_str(), "849201");
    // fresh nonce per item
    assert_ne!(v.encrypt_otp("849201").iv_b64, e.iv_b64);
}

#[test]
fn wrong_key_and_tamper_rejected() {
    let v = OtpVault::from_key([7u8; 32]);
    let e = v.encrypt_otp("482913");
    assert_eq!(
        OtpVault::from_key([8u8; 32]).decrypt_otp(&e).unwrap_err(),
        VaultError::DecryptFailed
    );
    let mut t = e.clone();
    t.iv_b64 = "AAAA".into();
    assert_eq!(v.decrypt_otp(&t).unwrap_err(), VaultError::Malformed);
    let mut t = e.clone();
    t.ciphertext_b64 = base64_flip(&e.ciphertext_b64);
    assert_eq!(v.decrypt_otp(&t).unwrap_err(), VaultError::DecryptFailed);
}

fn base64_flip(s: &str) -> String {
    let mut c = s.to_string();
    let r = if c.starts_with('A') { "B" } else { "A" };
    c.replace_range(0..1, r);
    c
}
