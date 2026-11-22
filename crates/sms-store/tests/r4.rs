//! R4 regressions (Sol-6.2 re-review): restore multiplicity and thread
//! mapping, id-space exhaustion and retired-id reuse, v4 migration seeding /
//! repair / legacy in-flight sends, hand-off serialization, tombstone
//! snapshot and the explicit recovery path.

use sms_store::ids::{MAX_EXTERNAL_ID, MAX_ID};
use sms_store::{IncomingSms, OutgoingSms, PartKind, PartOutcome, RestoreInput, SendStatus, Store};
use sms_types::{MessageProtocol, RawSms};

const NOW: i64 = 1_760_000_000_000;

fn raw(id: i64, thread_id: i64, sender: &str, body: &str) -> RawSms {
    RawSms {
        id,
        thread_id,
        sender: sender.into(),
        contact_name: None,
        body: body.into(),
        timestamp_ms: NOW,
        is_known_contact: false,
        is_outgoing: false,
        sim_slot: 0,
        is_read: Some(true),
        is_starred: Some(false),
        protocol: None,
        rcs_status: None,
        rcs_rich_card: None,
    }
}

fn restore(
    s: &mut Store,
    msgs: &[RawSms],
    archived: &[i64],
) -> sms_store::Result<sms_store::RestoreOutcome> {
    s.restore_merge(&RestoreInput {
        messages: msgs,
        archived_thread_ids: archived,
        blocked_senders: &[],
        completed_reminder_ids: &[],
        settings: &[],
    })
}

fn incoming(sender: &str, body: &str) -> IncomingSms {
    IncomingSms {
        sender: sender.into(),
        contact_name: None,
        body: body.into(),
        is_known_contact: false,
        sim_slot: 0,
        protocol: None,
        timestamp_ms: Some(NOW),
    }
}

fn next_mark(s: &Store, name: &str) -> Option<i64> {
    s.connection()
        .query_row("SELECT next FROM id_alloc WHERE name = ?1", [name], |r| {
            r.get(0)
        })
        .ok()
}

// ---- #1 multiplicity and thread mapping -----------------------------------

#[test]
fn rows_differing_by_sim_or_transport_are_distinct_even_on_empty_restore() {
    let mut s = Store::open_in_memory(330).unwrap();
    let msgs = vec![
        raw(1, 10, "VM-BANK", "Rs 100 debited"),
        RawSms {
            sim_slot: 1,
            ..raw(2, 10, "VM-BANK", "Rs 100 debited")
        },
        RawSms {
            protocol: Some(MessageProtocol::Rcs),
            ..raw(3, 10, "VM-BANK", "Rs 100 debited")
        },
        // A true twin (same everything, another id) is a second message too.
        raw(4, 10, "VM-BANK", "Rs 100 debited"),
    ];
    let o = restore(&mut s, &msgs, &[]).unwrap();
    assert_eq!((o.ingest.inserted, o.ingest.skipped_existing), (4, 0));
    assert_eq!(s.message_count().unwrap(), 4);
    let again = restore(&mut s, &msgs, &[]).unwrap();
    assert_eq!(
        (again.ingest.inserted, again.ingest.skipped_existing),
        (0, 4)
    );
}

#[test]
fn thread_mapping_follows_the_matched_rows() {
    let mut s = Store::open_in_memory(330).unwrap();
    // The same sender in two local threads (7 first, then 8).
    s.ingest_batch(&[raw(1, 7, "VM-X", "first"), raw(2, 8, "VM-X", "second")])
        .unwrap();
    // Archive thread 50 holds the message that lives in local thread 8.
    let o = restore(&mut s, &[raw(90, 50, "VM-X", "second")], &[50]).unwrap();
    assert_eq!(o.ingest.skipped_existing, 1);
    assert!(
        s.thread(8).unwrap().unwrap().is_archived,
        "flag followed the match"
    );
    assert!(
        !s.thread(7).unwrap().unwrap().is_archived,
        "flag hit another thread"
    );
    // Matches in two local threads: ambiguous, nothing is written.
    let before = s.message_count().unwrap();
    let r = restore(
        &mut s,
        &[
            raw(91, 60, "VM-X", "first"),
            raw(92, 60, "VM-X", "second"),
            raw(93, 60, "VM-X", "new"),
        ],
        &[60],
    );
    assert!(r.is_err());
    assert_eq!(s.message_count().unwrap(), before);
    assert!(!s.thread(7).unwrap().unwrap().is_archived);
}

// ---- #2 / #3 id space -------------------------------------------------------

#[test]
fn a_max_id_archive_cannot_stop_ingest_or_sends() {
    let mut s = Store::open_in_memory(330).unwrap();
    let o = restore(&mut s, &[raw(MAX_ID, MAX_ID, "VM-A", "edge")], &[MAX_ID]).unwrap();
    assert_eq!((o.remapped_messages, o.remapped_threads), (1, 1));
    assert!(next_mark(&s, "message").unwrap() <= MAX_EXTERNAL_ID + 1);
    assert!(next_mark(&s, "thread").unwrap() <= MAX_EXTERNAL_ID + 1);
    let t = s.all_threads().unwrap();
    assert_eq!(t.len(), 1);
    assert!(t[0].is_archived, "archive flag followed the remap");
    s.ingest_incoming(&incoming("+1555", "after"), NOW).unwrap();
    let out = OutgoingSms {
        thread_id: None,
        address: Some("+1666".into()),
        body: "new conversation".into(),
        sim_slot: 0,
        reply_to: None,
    };
    s.queue_outgoing(&out, NOW).unwrap();
}

#[test]
fn restore_never_reclaims_a_retired_message_or_thread_id() {
    let mut s = Store::open_in_memory(330).unwrap();
    let a = s.ingest_incoming(&incoming("+1555", "a"), NOW).unwrap();
    let b = s.ingest_incoming(&incoming("+1666", "b"), NOW).unwrap();
    s.delete_messages(&[b.id]).unwrap(); // b.id and b.thread_id are retired
    let o = restore(&mut s, &[raw(b.id, b.thread_id, "+1777", "other")], &[]).unwrap();
    assert_eq!((o.remapped_messages, o.remapped_threads), (1, 1));
    assert!(s.message(b.id).unwrap().is_none(), "retired id reissued");
    assert!(
        s.thread(b.thread_id).unwrap().is_none(),
        "retired thread reissued"
    );
    assert!(s.message(a.id).unwrap().is_some());
    // Ids above the mark are kept (an archive restored into a used store).
    let hi = next_mark(&s, "message").unwrap() + 10;
    let th = next_mark(&s, "thread").unwrap() + 10;
    let o = restore(&mut s, &[raw(hi, th, "+1888", "fresh")], &[]).unwrap();
    assert_eq!((o.remapped_messages, o.remapped_threads), (0, 0));
    assert!(s.message(hi).unwrap().is_some());
}

// ---- v4 migration -----------------------------------------------------------

fn v1_db(
    dir: &std::path::Path,
    rows: &[(i64, i64, &str, bool, Option<&str>)],
) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("v1.db");
    let _ = std::fs::remove_file(&path);
    let mut c = rusqlite::Connection::open(&path).unwrap();
    let tx = c.transaction().unwrap();
    tx.execute_batch(sms_store::schema::SCHEMA_V1).unwrap();
    for (seq, (id, thread, sender, out, status)) in rows.iter().enumerate() {
        tx.execute(
            "INSERT INTO messages (id, ins_seq, thread_id, sender, sender_lc, body, ts_ms,
                is_known_contact, is_outgoing, sim_slot, is_read, category, confidence, reason,
                scores_json, sender_kind_json, send_status)
             VALUES (?1, ?2, ?3, ?4, lower(?4), 'legacy body', ?5, 0, ?6, 0, 1, 'Personal', 1.0,
                'Contact', '[0,0,0,0,0]', '{\"type\":\"PhoneNumber\"}', ?7)",
            rusqlite::params![id, seq as i64 + 1, thread, sender, NOW, out, status],
        )
        .unwrap();
    }
    tx.execute(
        "INSERT INTO reminders_done(msg_id) SELECT MAX(id) FROM messages",
        [],
    )
    .unwrap();
    tx.pragma_update(None, "user_version", 1).unwrap();
    tx.commit().unwrap();
    path
}

#[test]
fn v1_upgrade_seeds_id_marks_so_deleted_legacy_maxima_are_not_reissued() {
    let dir = std::env::temp_dir().join(format!("snipt-r4-seed-{}", std::process::id()));
    let path = v1_db(
        &dir,
        &[
            (1001, 101, "+1555", false, None),
            (1002, 102, "+1666", false, None),
        ],
    );
    let mut s = Store::open(&path, 330).unwrap();
    assert_eq!(
        s.schema_version().unwrap(),
        sms_store::migrate::LATEST_VERSION
    );
    s.delete_messages(&[1002]).unwrap(); // newest message and thread gone
    let n = s.ingest_incoming(&incoming("+1999", "new"), NOW).unwrap();
    assert!(n.id > 1002, "message id {} reissued", n.id);
    assert!(n.thread_id > 102, "thread id {} reissued", n.thread_id);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn v1_upgrade_renumbers_a_saturated_store_and_parks_in_flight_sends() {
    let dir = std::env::temp_dir().join(format!("snipt-r4-sat-{}", std::process::id()));
    let path = v1_db(
        &dir,
        &[
            (1001, 101, "+1555", true, Some("sending")),
            (1002, 101, "+1555", true, Some("sent")),
            (MAX_ID, MAX_ID, "+1666", false, None),
        ],
    );
    let mut s = Store::open(&path, 330).unwrap();
    assert_eq!(
        s.send_statuses(&[1001, 1002]).unwrap(),
        vec![(1001, SendStatus::Indeterminate), (1002, SendStatus::Sent)]
    );
    assert!(s.message(MAX_ID).unwrap().is_none(), "saturated id kept");
    assert_eq!(s.message_count().unwrap(), 3);
    let moved = s
        .all_threads()
        .unwrap()
        .into_iter()
        .find(|t| t.sender == "+1666")
        .unwrap();
    assert!(moved.thread_id <= MAX_EXTERNAL_ID);
    // References followed the renumbered message (reminder flag, search).
    let mid = s.messages_page(moved.thread_id, None, 5).unwrap().0[0]
        .sms
        .id;
    let done: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM reminders_done WHERE msg_id = ?1",
            [mid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(done, 1);
    assert!(next_mark(&s, "message").unwrap() <= MAX_EXTERNAL_ID + 1);
    let fk: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(fk, 0);
    // Allocation works again.
    s.ingest_incoming(&incoming("+1999", "new"), NOW).unwrap();
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- #4 hand-off ------------------------------------------------------------

fn queued(s: &mut Store) -> i64 {
    s.queue_outgoing(
        &OutgoingSms {
            thread_id: None,
            address: Some("+1555".into()),
            body: "hi".into(),
            sim_slot: 0,
            reply_to: None,
        },
        NOW,
    )
    .unwrap()
    .id
}

#[test]
fn delete_cannot_cascade_an_attempt_during_hand_off() {
    let mut s = Store::open_in_memory(330).unwrap();
    let id = queued(&mut s);
    let a = s.begin_send_attempt(id, 1, NOW).unwrap();
    assert_eq!(s.handoff_open(&[id]).unwrap(), vec![id]);
    assert_eq!(s.delete_messages(&[id]).unwrap(), 0, "deleted mid hand-off");
    assert!(s.bind_send_provider(a, 40).unwrap(), "attempt survived");
    let u = s.end_send_handoff(a, true).unwrap().unwrap();
    assert_eq!(u.status, SendStatus::Sending);
    assert_eq!(s.delete_messages(&[id]).unwrap(), 1);
    // A refused hand-off ends failed.
    let id2 = queued(&mut s);
    let a2 = s.begin_send_attempt(id2, 1, NOW).unwrap();
    assert_eq!(
        s.end_send_handoff(a2, false).unwrap().unwrap().status,
        SendStatus::Failed
    );
}

#[test]
fn an_interrupted_hand_off_becomes_indeterminate_and_only_callbacks_move_it() {
    let mut s = Store::open_in_memory(330).unwrap();
    let id = queued(&mut s);
    let a = s.begin_send_attempt(id, 1, NOW).unwrap();
    // Process died here; the next process recovers.
    assert_eq!(s.recover_interrupted_handoffs().unwrap(), 1);
    assert_eq!(
        s.send_statuses(&[id]).unwrap()[0].1,
        SendStatus::Indeterminate
    );
    assert!(s.handoff_open(&[id]).unwrap().is_empty());
    // Never back to queued/sending (no resend); a late radio callback wins.
    assert!(
        s.set_send_status(id, SendStatus::Queued)
            .unwrap()
            .is_some_and(|u| !u.changed)
    );
    let u = s
        .record_send_part(a, 0, PartKind::Sent, PartOutcome::Ok)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Sent);
}

// ---- #5 tombstones and recovery --------------------------------------------

#[test]
fn a_tombstone_newer_than_the_snapshot_is_not_pruned() {
    let mut s = Store::open_in_memory(330).unwrap();
    s.ingest_from_provider(&incoming("+1555", "old"), Some(1), NOW)
        .unwrap();
    // 1. snapshot, 2. provider queried (has only row 1)
    let snap = s.provider_snapshot().unwrap();
    // 3. spam arrives as provider row 2 and cleanup deletes it locally
    let (spam, _) = s
        .ingest_from_provider(&incoming("VM-SPAM", "win a prize"), Some(2), NOW)
        .unwrap();
    s.delete_messages(&[spam.id]).unwrap();
    // 4. the older snapshot lacks row 2: its tombstone must survive
    s.provider_reconcile(&snap.candidates, &snap.tombstones, &[1], &[], &[1])
        .unwrap();
    let snap = s.provider_snapshot().unwrap();
    assert_eq!(snap.tombstones, vec![2]);
    // Next pass sees row 2 in the provider inbox: not replayed.
    let r = s
        .provider_reconcile(&snap.candidates, &snap.tombstones, &[1, 2], &[], &[1, 2])
        .unwrap();
    assert!(r.missing.is_empty(), "deleted SMS would be resurrected");
}

#[test]
fn explicit_import_recovers_legacy_rows_below_the_replay_floor() {
    let dir = std::env::temp_dir().join(format!("snipt-r4-floor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v1.db");
    let _ = std::fs::remove_file(&path);
    {
        // v1 store that ingested provider row 22 but missed row 21.
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c.transaction().unwrap();
        tx.execute_batch(sms_store::schema::SCHEMA_V1).unwrap();
        tx.execute(
            "INSERT INTO messages (id, ins_seq, provider_id, provider_kind, thread_id, sender,
                sender_lc, body, ts_ms, is_known_contact, is_outgoing, sim_slot, category,
                confidence, reason, scores_json, sender_kind_json)
             VALUES (1001, 1, 22, 'sms', 101, '+1555', '+1555', 'b', ?1, 0, 0, 0, 'Personal',
                1.0, 'Contact', '[0,0,0,0,0]', '{\"type\":\"PhoneNumber\"}')",
            [NOW],
        )
        .unwrap();
        tx.pragma_update(None, "user_version", 1).unwrap();
        tx.commit().unwrap();
    }
    let mut s = Store::open(&path, 330).unwrap();
    let snap = s.provider_snapshot().unwrap();
    let r = s
        .provider_reconcile(
            &snap.candidates,
            &snap.tombstones,
            &[21, 22],
            &[],
            &[21, 22],
        )
        .unwrap();
    assert!(
        r.missing.is_empty(),
        "automatic replay stays below the floor"
    );
    assert_eq!(s.provider_missing(&[21, 22]).unwrap(), vec![21]);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
