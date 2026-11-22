//! Command-layer tests: call `Core` (the code every async command wraps)
//! against an in-memory store, no window involved.

use sms_crypto::EncryptedOtpPayload;
use sms_engine::cleanup::CleanupBucketId;
use sms_store::{IncomingSms, OutgoingSms, SendStatus, Store};
use sms_types::{BudgetLimits, PaymentCycleConfig};
use snipt_lib::core::{AppState, Capabilities, Core, MAX_QUERY_CHARS, platform_capabilities};

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
fn capabilities_is_linux_read_only_on_desktop() {
    assert_eq!(platform_capabilities().platform, "linux");
    assert_eq!(
        serde_json::to_string(&platform_capabilities()).unwrap(),
        r#"{"platform":"linux","canSend":false,"readOnly":true,"defaultSmsRole":false,"rcs":false,"mms":false,"simSlots":2,"nativePicker":true}"#
    );
}

fn phone_caps() -> Capabilities {
    Capabilities {
        platform: "android",
        can_send: true,
        read_only: false,
        ..platform_capabilities()
    }
}

fn seeded_phone() -> Core {
    let mut st = Store::open_in_memory(330).unwrap();
    st.ingest_batch(&sms_testdata::create_seed_messages(now()))
        .unwrap();
    Core::with_capabilities(st, phone_caps())
}

#[test]
fn get_thread_counts_settings_and_reminders() {
    let mut c = seeded();
    let row = c.threads_page("All", None, None, Some(1)).unwrap().threads[0].clone();
    assert_eq!(c.get_thread(row.thread_id).unwrap(), Some(row.clone()));
    assert_eq!(c.get_thread(-1).unwrap(), None);
    assert!(row.preview.chars().count() <= 140);

    let counts = c.inbox_counts(None).unwrap();
    assert_eq!(counts.total_messages, 27);
    assert_eq!(counts.capacity, 10_000);
    assert!(counts.unread > 0 && counts.filters.all > 0);
    assert!(c.inbox_counts(Some(2)).is_err());

    let s = c.get_settings().unwrap();
    assert_eq!(s.cycle_config, PaymentCycleConfig::default());
    assert_eq!(s.budget_limits, BudgetLimits::default());
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["cycleConfig"]["Credit Card"], 15.0);

    let rem = c.reminders().unwrap();
    assert!(!rem.is_empty() && rem.iter().all(|r| !r.is_done));
    c.set_reminder_done(rem[0].sms_id, true).unwrap();
    let rem2 = c.reminders().unwrap();
    assert!(rem2.iter().any(|r| r.sms_id == rem[0].sms_id && r.is_done));
    assert!(!rem2[0].is_done, "pending reminders sort first");
}

#[test]
fn ingest_incoming_returns_thread_and_redacted_notification() {
    let mut c = seeded();
    let msg = IncomingSms {
        sender: "AX-HDFCBK".into(),
        contact_name: None,
        body: "Your OTP is 551203 for login. Do not share.".into(),
        is_known_contact: false,
        sim_slot: 1,
        protocol: None,
        timestamp_ms: None,
    };
    let r = c.ingest_incoming(&msg, Some(42)).unwrap();
    assert!(r.inserted && r.notification.notify);
    let n = &r.notification;
    assert!(!n.message.sms.body.contains("551203"));
    assert!(!serde_json::to_string(&r).unwrap().contains("551203"));
    assert_eq!(n.message.otp.as_ref().unwrap().code, "\u{2022}".repeat(6));
    assert_eq!(
        c.vault_reveal(n.otp_vault.as_ref().unwrap()).unwrap(),
        "551203"
    );
    assert!(n.otp_expires_at_ms.unwrap() > now());
    assert_eq!(r.thread.latest_message_id, n.message.sms.id);
    assert!(r.thread.latest_has_otp && !r.thread.preview.contains("551203"));
    // retry with the same provider id: no new row, no notification
    let again = c.ingest_incoming(&msg, Some(42)).unwrap();
    assert!(!again.inserted && !again.notification.notify);
    assert_eq!(c.store().message_count().unwrap(), 28);
    // blocked sender: stored but silent
    c.block("AX-HDFCBK", true).unwrap();
    let quiet = c.ingest_incoming(&msg, None).unwrap();
    assert!(quiet.inserted && !quiet.notification.notify);
    assert!(
        c.ingest_incoming(
            &IncomingSms {
                sender: " ".into(),
                ..msg.clone()
            },
            None
        )
        .is_err()
    );
    assert!(
        c.ingest_incoming(
            &IncomingSms {
                body: "x".repeat(20_001),
                ..msg
            },
            None
        )
        .is_err()
    );
}

#[test]
fn send_queue_rejected_on_linux_accepted_on_phone() {
    let out = |t: i64| OutgoingSms {
        thread_id: Some(t),
        address: None,
        body: "On my way".into(),
        sim_slot: 0,
    };
    let mut linux = seeded();
    let t = linux
        .threads_page("All", None, None, Some(1))
        .unwrap()
        .threads[0]
        .thread_id;
    let err = linux.queue_outgoing(&out(t)).unwrap_err();
    assert!(err.0.contains("not available"));
    assert_eq!(linux.store().message_count().unwrap(), 27);

    let mut phone = seeded_phone();
    assert!(phone.capabilities().can_send);
    let r = phone.queue_outgoing(&out(t)).unwrap();
    assert_eq!(r.message.send_status, Some(SendStatus::Queued));
    assert_eq!(r.thread.latest_message_id, r.message.sms.id);
    let id = r.message.sms.id;
    let page = phone.messages_page(t, None, Some(1)).unwrap();
    assert_eq!(page.messages[0].sms.id, id);
    assert_eq!(page.messages[0].send_status, Some(SendStatus::Queued));
    let json = serde_json::to_value(&page.messages[0]).unwrap();
    assert_eq!(json["sendStatus"], "queued");
    assert!(json["sms"].is_object(), "flattened ParsedSms shape");
    assert!(
        phone
            .set_send_status(id, SendStatus::Delivered, Some(5))
            .unwrap()
    );
    assert_eq!(phone.provider_refs(&[id]).unwrap()[0].provider_id, 5);
    assert_eq!(
        phone.provider_mark_read(&[5]).unwrap(),
        0,
        "outgoing is already read"
    );
    assert_eq!(phone.provider_delete(&[5]).unwrap(), 1);
    assert!(phone.provider_delete(&vec![1; 20_001]).is_err());
}

#[test]
fn purge_expired_otps_follows_ts_ttl_rule() {
    let mut c = seeded();
    let before = c.store().message_count().unwrap();
    let expired = c
        .otp_items()
        .unwrap()
        .iter()
        .filter(|o| o.item.is_expired)
        .count();
    let purged = c.purge_expired_otps().unwrap();
    assert!(purged > 0 && purged <= expired);
    assert_eq!(c.store().message_count().unwrap(), before - purged as i64);
    assert!(c.otp_items().unwrap().iter().all(|o| !o.item.is_expired));
    assert_eq!(c.purge_expired_otps().unwrap(), 0);
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
    for it in &items {
        let plain = c.vault_reveal(&it.vault).unwrap();
        assert!(
            !it.item.body.contains(&plain),
            "otp_items body leaks the code"
        );
        assert!(it.item.body.contains('\u{2022}'));
    }
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

fn incoming(sender: &str, body: &str) -> IncomingSms {
    IncomingSms {
        sender: sender.into(),
        contact_name: None,
        body: body.into(),
        is_known_contact: false,
        sim_slot: 0,
        protocol: None,
        timestamp_ms: None,
    }
}

#[test]
fn provider_reconcile_deletes_missing_and_marks_read() {
    let mut c = seeded_phone();
    for (pid, body) in [(501, "hello one"), (502, "hello two"), (503, "hello three")] {
        assert!(
            c.ingest_incoming(&incoming("+15550001", body), Some(pid))
                .unwrap()
                .inserted
        );
    }
    let before = c.store().message_count().unwrap();
    // a failed/empty provider snapshot must not wipe stored provider rows
    assert!(c.provider_reconcile(&[], &[]).is_err());
    assert_eq!(c.store().message_count().unwrap(), before);
    // 502 deleted elsewhere, 503 read elsewhere
    let rep = c.provider_reconcile(&[501, 503], &[503]).unwrap();
    assert_eq!((rep.deleted, rep.marked_read), (1, 1));
    assert_eq!(c.store().message_count().unwrap(), before - 1);
    // idempotent
    let again = c.provider_reconcile(&[501, 503], &[503]).unwrap();
    assert_eq!((again.deleted, again.marked_read), (0, 0));
}

#[test]
fn platform_status_never_raises_can_send_on_desktop() {
    let mut c = seeded();
    let caps = c.apply_platform_status(true, true, 2, false);
    // the test binary is a desktop build: the radio flag stays off
    assert!(!caps.can_send && caps.read_only && caps.default_sms_role);
    assert_eq!(c.apply_platform_status(false, false, 0, false).sim_slots, 1);
}

#[test]
fn persistent_vault_key_survives_a_new_core_and_otp_code_is_plain() {
    let key = [7u8; 32];
    let mut a = seeded_phone();
    a.set_vault_key(key);
    let r = a
        .ingest_incoming(
            &incoming(
                "AX-HDFCBK",
                "Your OTP is 482913 for HDFC Bank. Do not share.",
            ),
            Some(900),
        )
        .unwrap();
    let payload = r.notification.otp_vault.clone().unwrap();
    assert_eq!(
        a.otp_code(r.notification.message.sms.id)
            .unwrap()
            .as_deref(),
        Some("482913")
    );
    // a second core (e.g. after a restart) with the same Keystore-held DEK
    let mut b = seeded_phone();
    assert!(b.vault_reveal(&payload).is_err());
    b.set_vault_key(key);
    assert_eq!(b.vault_reveal(&payload).unwrap(), "482913");
    assert_eq!(a.otp_code(1).unwrap(), None);
}

// ---- backup restore through the core (S14) ------------------------------

/// Shared test passphrase (not a secret): same constant as the sms-crypto
/// tests and `scripts/backup-xcheck.ts`.
const BACKUP_TEST_PASS: &str = "my-secret-pass-2026";

fn ts_fixture() -> (String, usize) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../crates/sms-crypto/tests/fixtures/ts_backup.mysms.enc"
    ))
    .unwrap();
    let env: serde_json::Value = serde_json::from_str(&text).unwrap();
    let n = env["messageCount"].as_u64().unwrap() as usize;
    (text, n)
}

#[test]
fn ts_written_backup_restores_through_core_and_is_idempotent() {
    let (text, n) = ts_fixture();
    assert!(n > 0);
    let mut c = Core::new(Store::open_in_memory(0).unwrap());
    let first = c.backup_restore(&text, BACKUP_TEST_PASS).unwrap();
    assert_eq!(first.ingest.inserted, n);
    assert_eq!(first.ingest.skipped_existing, 0);
    assert_eq!(c.store().message_count().unwrap(), n as i64);

    let second = c.backup_restore(&text, BACKUP_TEST_PASS).unwrap();
    assert_eq!(second.ingest.inserted, 0);
    assert_eq!(second.ingest.skipped_existing, n);
    assert_eq!(c.store().message_count().unwrap(), n as i64);
}

#[test]
fn wrong_passphrase_is_rejected_through_core_without_touching_the_store() {
    let (text, _) = ts_fixture();
    let mut c = Core::new(Store::open_in_memory(0).unwrap());
    for bad in ["wrong-passphrase", "my-secret-pass-2027"] {
        let err = c.backup_restore(&text, bad).unwrap_err();
        assert!(
            err.to_string().contains("Incorrect passphrase"),
            "unexpected error: {err}"
        );
        assert!(!err.to_string().contains(bad));
    }
    let empty = c.backup_restore(&text, "  ").unwrap_err();
    assert!(empty.to_string().contains("passphrase"));
    assert_eq!(c.store().message_count().unwrap(), 0);
}

#[test]
fn rust_written_backup_round_trips_through_core_and_is_ciphertext() {
    let a = seeded();
    let f = a.backup_create(BACKUP_TEST_PASS).unwrap();
    assert!(f.filename.ends_with(".mysms.enc"));
    // Random base64 can contain any 3 letters, so probe with whole bodies.
    for m in a.store().all_messages().unwrap().iter().take(20) {
        assert!(
            !f.content.contains(&m.body),
            "plaintext leaked into the envelope"
        );
    }
    assert!(f.content.contains("mySMS-encrypted-backup-v1"));
    let mut b = Core::new(Store::open_in_memory(0).unwrap());
    let r = b.backup_restore(&f.content, BACKUP_TEST_PASS).unwrap();
    assert_eq!(r.ingest.inserted, f.message_count);
}
