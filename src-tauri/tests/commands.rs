//! Command-layer tests: call `Core` (the code every async command wraps)
//! against an in-memory store, no window involved.

use sms_crypto::EncryptedOtpPayload;
use sms_engine::cleanup::CleanupBucketId;
use sms_store::Store;
use sms_types::{BudgetLimits, PaymentCycleConfig};
use snipt_lib::core::{AppState, Core, MAX_QUERY_CHARS, platform_capabilities};

fn now() -> i64 {
    snipt_lib::core::now_ms()
}

#[test]
fn import_seed_is_gated_by_build_flavour() {
    let mut c = Core::new(Store::open_in_memory(0).unwrap());
    let r = c.import_seed();
    if snipt_lib::core::seed_allowed() {
        assert_eq!(r.unwrap().inserted, 27);
    } else {
        assert!(r.is_err());
        assert_eq!(c.store().message_count().unwrap(), 0);
    }
}

fn seeded() -> Core {
    // Seed through the store: `import_seed` is compiled out of release gates.
    let mut st = Store::open_in_memory(330).unwrap();
    let r = st
        .ingest_batch(&sms_testdata::create_seed_messages(now()))
        .unwrap();
    assert_eq!(r.inserted, 27);
    Core::new(st)
}

#[test]
fn capabilities_is_linux_on_desktop() {
    assert_eq!(platform_capabilities().platform, "linux");
    assert_eq!(
        serde_json::to_string(&platform_capabilities()).unwrap(),
        r#"{"platform":"linux"}"#
    );
}

#[test]
fn threads_paging_and_filter_validation() {
    let c = seeded();
    let p1 = c.threads_page("All", None, None, Some(5)).unwrap();
    assert_eq!(p1.threads.len(), 5);
    let next = p1.next.expect("more pages");
    let p2 = c.threads_page("All", None, Some(next), Some(500)).unwrap();
    assert!(!p2.threads.is_empty());
    assert!(
        p1.threads
            .iter()
            .all(|a| p2.threads.iter().all(|b| a.thread_id != b.thread_id))
    );
    assert!(c.threads_page("Bogus", None, None, None).is_err());
    assert!(c.threads_page("All", Some(2), None, None).is_err());
    let t = p1.threads[0].thread_id;
    let m = c.messages_page(t, None, Some(1)).unwrap();
    assert_eq!(m.messages.len(), 1);
}

#[test]
fn search_and_input_limits() {
    let c = seeded();
    let hits = c.search("All", None, "hdfc", None).unwrap();
    assert!(!hits.is_empty());
    assert!(
        c.search("All", None, &"x".repeat(MAX_QUERY_CHARS + 1), None)
            .is_err()
    );
}

#[test]
fn flags_archive_block_delete() {
    let mut c = seeded();
    let t = c.threads_page("All", None, None, Some(1)).unwrap().threads[0].clone();
    c.archive(t.thread_id, true).unwrap();
    let arch = c.threads_page("Archived", None, None, None).unwrap();
    assert!(arch.threads.iter().any(|x| x.thread_id == t.thread_id));
    c.archive(t.thread_id, false).unwrap();
    c.block(&t.sender, true).unwrap();
    assert!(c.block("   ", true).is_err());
    let mid = c
        .messages_page(t.thread_id, None, Some(1))
        .unwrap()
        .messages[0]
        .sms
        .id;
    assert!(c.mark_read(mid).unwrap());
    assert!(c.star(mid, true).unwrap());
    assert!(!c.mark_read(-5).unwrap());
    assert_eq!(c.delete_messages(&[mid, -5]).unwrap(), 1);
    assert!(c.delete_messages(&vec![1; 20_001]).is_err());
}

#[test]
fn settings_persist_and_validate() {
    let mut c = seeded();
    assert_eq!(c.cycle_config().unwrap(), PaymentCycleConfig::default());
    let cfg = PaymentCycleConfig {
        upi: 5.0,
        credit_card: 20.0,
        savings_account: 1.0,
    };
    c.set_cycle(cfg).unwrap();
    assert_eq!(c.cycle_config().unwrap(), cfg);
    assert!(c.set_cycle(PaymentCycleConfig { upi: 0.0, ..cfg }).is_err());
    assert!(
        c.set_cycle(PaymentCycleConfig {
            upi: f64::NAN,
            ..cfg
        })
        .is_err()
    );
    let lim = BudgetLimits {
        food_dining: 1.0,
        ..BudgetLimits::default()
    };
    c.set_budget(lim).unwrap();
    assert_eq!(c.budget_limits().unwrap(), lim);
    assert!(
        c.set_budget(BudgetLimits {
            general_other: -1.0,
            ..lim
        })
        .is_err()
    );
    c.finance_summary(false).unwrap();
    c.budget_stats(true).unwrap();
}

#[test]
fn otp_vault_reveal_roundtrip() {
    let c = seeded();
    let items = c.otp_items().unwrap();
    assert!(!items.is_empty());
    let v = &items[0];
    assert!(v.item.otp.code.chars().all(|ch| ch == '\u{2022}'));
    let code = c.vault_reveal(&v.vault).unwrap();
    assert!(!code.is_empty() && code.chars().all(|ch| ch != '\u{2022}'));
    let mut bad: EncryptedOtpPayload = v.vault.clone();
    bad.ciphertext_b64 = "AAAA".into();
    assert!(c.vault_reveal(&bad).is_err());
    // another session's vault cannot open it
    assert!(seeded().vault_reveal(&v.vault).is_err());
}

#[test]
fn scan_and_cleanup_only_touch_scanned_buckets() {
    let mut c = seeded();
    let before = c.store().message_count().unwrap();
    let report = c.weekly_scan().unwrap();
    let n: usize = report
        .buckets
        .iter()
        .filter(|b| b.id == CleanupBucketId::SpamPhishing)
        .map(|b| b.message_ids.len())
        .sum();
    let deleted = c.apply_cleanup(&[CleanupBucketId::SpamPhishing]).unwrap();
    assert_eq!(deleted, n);
    assert_eq!(c.store().message_count().unwrap(), before - n as i64);
    assert_eq!(c.apply_cleanup(&[]).unwrap(), 0);
}

#[test]
fn translate_works_and_is_bounded() {
    let c = seeded();
    let r = c.translate("आपका खाता डेबिट हुआ").unwrap();
    assert!(r.needs_translation);
    assert!(c.translate(&"a".repeat(20_001)).is_err());
}

#[test]
fn backup_roundtrip_through_core() {
    let mut a = seeded();
    let m = a
        .messages_page(
            a.threads_page("All", None, None, Some(1)).unwrap().threads[0].thread_id,
            None,
            Some(1),
        )
        .unwrap()
        .messages[0]
        .sms
        .id;
    a.star(m, true).unwrap();
    a.set_cycle(PaymentCycleConfig {
        upi: 7.0,
        credit_card: 15.0,
        savings_account: 1.0,
    })
    .unwrap();
    assert!(a.backup_create("abc").is_err(), "short passphrase rejected");
    let f = a.backup_create("correct horse").unwrap();
    assert!(f.filename.ends_with(".mysms.enc"));
    assert_eq!(f.message_count, 27);

    let mut b = Core::new(Store::open_in_memory(0).unwrap());
    assert!(b.backup_restore(&f.content, "wrong pass").is_err());
    let r = b.backup_restore(&f.content, "correct horse").unwrap();
    assert_eq!(r.ingest.inserted, 27);
    assert_eq!(b.store().message_count().unwrap(), 27);
    assert_eq!(b.cycle_config().unwrap().upi, 7.0);
    // idempotent merge
    let r2 = b.backup_restore(&f.content, "correct horse").unwrap();
    assert_eq!(r2.ingest.inserted, 0);
}

#[test]
fn app_state_async_run_over_file_db_uses_wal() {
    let dir = std::env::temp_dir().join(format!("snipt-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("snipt.db");
    {
        let mut st = Store::open(&db, 0).unwrap();
        st.ingest_batch(&sms_testdata::create_seed_messages(now()))
            .unwrap();
    }
    let state = AppState::open(&db).unwrap();
    let (n, mode) = tauri::async_runtime::block_on(async {
        state
            .run(|c| {
                let mode: String = c
                    .store()
                    .connection()
                    .pragma_query_value(None, "journal_mode", |r| r.get(0))
                    .unwrap();
                Ok((c.store().message_count().unwrap(), mode))
            })
            .await
            .unwrap()
    });
    assert_eq!(n, 27);
    assert_eq!(mode, "wal");
    drop(state);
    std::fs::remove_dir_all(&dir).unwrap();
}
