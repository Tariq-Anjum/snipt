//! Store paths with no TS oracle counterpart: thread previews, inbox counts,
//! provider-id ingest, the outgoing send queue and provider sync. Counts are
//! still checked against the TS `filterThreads` fixture.

mod common;

use common::*;
use sms_engine::inbox::InboxFilter;
use sms_store::{
    IncomingSms, OutgoingSms, PartKind, PartOutcome, SendStatus, Store, threads::make_preview,
};
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
    (s, now, seed)
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
fn previews_come_from_latest_message_with_otp_masked() {
    let (s, _, seed) = seeded("base");
    let engine = sms_engine::SmsEngine::new();
    let mut saw_otp = false;
    for t in s.all_threads().unwrap() {
        let latest = seed.iter().find(|m| m.id == t.latest_message_id).unwrap();
        let a = engine.analyze(latest);
        let code = a.otp.as_ref().map(|o| o.code.clone());
        assert_eq!(t.preview, make_preview(&latest.body, code.as_deref()));
        assert!(t.preview.chars().count() <= 140, "thread {}", t.thread_id);
        assert_eq!(t.preview_is_outgoing, latest.is_outgoing);
        assert_eq!(t.latest_has_otp, a.otp.is_some());
        assert_eq!(t.latest_transaction, a.transaction);
        assert_eq!(t.latest_reminder, a.reminder);
        if let Some(code) = code {
            saw_otp = true;
            assert!(
                !t.preview.contains(&code),
                "thread {} leaks OTP",
                t.thread_id
            );
        }
    }
    assert!(saw_otp, "seed has a thread whose latest message is an OTP");
    // Paged / single / search rows carry the same preview.
    let page = s.threads_page(InboxFilter::All, None, None, 500).unwrap();
    for r in &page.threads {
        assert_eq!(s.thread(r.thread_id).unwrap().unwrap(), *r);
    }
    let hits = s.search(InboxFilter::All, None, "hdfc", 100).unwrap();
    assert!(!hits.is_empty() && hits.iter().all(|h| !h.preview.is_empty()));
}

#[test]
fn preview_is_capped_and_collapsed() {
    let long = "word\n".repeat(100);
    let p = make_preview(&long, None);
    assert_eq!(p.chars().count(), 140);
    assert!(p.ends_with('\u{2026}') && !p.contains('\n'));
    assert_eq!(make_preview("  a \n\t b  ", None), "a b");
}

#[test]
fn inbox_counts_equal_ts_filter_fixture() {
    let fx = fixture("threads.json");
    for state in ["base", "blocked"] {
        let (s, _, seed) = seeded(state);
        let c = s.inbox_counts(None).unwrap();
        let want = |f: &str| {
            fx["filters"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| {
                    x["state"] == state && x["filter"] == f && x["sim"] == "all" && x["query"] == ""
                })
                .unwrap()["threadIds"]
                .as_array()
                .unwrap()
                .len() as i64
        };
        let f = c.filters;
        let got = [
            ("All", f.all),
            ("Personal", f.personal),
            ("Transactions", f.transactions),
            ("Promotions", f.promotions),
            ("Priority", f.priority),
            ("Spam", f.spam),
            ("RCS", f.rcs),
            ("Starred", f.starred),
            ("Archived", f.archived),
        ];
        for (name, n) in got {
            assert_eq!(n, want(name), "{state} {name}");
        }
        assert_eq!(c.total_messages, seed.len() as i64);
        assert_eq!(c.capacity, 10_000);
        let unread: i64 = fx["threads"][state]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| {
                !t["isArchived"].as_bool().unwrap() && t["classification"]["category"] != "Spam"
            })
            .map(|t| t["unreadCount"].as_i64().unwrap())
            .sum();
        assert_eq!(c.unread, unread, "{state} unread");
        // SIM-scoped counts partition the SIM-agnostic ones.
        let (a, b) = (
            s.inbox_counts(Some(0)).unwrap(),
            s.inbox_counts(Some(1)).unwrap(),
        );
        assert_eq!(a.filters.all + b.filters.all, f.all);
    }
}

#[test]
fn provider_ingest_is_idempotent_and_syncs_read_delete() {
    let (mut s, now, _) = seeded("base");
    let msg = incoming("AX-HDFCBK", "Your OTP is 551203 for login. Do not share.");
    let (a, ins) = s.ingest_from_provider(&msg, Some(9001), now).unwrap();
    assert!(ins);
    let (b, ins2) = s.ingest_from_provider(&msg, Some(9001), now + 5).unwrap();
    assert!(!ins2);
    assert_eq!(a, b);
    let refs = s.provider_refs(&[a.id, -1]).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].provider_id, 9001);

    let unread_before = s.thread(a.thread_id).unwrap().unwrap().unread_count;
    assert_eq!(s.provider_mark_read(&[9001, 424242]).unwrap(), 1);
    assert_eq!(s.provider_mark_read(&[9001]).unwrap(), 0);
    assert_eq!(
        s.thread(a.thread_id).unwrap().unwrap().unread_count,
        unread_before - 1
    );
    let n = s.message_count().unwrap();
    assert_eq!(s.provider_delete(&[9001]).unwrap(), 1);
    assert_eq!(s.message_count().unwrap(), n - 1);
    assert!(s.message(a.id).unwrap().is_none());
}

#[test]
fn outgoing_queue_and_send_status() {
    let (mut s, now, _) = seeded("base");
    let reply = OutgoingSms {
        thread_id: Some(101),
        address: None,
        body: "  On my way  ".into(),
        sim_slot: 1,
        reply_to: None,
    };
    let m = s.queue_outgoing(&reply, now + 10).unwrap();
    assert!(m.is_outgoing && m.read());
    assert_eq!(m.body, "On my way");
    assert_eq!(m.thread_id, 101);
    let t = s.thread(101).unwrap().unwrap();
    assert_eq!(t.latest_message_id, m.id);
    assert!(t.preview_is_outgoing);
    assert_eq!(
        s.send_statuses(&[m.id]).unwrap(),
        vec![(m.id, SendStatus::Queued)]
    );

    let a = s.begin_send_attempt(m.id, 1, now + 11).unwrap();
    assert!(s.bind_send_provider(a, 777).unwrap());
    let u = s
        .record_send_part(a, 0, PartKind::Sent, PartOutcome::Ok)
        .unwrap()
        .unwrap();
    assert_eq!((u.status, u.changed), (SendStatus::Sent, true));
    assert_eq!(
        s.send_statuses(&[m.id]).unwrap(),
        vec![(m.id, SendStatus::Sent)]
    );
    assert_eq!(s.provider_refs(&[m.id]).unwrap()[0].provider_id, 777);
    // Incoming messages have no send state and cannot be given one.
    let inc = s.messages_page(101, None, 50).unwrap().0;
    let inc_id = inc.iter().find(|x| !x.sms.is_outgoing).unwrap().sms.id;
    assert!(
        s.set_send_status(inc_id, SendStatus::Sent)
            .unwrap()
            .is_none()
    );
    assert!(s.begin_send_attempt(inc_id, 1, now).is_err());
    assert!(s.send_statuses(&[inc_id]).unwrap().is_empty());

    // New address starts a thread; same address (any case) joins it.
    let first = s
        .queue_outgoing(
            &OutgoingSms {
                thread_id: None,
                address: Some("+919800000001".into()),
                body: "hi".into(),
                sim_slot: 0,
                reply_to: None,
            },
            now + 20,
        )
        .unwrap();
    assert!(s.thread(first.thread_id).unwrap().is_some());
    let mut bad = |o: OutgoingSms| s.queue_outgoing(&o, now).is_err();
    let base = OutgoingSms {
        thread_id: Some(101),
        address: None,
        body: "x".into(),
        sim_slot: 0,
        reply_to: None,
    };
    assert!(bad(OutgoingSms {
        body: "   ".into(),
        ..base.clone()
    }));
    assert!(bad(OutgoingSms {
        sim_slot: 2,
        ..base.clone()
    }));
    assert!(bad(OutgoingSms {
        thread_id: Some(-9),
        ..base.clone()
    }));
    assert!(bad(OutgoingSms {
        thread_id: None,
        ..base
    }));
}
