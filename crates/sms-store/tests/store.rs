//! sms-store parity with the TS store semantics (threads.json / budget.json
//! fixtures) and the store parts of TS test 8.

mod common;

use common::*;
use sms_engine::inbox::InboxFilter;
use sms_store::{IncomingSms, Store};
use sms_types::*;

fn seeded(state: &str) -> (Store, i64, Vec<RawSms>) {
    let (now, seed) = seed();
    let mut s = Store::open_in_memory(TZ).unwrap();
    s.ingest_batch(&seed).unwrap();
    let fx = fixture("threads.json");
    let st = &fx["states"][state];
    for t in st["archivedThreadIds"].as_array().unwrap() {
        s.set_archived(t.as_i64().unwrap(), true).unwrap();
    }
    for b in st["blockedSenders"].as_array().unwrap() {
        s.set_blocked(b.as_str().unwrap(), true).unwrap();
    }
    for id in st["completedReminderSmsIds"].as_array().unwrap() {
        s.set_reminder_done(id.as_i64().unwrap(), true).unwrap();
    }
    (s, now, seed)
}

#[test]
fn fts5_trigram_available_and_latest_schema() {
    let s = Store::open_in_memory(TZ).unwrap();
    assert_eq!(
        s.schema_version().unwrap(),
        sms_store::migrate::LATEST_VERSION
    );
    let sql: String = s
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'messages_fts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("fts5") && sql.contains("trigram"));
    // proves bundled FTS5 trigram works
    s.connection()
        .execute_batch("CREATE VIRTUAL TABLE t USING fts5(x, tokenize='trigram'); INSERT INTO t VALUES ('SWIGGY INSTAMART');")
        .unwrap();
    let n: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM t WHERE t MATCH '\"instam\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn file_db_reopens_with_wal_and_migrations_idempotent() {
    let dir = std::env::temp_dir().join(format!("mysms-store-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.db");
    let (_, seed) = seed();
    {
        let mut s = Store::open(&path, TZ).unwrap();
        s.ingest_batch(&seed).unwrap();
        let mode: String = s
            .connection()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
    let mut s = Store::open(&path, TZ).unwrap();
    assert_eq!(s.message_count().unwrap(), seed.len() as i64);
    let rep = s.ingest_batch(&seed).unwrap();
    assert_eq!(rep.inserted, 0);
    assert_eq!(rep.skipped_existing, seed.len());
    drop(s);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn threads_table_equals_fixture() {
    let fx = fixture("threads.json");
    for state in ["base", "blocked"] {
        let (s, _, _) = seeded(state);
        let rows = s.all_threads().unwrap();
        let want = fx["threads"][state].as_array().unwrap();
        assert_eq!(rows.len(), want.len(), "{state}");
        for (r, w) in rows.iter().zip(want) {
            let ctx = format!("{state} thread {}", r.thread_id);
            assert_eq!(r.thread_id, w["threadId"].as_i64().unwrap(), "{ctx}");
            assert_eq!(r.sender, w["sender"].as_str().unwrap(), "{ctx}");
            assert_eq!(r.display_name, w["displayName"].as_str().unwrap(), "{ctx}");
            assert_eq!(
                serde_json::to_value(&r.sender_kind).unwrap(),
                w["senderKind"],
                "{ctx}"
            );
            assert_eq!(
                r.latest_message_id,
                w["latestMessageId"].as_i64().unwrap(),
                "{ctx}"
            );
            assert_eq!(
                r.message_count as usize,
                w["messageIds"].as_array().unwrap().len(),
                "{ctx}"
            );
            json_close(
                &serde_json::to_value(&r.classification).unwrap(),
                &w["classification"],
                &ctx,
            )
            .unwrap();
            assert_eq!(r.unread_count, w["unreadCount"].as_i64().unwrap(), "{ctx}");
            assert_eq!(r.has_starred, w["hasStarred"].as_bool().unwrap(), "{ctx}");
            assert_eq!(r.is_archived, w["isArchived"].as_bool().unwrap(), "{ctx}");
            assert_eq!(r.is_blocked, w["isBlocked"].as_bool().unwrap(), "{ctx}");
            assert_eq!(
                i64::from(r.sim_slot),
                w["simSlot"].as_i64().unwrap(),
                "{ctx}"
            );
            assert_eq!(
                r.is_rcs_capable,
                w["isRcsCapable"].as_bool().unwrap(),
                "{ctx}"
            );
        }
        // Messages of each thread come back newest-first and equal the TS order reversed.
        for w in want {
            let tid = w["threadId"].as_i64().unwrap();
            let (msgs, next) = s.messages_page(tid, None, 500).unwrap();
            assert!(next.is_none());
            let mut ids: Vec<i64> = msgs.iter().map(|m| m.sms.id).collect();
            ids.reverse();
            let want_ids: Vec<i64> = serde_json::from_value(w["messageIds"].clone()).unwrap();
            assert_eq!(ids, want_ids, "{state} thread {tid} message order");
        }
    }
}

#[test]
fn search_and_filters_equal_fixture() {
    let fx = fixture("threads.json");
    let (base, _, _) = seeded("base");
    let (blocked, _, _) = seeded("blocked");
    let cases = fx["filters"].as_array().unwrap();
    for c in cases {
        let s = if c["state"] == "base" {
            &base
        } else {
            &blocked
        };
        let filter = InboxFilter::parse(c["filter"].as_str().unwrap()).unwrap();
        let sim = c["sim"].as_u64().map(|v| v as u8);
        let q = c["query"].as_str().unwrap();
        let got: Vec<i64> = s
            .search(filter, sim, q, 1000)
            .unwrap()
            .iter()
            .map(|t| t.thread_id)
            .collect();
        let want: Vec<i64> = serde_json::from_value(c["threadIds"].clone()).unwrap();
        assert_eq!(got, want, "{c}");
        if q.trim().is_empty() {
            let page: Vec<i64> = s
                .threads_page(filter, sim, None, 1000)
                .unwrap()
                .threads
                .iter()
                .map(|t| t.thread_id)
                .collect();
            assert_eq!(page, want, "page {c}");
        }
    }
}

#[test]
fn otp_items_and_reminders_equal_fixture() {
    let fx = fixture("threads.json");
    for state in ["base", "blocked"] {
        let (s, now, _) = seeded(state);
        let otps: Vec<serde_json::Value> = s
            .otp_items(now)
            .unwrap()
            .iter()
            .map(|o| {
                serde_json::json!({"smsId": o.sms_id, "threadId": o.thread_id, "sender": o.sender,
                    "expiresAtMs": o.expires_at_ms, "isExpired": o.is_expired, "code": o.otp.code})
            })
            .collect();
        assert_eq!(
            serde_json::Value::Array(otps),
            fx["otpItems"][state],
            "{state}"
        );
        let rems: Vec<serde_json::Value> = s
            .reminders()
            .unwrap()
            .iter()
            .map(|r| {
                serde_json::json!({"smsId": r.sms_id, "threadId": r.thread_id, "sender": r.sender,
                    "isDone": r.is_done, "kind": r.reminder.kind})
            })
            .collect();
        assert_eq!(
            serde_json::Value::Array(rems),
            fx["reminders"][state],
            "{state}"
        );
    }
}

#[test]
fn finance_and_budget_equal_fixture() {
    let fx = fixture("budget.json");
    let finances = fx["finances"].as_array().unwrap();
    let mut sums = Vec::new();
    for (i, f) in finances.iter().enumerate() {
        let (s, now, _) = seeded(f["state"].as_str().unwrap());
        let cfg: PaymentCycleConfig = serde_json::from_value(f["cycleConfig"].clone()).unwrap();
        let sum = s
            .finance_summary(&cfg, now, f["onlyActiveCycle"].as_bool().unwrap())
            .unwrap();
        json_close(
            &serde_json::to_value(&sum).unwrap(),
            &f["summary"],
            &format!("finances[{i}]"),
        )
        .unwrap();
        sums.push((s, cfg, now));
    }
    for (i, b) in fx["budgets"].as_array().unwrap().iter().enumerate() {
        let (s, cfg, now) = &sums[b["financeIndex"].as_u64().unwrap() as usize];
        let limits: BudgetLimits = serde_json::from_value(b["limits"].clone()).unwrap();
        let only = b["onlyActiveCycle"].as_bool().unwrap();
        // budget.json computes stats over the finance summary of its finance case
        let f = &finances[b["financeIndex"].as_u64().unwrap() as usize];
        let fin = s
            .finance_summary(cfg, *now, f["onlyActiveCycle"].as_bool().unwrap())
            .unwrap();
        let stats = sms_engine::budget::compute_budget_stats(&fin.transactions, &limits, only);
        json_close(
            &serde_json::to_value(&stats).unwrap(),
            &b["stats"],
            &format!("budgets[{i}]"),
        )
        .unwrap();
    }
}

/// Store parts of TS test 8: RCS filter, OTP purge, ingest; plus test 5 delete.
#[test]
fn rcs_filter_and_purge() {
    let fx = fixture("threads.json");
    let (mut s, now, seed) = seeded("base");
    assert_eq!(
        s.threads_page(InboxFilter::Rcs, None, None, 50)
            .unwrap()
            .threads
            .len(),
        4
    );
    assert!(s.otp_items(now).unwrap().iter().any(|o| o.is_expired));

    let purged = s.purge_expired_otps(now).unwrap();
    assert_eq!(purged.len(), 2);
    let want: Vec<i64> = serde_json::from_value(fx["purge"]["purgedIds"].clone()).unwrap();
    assert_eq!(purged, want);
    assert_eq!(s.message_count().unwrap() as usize, seed.len() - 2);

    let created = s
        .ingest_incoming(
            &IncomingSms {
                sender: " VM-AIRTEL-T ".into(),
                contact_name: None,
                body: " 492011 is your Airtel OTP. ".into(),
                is_known_contact: false,
                sim_slot: 0,
                protocol: None,
                timestamp_ms: Some(now + 1000),
            },
            now,
        )
        .unwrap();
    assert_eq!(created.sender, "VM-AIRTEL-T");
    let want_a: RawSms = serde_json::from_value(fx["ingest"][0].clone()).unwrap();
    assert_eq!(created, want_a);
    let t = s.thread(created.thread_id).unwrap().unwrap();
    assert_eq!(t.classification.category, Category::Priority);
    assert_eq!(t.unread_count, 1);
}

#[test]
fn ingest_into_existing_thread_unarchives() {
    let fx = fixture("threads.json");
    let (mut s, now, _) = seeded("base");
    s.set_archived(101, true).unwrap();
    let created = s
        .ingest_incoming(
            &IncomingSms {
                sender: "ax-hdfcbk-s".into(),
                contact_name: None,
                body: "Sent Rs.10.00 via UPI".into(),
                is_known_contact: false,
                sim_slot: 1,
                protocol: None,
                timestamp_ms: Some(now + 2000),
            },
            now,
        )
        .unwrap();
    let want: RawSms = serde_json::from_value(fx["ingest"][1].clone()).unwrap();
    assert_eq!(created, want);
    let t = s.thread(101).unwrap().unwrap();
    assert!(!t.is_archived);
    assert_eq!(t.latest_message_id, created.id);
}

#[test]
fn delete_old_promotions_and_fts_consistency() {
    let (mut s, now, seed) = seeded("base");
    let report = s.weekly_scan(now).unwrap();
    let promos = report
        .buckets
        .iter()
        .find(|b| b.id == sms_engine::cleanup::CleanupBucketId::OldPromotions)
        .unwrap()
        .message_ids
        .clone();
    assert_eq!(promos.len(), 3);
    assert_eq!(s.delete_messages(&promos).unwrap(), 3);
    assert_eq!(s.message_count().unwrap() as usize, seed.len() - 3);
    // thread 110 (Myntra) had only promos → thread row removed
    assert!(s.thread(110).unwrap().is_none());
    // FTS no longer returns deleted rows
    let n: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH '\"myntra\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    assert!(
        s.search(InboxFilter::All, None, "myntra", 50)
            .unwrap()
            .is_empty()
    );
    let fts_rows: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM messages_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fts_rows, s.message_count().unwrap());
}

#[test]
fn mark_read_star_block_update_threads() {
    let (mut s, _, _) = seeded("base");
    let before = s.thread(101).unwrap().unwrap().unread_count;
    assert!(s.mark_read(1003).unwrap());
    assert_eq!(s.thread(101).unwrap().unwrap().unread_count, before - 1);
    assert!(s.set_starred(1003, true).unwrap());
    assert!(s.thread(101).unwrap().unwrap().has_starred);
    s.set_blocked("AX-HDFCBK-S", true).unwrap();
    let t = s.thread(101).unwrap().unwrap();
    assert!(t.is_blocked);
    assert_eq!(t.classification.category, Category::Spam);
    s.set_blocked("ax-hdfcbk-s", false).unwrap();
    assert!(!s.thread(101).unwrap().unwrap().is_blocked);
    assert!(!s.mark_read(999_999).unwrap());
}

#[test]
fn rejects_invalid_sim_slot() {
    let (_, mut seed) = seed();
    seed[0].sim_slot = 7;
    let mut s = Store::open_in_memory(TZ).unwrap();
    assert!(s.ingest_batch(&seed[..1]).is_err());
    assert_eq!(s.message_count().unwrap(), 0);
}

/// R1 blocker: timestamps are validated at the ingest boundary.
#[test]
fn rejects_out_of_range_timestamps() {
    let (now, mut seed) = seed();
    let mut s = Store::open_in_memory(TZ).unwrap();
    for ts in [i64::MIN, i64::MAX, time::MAX_TIMESTAMP_MS + 1] {
        seed[0].timestamp_ms = ts;
        assert!(s.ingest_batch(&seed[..1]).is_err(), "ts={ts}");
        let incoming = IncomingSms {
            sender: "VM-TEST-S".into(),
            contact_name: None,
            body: "hello".into(),
            is_known_contact: false,
            sim_slot: 0,
            protocol: None,
            timestamp_ms: Some(ts),
        };
        assert!(s.ingest_incoming(&incoming, now).is_err(), "ts={ts}");
    }
    assert_eq!(s.message_count().unwrap(), 0);
    seed[0].timestamp_ms = time::MAX_TIMESTAMP_MS;
    assert_eq!(s.ingest_batch(&seed[..1]).unwrap().inserted, 1);
}

/// R1 blocker: rows that bypassed validation (legacy DB) must not wrap the
/// OTP expiry into the past.
#[test]
fn otp_expiry_saturates_for_extreme_stored_timestamps() {
    let (now, seed) = seed();
    let mut s = Store::open_in_memory(TZ).unwrap();
    s.ingest_batch(&seed).unwrap();
    let otp_ids: Vec<i64> = s.otp_items(now).unwrap().iter().map(|o| o.sms_id).collect();
    assert!(!otp_ids.is_empty());
    let id = otp_ids[0];
    s.connection()
        .execute(
            "UPDATE messages SET ts_ms = ?2 WHERE id = ?1",
            [id, i64::MAX],
        )
        .unwrap();
    let item = s
        .otp_items(now)
        .unwrap()
        .into_iter()
        .find(|o| o.sms_id == id)
        .unwrap();
    assert_eq!(item.expires_at_ms, i64::MAX);
    assert!(!item.is_expired);
    let purged = s.purge_expired_otps(now).unwrap();
    assert!(!purged.contains(&id));
    let scan = s.weekly_scan(now).unwrap();
    assert!(!scan.buckets.iter().any(|b| b.message_ids.contains(&id)));
}

/// R1 optional: tokens whose SQLite trigram folding differs from JS
/// `toLowerCase()` (İ, final sigma) or that FTS5 cannot parse (NUL) must still
/// match, via the exact scan.
#[test]
fn search_tokens_fts_cannot_represent() {
    let (now, _) = seed();
    let mut s = Store::open_in_memory(TZ).unwrap();
    for (sender, body) in [
        ("VM-TRAVEL-S", "Flight to İSTANBUL confirmed for 24 Nov."),
        ("VM-GREEK-S", "ΟΔΟΣ ΑΘΗΝΑΣ 12 delivery today."),
        ("VM-NULLS-S", "code\0word alpha"),
        ("VM-HINDI-S", "आपका बिल भुगतान सफल रहा"),
    ] {
        let m = IncomingSms {
            sender: sender.into(),
            contact_name: None,
            body: body.into(),
            is_known_contact: false,
            sim_slot: 0,
            protocol: None,
            timestamp_ms: Some(now),
        };
        s.ingest_incoming(&m, now).unwrap();
    }
    let senders = |q: &str| -> Vec<String> {
        s.search(InboxFilter::All, None, q, 50)
            .unwrap()
            .into_iter()
            .map(|t| t.sender)
            .collect()
    };
    assert_eq!(senders("İSTANBUL"), ["VM-TRAVEL-S"]);
    assert_eq!(senders("i\u{307}stanbul"), ["VM-TRAVEL-S"]);
    assert_eq!(senders("stanbul"), ["VM-TRAVEL-S"]);
    assert_eq!(senders("ΟΔΟΣ"), ["VM-GREEK-S"]);
    assert_eq!(senders("e\0w"), ["VM-NULLS-S"]);
    assert_eq!(senders("code\0word"), ["VM-NULLS-S"]);
    assert_eq!(senders("भुगतान"), ["VM-HINDI-S"]);
    assert!(senders("istanbul").is_empty()); // JS: "i̇stanbul" lacks "ist"
}
