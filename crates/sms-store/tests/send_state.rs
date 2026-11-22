//! R2 #3 / R3 #1-#3 regressions: ids are never reused, send attempts carry
//! a once-bound provider id, per-part results are persisted and the send
//! state only moves forward; R2 #4 / R3 #2 reconciliation race.

use sms_store::ids::{MAX_EXTERNAL_ID, MAX_ID};
use sms_store::{IncomingSms, OutgoingSms, PartKind, PartOutcome, ProviderRef, SendStatus, Store};
use sms_types::RawSms;

const NOW: i64 = 1_760_000_000_000;

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

fn to(addr: &str) -> OutgoingSms {
    OutgoingSms {
        thread_id: None,
        address: Some(addr.into()),
        body: "hello".into(),
        sim_slot: 0,
        reply_to: None,
    }
}

fn status(s: &Store, id: i64) -> SendStatus {
    s.send_statuses(&[id]).unwrap()[0].1
}

#[test]
fn deleted_newest_message_and_thread_ids_are_never_reused() {
    let mut s = Store::open_in_memory(330).unwrap();
    let a = s
        .ingest_incoming(&incoming("+15550001", "one"), NOW)
        .unwrap();
    assert_eq!(s.delete_messages(&[a.id]).unwrap(), 1);
    // A different contact after deleting the only (newest) conversation.
    let b = s
        .ingest_incoming(&incoming("+15550002", "two"), NOW)
        .unwrap();
    assert_ne!(b.id, a.id, "message id reused");
    assert_ne!(b.thread_id, a.thread_id, "thread id reused");
    s.delete_messages(&[b.id]).unwrap();
    let c = s.queue_outgoing(&to("+15550003"), NOW).unwrap();
    assert!(c.id > b.id && c.thread_id > b.thread_id);
    // Survives reopening (persisted high-water mark), file-backed.
    let dir = std::env::temp_dir().join(format!("snipt-ids-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.db");
    let last = {
        let mut f = Store::open(&path, 0).unwrap();
        let m = f.ingest_incoming(&incoming("+1555", "x"), NOW).unwrap();
        f.delete_messages(&[m.id]).unwrap();
        m
    };
    let mut f = Store::open(&path, 0).unwrap();
    let n = f.ingest_incoming(&incoming("+1666", "y"), NOW).unwrap();
    assert!(n.id > last.id && n.thread_id > last.thread_id);
    drop(f);
    let _ = std::fs::remove_dir_all(&dir);
}

fn raw(id: i64, thread_id: i64) -> RawSms {
    RawSms {
        id,
        thread_id,
        sender: "VM-TEST".into(),
        contact_name: None,
        body: "Hello there".into(),
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

#[test]
fn ids_outside_js_safe_range_are_rejected_and_never_allocate_as_real() {
    let mut s = Store::open_in_memory(330).unwrap();
    for (id, t) in [
        (i64::MAX, 1),
        (MAX_ID + 1, 1),
        (0, 1),
        (-5, 1),
        (5, i64::MAX),
        (5, 0),
    ] {
        assert!(s.ingest_batch(&[raw(id, t)]).is_err(), "id {id} thread {t}");
    }
    assert_eq!(s.message_count().unwrap(), 0);
    // R4 #2: an explicit id the allocator could never continue from is
    // refused; the largest external id is stored and allocation continues.
    assert!(s.ingest_batch(&[raw(MAX_ID, 1)]).is_err());
    assert!(s.ingest_batch(&[raw(5, MAX_ID)]).is_err());
    assert!(s.ingest_batch(&[raw(MAX_EXTERNAL_ID + 1, 1)]).is_err());
    assert_eq!(s.message_count().unwrap(), 0);
    s.ingest_batch(&[raw(MAX_EXTERNAL_ID, MAX_EXTERNAL_ID)])
        .unwrap();
    let next = s.ingest_incoming(&incoming("+1777", "z"), NOW).unwrap();
    assert!(next.id > MAX_EXTERNAL_ID && next.thread_id > MAX_EXTERNAL_ID);
    let typ: String = s
        .connection()
        .query_row("SELECT typeof(MAX(id)) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(typ, "integer");
}

#[test]
fn send_status_is_monotonic_and_late_callbacks_cannot_regress_it() {
    let mut s = Store::open_in_memory(330).unwrap();
    let m = s.queue_outgoing(&to("+15550001"), NOW).unwrap();
    let a = s.begin_send_attempt(m.id, 1, NOW).unwrap();
    assert_eq!(status(&s, m.id), SendStatus::Sending);
    assert!(
        s.begin_send_attempt(m.id, 1, NOW).is_err(),
        "one attempt per queued message"
    );
    // Delivery report arrives before the sent callback (separate threads).
    let u = s
        .record_send_part(a, 0, PartKind::Delivery, PartOutcome::Ok)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Sending);
    let u = s
        .record_send_part(a, 0, PartKind::Sent, PartOutcome::Ok)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Delivered);
    // Late / duplicate / contradictory callbacks: no regression.
    let u = s
        .record_send_part(a, 0, PartKind::Sent, PartOutcome::Failed)
        .unwrap()
        .unwrap();
    assert_eq!((u.status, u.changed), (SendStatus::Delivered, false));
    let u = s.set_send_status(m.id, SendStatus::Sent).unwrap().unwrap();
    assert_eq!((u.status, u.changed), (SendStatus::Delivered, false));
    let u = s
        .set_send_status(m.id, SendStatus::Failed)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Delivered, "delivered is terminal");
    assert_eq!(status(&s, m.id), SendStatus::Delivered);

    // Failed is terminal too.
    let m2 = s.queue_outgoing(&to("+15550002"), NOW).unwrap();
    s.set_send_status(m2.id, SendStatus::Failed).unwrap();
    assert!(s.begin_send_attempt(m2.id, 1, NOW).is_err());
    let u = s.set_send_status(m2.id, SendStatus::Sent).unwrap().unwrap();
    assert_eq!((u.status, u.changed), (SendStatus::Failed, false));
}

#[test]
fn multipart_needs_every_part_and_any_failure_is_terminal() {
    let mut s = Store::open_in_memory(330).unwrap();
    let m = s.queue_outgoing(&to("+15550001"), NOW).unwrap();
    let parts = 40; // > 32: the old request-code scheme collided here
    let a = s.begin_send_attempt(m.id, parts, NOW).unwrap();
    // The highest-index part first must not mark the message sent.
    let u = s
        .record_send_part(a, parts - 1, PartKind::Sent, PartOutcome::Ok)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Sending);
    for p in 0..parts - 2 {
        s.record_send_part(a, p, PartKind::Sent, PartOutcome::Ok)
            .unwrap();
    }
    assert_eq!(status(&s, m.id), SendStatus::Sending);
    let u = s
        .record_send_part(a, parts - 2, PartKind::Sent, PartOutcome::Failed)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Failed);
    // Out-of-range part and unknown attempt are ignored.
    assert!(
        s.record_send_part(a, parts, PartKind::Sent, PartOutcome::Ok)
            .unwrap()
            .is_none()
    );
    assert!(
        s.record_send_part(a + 999, 0, PartKind::Sent, PartOutcome::Ok)
            .unwrap()
            .is_none()
    );
    // Results are persisted rows, not process memory.
    let n: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM send_part_results WHERE attempt_id = ?1",
            [a],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, i64::from(parts));
}

#[test]
fn uninterpretable_delivery_report_is_unconfirmed_not_delivered() {
    let mut s = Store::open_in_memory(330).unwrap();
    let m = s.queue_outgoing(&to("+15550001"), NOW).unwrap();
    let a = s.begin_send_attempt(m.id, 2, NOW).unwrap();
    for p in 0..2 {
        s.record_send_part(a, p, PartKind::Sent, PartOutcome::Ok)
            .unwrap();
    }
    assert_eq!(status(&s, m.id), SendStatus::Sent);
    s.record_send_part(a, 0, PartKind::Delivery, PartOutcome::Ok)
        .unwrap();
    assert_eq!(status(&s, m.id), SendStatus::Sent);
    let u = s
        .record_send_part(a, 1, PartKind::Delivery, PartOutcome::Unknown)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Unconfirmed);
    // A carrier failure report is a failure, not "delivered".
    let m2 = s.queue_outgoing(&to("+15550002"), NOW).unwrap();
    let a2 = s.begin_send_attempt(m2.id, 1, NOW).unwrap();
    s.record_send_part(a2, 0, PartKind::Sent, PartOutcome::Ok)
        .unwrap();
    let u = s
        .record_send_part(a2, 0, PartKind::Delivery, PartOutcome::Failed)
        .unwrap()
        .unwrap();
    assert_eq!(u.status, SendStatus::Failed);
}

#[test]
fn provider_binding_is_set_once_and_attempt_ids_are_unique() {
    let mut s = Store::open_in_memory(330).unwrap();
    let m = s.queue_outgoing(&to("+15550001"), NOW).unwrap();
    let a = s.begin_send_attempt(m.id, 1, NOW).unwrap();
    assert!(s.bind_send_provider(a, 10).unwrap());
    assert!(
        s.bind_send_provider(a, 10).unwrap(),
        "same id is idempotent"
    );
    assert!(!s.bind_send_provider(a, 11).unwrap(), "rebinding refused");
    assert_eq!(s.provider_refs(&[m.id]).unwrap()[0].provider_id, 10);
    // Another message's attempt cannot take provider row 10.
    let m2 = s.queue_outgoing(&to("+15550002"), NOW).unwrap();
    let a2 = s.begin_send_attempt(m2.id, 1, NOW).unwrap();
    assert!(!s.bind_send_provider(a2, 10).unwrap());
    assert!(s.provider_refs(&[m2.id]).unwrap().is_empty());
    // Deleting the message (cascade) never frees its attempt id for reuse.
    // (R4 #4: only after the hand-offs are closed.)
    s.end_send_handoff(a, true).unwrap();
    s.end_send_handoff(a2, false).unwrap();
    assert_eq!(s.delete_messages(&[m.id, m2.id]).unwrap(), 2);
    let m3 = s.queue_outgoing(&to("+15550003"), NOW).unwrap();
    let a3 = s.begin_send_attempt(m3.id, 1, NOW).unwrap();
    assert!(a3 > a2);
    assert!(
        s.record_send_part(a2, 0, PartKind::Sent, PartOutcome::Ok)
            .unwrap()
            .is_none(),
        "callback of a deleted message's attempt is inert"
    );
    assert_eq!(status(&s, m3.id), SendStatus::Sending);
}

#[test]
fn reply_is_bound_to_source_message_and_recipient() {
    let mut s = Store::open_in_memory(330).unwrap();
    let a = s
        .ingest_incoming(&incoming("+15550001", "hi"), NOW)
        .unwrap();
    let reply = OutgoingSms {
        thread_id: Some(a.thread_id),
        address: Some(" +15550001 ".into()),
        body: "ok".into(),
        sim_slot: 0,
        reply_to: Some(a.id),
    };
    s.queue_outgoing(&reply, NOW).unwrap();
    let wrong = OutgoingSms {
        address: Some("+19990000".into()),
        ..reply.clone()
    };
    assert!(s.queue_outgoing(&wrong, NOW).is_err(), "recipient mismatch");
    // Source deleted: the stale notification's reply is refused.
    let msgs = s.messages_page(a.thread_id, None, 50).unwrap().0;
    let ids: Vec<i64> = msgs.iter().map(|m| m.sms.id).collect();
    s.delete_messages(&ids).unwrap();
    let b = s
        .ingest_incoming(&incoming("+15550002", "other"), NOW)
        .unwrap();
    assert_ne!(b.thread_id, a.thread_id);
    assert!(s.queue_outgoing(&reply, NOW).is_err());
}

#[test]
fn reconcile_never_deletes_a_message_received_after_the_candidate_snapshot() {
    let mut s = Store::open_in_memory(330).unwrap();
    s.ingest_from_provider(&incoming("+1555", "one"), Some(1), NOW)
        .unwrap();
    s.ingest_from_provider(&incoming("+1555", "two"), Some(2), NOW)
        .unwrap();
    // 1. local candidates captured first
    let candidates = s.provider_candidates().unwrap();
    assert_eq!(candidates.len(), 2);
    // 2. provider queried: row 2 was deleted elsewhere
    let present = [1];
    // 3. SMS_DELIVER writes provider row 3 and ingests it before the
    //    reconcile is applied (the provider snapshot above lacks it)
    let (late, _) = s
        .ingest_from_provider(&incoming("+1555", "three"), Some(3), NOW)
        .unwrap();
    let r = s
        .provider_reconcile(&candidates, &[], &present, &[], &[1])
        .unwrap();
    assert_eq!(r.deleted, 1);
    assert!(
        s.message(late.id).unwrap().is_some(),
        "late receipt deleted"
    );
    assert_eq!(s.message_count().unwrap(), 2);
    // A candidate whose stored provider id no longer matches is not deleted.
    let stale = [ProviderRef {
        id: late.id,
        provider_id: 999,
    }];
    let r = s
        .provider_reconcile(&stale, &[], &[1, 3], &[], &[])
        .unwrap();
    assert_eq!(r.deleted, 0);
    // Empty provider snapshot with candidates is refused.
    assert!(
        s.provider_reconcile(&candidates, &[], &[], &[], &[])
            .is_err()
    );
}

#[test]
fn reconcile_reports_missed_receipts_but_not_local_deletions() {
    let mut s = Store::open_in_memory(330).unwrap();
    let (a, _) = s
        .ingest_from_provider(&incoming("+1555", "kept"), Some(5), NOW)
        .unwrap();
    let (b, _) = s
        .ingest_from_provider(&incoming("+1555", "purged"), Some(6), NOW)
        .unwrap();
    s.delete_messages(&[b.id]).unwrap(); // local delete (e.g. cleanup)
    let snap = s.provider_snapshot().unwrap();
    let c = snap.candidates.clone();
    // provider still has 5, 6 and a row 7 whose ingest was lost
    let r = s
        .provider_reconcile(&c, &snap.tombstones, &[5, 6, 7], &[5], &[5, 6, 7])
        .unwrap();
    assert_eq!(r.missing, vec![7]);
    assert_eq!(r.marked_read, 1);
    assert!(s.message(a.id).unwrap().unwrap().sms.read());
    // Replay is idempotent through the provider id.
    let (_, inserted) = s
        .ingest_from_provider(&incoming("+1555", "late"), Some(7), NOW)
        .unwrap();
    assert!(inserted);
    let snap = s.provider_snapshot().unwrap();
    let c = snap.candidates.clone();
    let r = s
        .provider_reconcile(&c, &snap.tombstones, &[5, 6, 7], &[], &[5, 6, 7])
        .unwrap();
    assert!(r.missing.is_empty());
    // Once row 6 is gone from the provider too, its tombstone is dropped.
    let r = s
        .provider_reconcile(&c, &snap.tombstones, &[5, 7], &[], &[5, 7])
        .unwrap();
    assert_eq!(r.deleted, 0);
    let n: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM provider_tombstones", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
