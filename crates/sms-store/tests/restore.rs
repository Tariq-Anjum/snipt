//! R2 #2: backup restore is an atomic, validated merge; an id collision with a
//! different message is remapped, never treated as "already present".

use sms_store::ids::MAX_ID;
use sms_store::{IncomingSms, RestoreInput, Store};
use sms_types::RawSms;

const NOW: i64 = 1_760_000_000_000;

fn raw(id: i64, thread_id: i64, sender: &str, body: &str, ts: i64) -> RawSms {
    RawSms {
        id,
        thread_id,
        sender: sender.into(),
        contact_name: None,
        body: body.into(),
        timestamp_ms: ts,
        is_known_contact: false,
        is_outgoing: false,
        sim_slot: 0,
        is_read: Some(false),
        is_starred: Some(false),
        protocol: None,
        rcs_status: None,
        rcs_rich_card: None,
    }
}

fn input<'a>(
    messages: &'a [RawSms],
    archived: &'a [i64],
    blocked: &'a [String],
    reminders: &'a [i64],
) -> RestoreInput<'a> {
    RestoreInput {
        messages,
        archived_thread_ids: archived,
        blocked_senders: blocked,
        completed_reminder_ids: reminders,
        settings: &[],
    }
}

fn local_message(s: &mut Store) -> RawSms {
    s.ingest_incoming(
        &IncomingSms {
            sender: "+15550001".into(),
            contact_name: None,
            body: "local message".into(),
            is_known_contact: true,
            sim_slot: 0,
            protocol: None,
            timestamp_ms: Some(NOW),
        },
        NOW,
    )
    .unwrap()
}

#[test]
fn restore_into_empty_store_keeps_every_id() {
    let seed = sms_testdata::create_seed_messages(NOW);
    let mut s = Store::open_in_memory(330).unwrap();
    let o = s.restore_merge(&input(&seed, &[], &[], &[])).unwrap();
    assert_eq!(o.ingest.inserted, seed.len());
    assert_eq!((o.remapped_messages, o.remapped_threads), (0, 0));
    for m in &seed {
        let got = s.message(m.id).unwrap().expect("same id");
        assert_eq!(got.sms.thread_id, m.thread_id);
        assert_eq!(got.sms.body, m.body);
    }
}

#[test]
fn id_collision_with_a_different_message_is_remapped_not_dropped() {
    let mut s = Store::open_in_memory(330).unwrap();
    let local = local_message(&mut s);
    // An unrelated history that allocated the same message and thread ids.
    let incoming = vec![
        raw(
            local.id,
            local.thread_id,
            "VM-OTHER",
            "Your bill of Rs 499 is due on 5th",
            NOW - 5,
        ),
        raw(
            local.id + 1,
            local.thread_id,
            "VM-OTHER",
            "Second message",
            NOW - 4,
        ),
    ];
    let o = s
        .restore_merge(&input(&incoming, &[local.thread_id], &[], &[local.id]))
        .unwrap();
    assert_eq!(o.ingest.inserted, 2);
    assert_eq!(o.ingest.skipped_existing, 0);
    assert_eq!(o.remapped_messages, 1);
    assert_eq!(o.remapped_threads, 1);
    assert_eq!(s.message_count().unwrap(), 3);
    // The local message and its thread are untouched (not archived, no reminder flag).
    let kept = s.message(local.id).unwrap().unwrap();
    assert_eq!(kept.sms.body, "local message");
    assert!(!s.thread(local.thread_id).unwrap().unwrap().is_archived);
    // The archived flag followed the remapped incoming thread.
    let other = s
        .all_threads()
        .unwrap()
        .into_iter()
        .find(|t| t.sender == "VM-OTHER")
        .unwrap();
    assert_ne!(other.thread_id, local.thread_id);
    assert!(other.is_archived);
    assert_eq!(other.message_count, 2);
    // Re-restoring the same archive is idempotent (identity match).
    let again = s
        .restore_merge(&input(&incoming, &[local.thread_id], &[], &[local.id]))
        .unwrap();
    assert_eq!(
        (again.ingest.inserted, again.ingest.skipped_existing),
        (0, 2)
    );
    assert_eq!(s.message_count().unwrap(), 3);
}

#[test]
fn same_message_counts_as_present_under_any_id() {
    let mut s = Store::open_in_memory(330).unwrap();
    let local = local_message(&mut s);
    let same = vec![
        RawSms {
            is_known_contact: true,
            ..raw(local.id, local.thread_id, "+15550001", "local message", NOW)
        },
        raw(local.id + 500, 900, "+15550001", "local message", NOW),
    ];
    // R4 #1: presence is a multiset match. Two identical archive rows
    // against one stored row: one matches, the other is a distinct message.
    let o = s.restore_merge(&input(&same, &[], &[], &[])).unwrap();
    assert_eq!((o.ingest.inserted, o.ingest.skipped_existing), (1, 1));
    assert_eq!(s.message_count().unwrap(), 2);
    // Re-restoring is idempotent: both now match.
    let again = s.restore_merge(&input(&same, &[], &[], &[])).unwrap();
    assert_eq!(
        (again.ingest.inserted, again.ingest.skipped_existing),
        (0, 2)
    );
    assert_eq!(s.message_count().unwrap(), 2);
}

#[test]
fn invalid_archive_changes_nothing() {
    let mut s = Store::open_in_memory(330).unwrap();
    local_message(&mut s);
    let before = s.message_count().unwrap();
    let blocked = vec!["VM-SPAM".to_string()];
    let cases: Vec<(Vec<RawSms>, Vec<i64>, Vec<i64>)> = vec![
        (
            vec![raw(5, 7, "A", "ok", NOW), raw(i64::MAX, 7, "A", "bad", NOW)],
            vec![],
            vec![],
        ),
        (
            vec![
                raw(5, 7, "A", "ok", NOW),
                raw(MAX_ID + 1, 7, "A", "bad", NOW),
            ],
            vec![],
            vec![],
        ),
        (
            vec![raw(5, 7, "A", "ok", NOW), raw(6, -1, "A", "bad", NOW)],
            vec![],
            vec![],
        ),
        (
            vec![raw(5, 7, "A", "a", NOW), raw(5, 7, "A", "dup id", NOW)],
            vec![],
            vec![],
        ),
        (vec![raw(5, 7, "A", "ok", NOW)], vec![i64::MAX], vec![]),
        (vec![raw(5, 7, "A", "ok", NOW)], vec![], vec![0]),
    ];
    for (msgs, archived, reminders) in cases {
        assert!(
            s.restore_merge(&input(&msgs, &archived, &blocked, &reminders))
                .is_err()
        );
        assert_eq!(s.message_count().unwrap(), before);
        assert!(s.blocked_senders().unwrap().is_empty(), "partial write");
    }
}
