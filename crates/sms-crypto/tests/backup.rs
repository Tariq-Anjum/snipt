//! Port of TS test 10 + cross-compat with the TS-written fixture.

use sms_crypto::backup::*;
use sms_testdata::create_seed_messages;
use sms_types::{BudgetLimits, PaymentCycleConfig, RawSms};
use std::time::Instant;

/// Test passphrase constant (same literal as engine.test.ts / gen-fixtures.ts).
const PASS: &str = "my-secret-pass-2026";

fn fixture_dir() -> String {
    format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))
}

fn payload(now: i64) -> MySmsBackupPayload {
    MySmsBackupPayload {
        schema_version: 1,
        app: "mySMS".into(),
        created_at_ms: now,
        provider: CloudBackupProvider::Onedrive,
        messages: create_seed_messages(now),
        archived_thread_ids: vec![110],
        blocked_senders: vec!["+918800291844".into()],
        completed_reminder_sms_ids: vec![1011],
        cycle_config: PaymentCycleConfig {
            credit_card: 12.0,
            ..PaymentCycleConfig::default()
        },
        budget_limits: BudgetLimits {
            food_dining: 6500.0,
            ..BudgetLimits::default()
        },
        scan_schedule_day: "Every Sunday · 9:00 AM".into(),
        auto_translate_to_english: true,
    }
}

#[test]
fn roundtrip_and_reject() {
    let now = 1_795_242_600_000;
    let p = payload(now);
    let (env, serialized) = create_encrypted_backup(&p, PASS, 10_000).unwrap();
    assert_eq!(env.cipher, "AES-256-GCM");
    assert_eq!(env.provider, CloudBackupProvider::Onedrive);
    assert_eq!(env.message_count, p.messages.len());
    assert!(!serialized.contains("SWIGGY INSTAMART"));
    assert!(serialized.starts_with("{\n  \"format\": \"mySMS-encrypted-backup-v1\""));

    let r = restore_encrypted_backup(&serialized, PASS).unwrap();
    assert_eq!(r.messages.len(), p.messages.len());
    assert_eq!(r.cycle_config.credit_card, 12.0);
    assert_eq!(r.budget_limits.food_dining, 6500.0);
    assert!(r.auto_translate_to_english);
    assert_eq!(r, p);

    let err = restore_encrypted_backup(&serialized, "wrong-passphrase").unwrap_err();
    assert!(err.to_string().contains("Incorrect passphrase"));

    // JS trim semantics (U+FEFF stripped) on both sides.
    assert!(restore_encrypted_backup(&serialized, "\u{FEFF} my-secret-pass-2026 \u{3000}").is_ok());
}

#[test]
fn ts_fixture_decrypts_in_rust() {
    let text = std::fs::read_to_string(format!("{}/ts_backup.mysms.enc", fixture_dir())).unwrap();
    let r = restore_encrypted_backup(&text, PASS).unwrap();
    let seed_fx: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/../sms-engine/tests/fixtures/seed_raw.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let seed: Vec<RawSms> = serde_json::from_value(seed_fx["messages"].clone()).unwrap();
    assert_eq!(r.messages, seed);
    assert_eq!(r.provider, CloudBackupProvider::Onedrive);
    assert_eq!(r.cycle_config.credit_card, 12.0);
    assert_eq!(r.budget_limits.food_dining, 6500.0);
    assert_eq!(r.scan_schedule_day, "Every Sunday · 9:00 AM");
    assert_eq!(
        restore_encrypted_backup(&text, "wrong-passphrase"),
        Err(BackupError::DecryptFailed)
    );
}

fn with_field(serialized: &str, key: &str, val: serde_json::Value) -> String {
    let mut v: serde_json::Value = serde_json::from_str(serialized).unwrap();
    v[key] = val;
    v.to_string()
}

#[test]
fn reader_rules() {
    let p = payload(0);
    let (_, s) = create_encrypted_backup(&p, PASS, 150_000).unwrap();
    // iterations < 10 000 → reader falls back to 150 000 (TS rule).
    assert!(restore_encrypted_backup(&with_field(&s, "iterations", 5000.into()), PASS).is_ok());
    assert!(restore_encrypted_backup(&with_field(&s, "iterations", "x".into()), PASS).is_ok());
    // DoS cap.
    assert_eq!(
        restore_encrypted_backup(&with_field(&s, "iterations", 50_000_000.into()), PASS),
        Err(BackupError::IterationsTooHigh)
    );
    assert_eq!(
        restore_encrypted_backup(&with_field(&s, "format", "v2".into()), PASS),
        Err(BackupError::Unsupported)
    );
    assert_eq!(
        restore_encrypted_backup(&with_field(&s, "saltB64", "".into()), PASS),
        Err(BackupError::Unsupported)
    );
    assert_eq!(
        restore_encrypted_backup("not json", PASS),
        Err(BackupError::InvalidFormat)
    );
    assert_eq!(
        restore_encrypted_backup(&s, "   "),
        Err(BackupError::PassphraseMissing)
    );
    // unknown fields ignored
    assert!(restore_encrypted_backup(&with_field(&s, "extra", 1.into()), PASS).is_ok());

    // tamper one ciphertext byte → tag check fails
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let mut ct = v["ciphertextB64"].as_str().unwrap().to_string();
    let flip = if ct.starts_with('A') { "B" } else { "A" };
    ct.replace_range(0..1, flip);
    assert_eq!(
        restore_encrypted_backup(&with_field(&s, "ciphertextB64", ct.into()), PASS),
        Err(BackupError::DecryptFailed)
    );

    assert_eq!(
        create_encrypted_backup(&p, " 12345 ", 150_000).unwrap_err(),
        BackupError::PassphraseTooShort
    );
}

#[test]
fn backup_filename_local_time() {
    let fx: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/../sms-engine/tests/fixtures/cycles.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let tz = fx["tzOffsetMin"].as_i64().unwrap() as i32;
    let cases = fx["backupFilenames"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        assert_eq!(
            build_backup_filename(c["nowMs"].as_i64().unwrap(), tz),
            c["name"].as_str().unwrap()
        );
    }
}

/// Records PBKDF2 cost at the writer default (ARCHITECTURE §5: drop to 310k if > 1.5 s on arm64).
#[test]
fn pbkdf2_600k_timing() {
    let p = MySmsBackupPayload {
        messages: vec![],
        ..payload(0)
    };
    let t = Instant::now();
    let (env, s) = create_encrypted_backup(&p, PASS, WRITER_ITERATIONS).unwrap();
    let enc = t.elapsed();
    let t = Instant::now();
    restore_encrypted_backup(&s, PASS).unwrap();
    let dec = t.elapsed();
    assert_eq!(env.iterations, 600_000);
    println!("PBKDF2-SHA256 600k: create {enc:?}, restore {dec:?}");
}
