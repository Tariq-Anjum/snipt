//! S15 store paths: schema v3, MMS ingest with attachments, provider-id
//! namespaces, pending/retry bookkeeping, caps.

mod common;

use sms_store::{AttachmentIn, FailOutcome, IncomingSms, MmsNotice, NoticeAction, Store};

const NOW: i64 = 1_795_000_000_000;

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

fn png() -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend([0u8; 64]);
    v
}

fn notice(tid: &str) -> MmsNotice {
    MmsNotice {
        transaction_id: tid.into(),
        location: "http://mmsc.example/m/1".into(),
        sender: Some("+919812345678".into()),
        sub_id: Some(1),
        sim_slot: 0,
        received_ms: NOW,
        expiry_ms: Some(NOW + 3_600_000),
    }
}

#[test]
fn migrates_to_latest() {
    let s = Store::open_in_memory(common::TZ).unwrap();
    assert_eq!(
        s.schema_version().unwrap(),
        sms_store::migrate::LATEST_VERSION
    );
}

#[test]
fn v2_database_upgrades_in_place() {
    let dir = std::env::temp_dir().join(format!("snipt-v2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v2.db");
    let _ = std::fs::remove_file(&path);
    {
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c.transaction().unwrap();
        tx.execute_batch(sms_store::schema::SCHEMA_V1).unwrap();
        tx.execute_batch(sms_store::schema::SCHEMA_V2).unwrap();
        tx.pragma_update(None, "user_version", 2).unwrap();
        tx.commit().unwrap();
    }
    let mut s = Store::open(&path, common::TZ).unwrap();
    assert_eq!(
        s.schema_version().unwrap(),
        sms_store::migrate::LATEST_VERSION
    );
    let (m, ins) = s
        .ingest_from_provider(&incoming("+1555", "hello"), Some(7), NOW)
        .unwrap();
    assert!(ins);
    assert_eq!(s.mms_refs(&[m.id]).unwrap().len(), 0);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mms_ingest_with_attachments_and_namespaces() {
    let mut s = Store::open_in_memory(common::TZ).unwrap();
    // SMS provider id 5 and MMS provider id 5 are different rows.
    let (sms, _) = s
        .ingest_from_provider(&incoming("+919812345678", "plain sms"), Some(5), NOW)
        .unwrap();
    let atts = vec![
        AttachmentIn {
            seq: 2,
            content_type: "image/png".into(),
            name: Some("image.png".into()),
            image_type: Some("image/png"),
            data: png(),
        },
        AttachmentIn {
            seq: 3,
            content_type: "audio/amr".into(),
            name: Some("v.amr".into()),
            image_type: None,
            data: vec![1, 2, 3],
        },
    ];
    let (mms, ins, imgs) = s
        .ingest_mms(
            &incoming("+919812345678", "Your OTP is 482913"),
            Some(5),
            &atts,
            None,
            NOW,
        )
        .unwrap();
    assert!(ins);
    assert_ne!(mms.id, sms.id);
    assert_eq!(mms.thread_id, sms.thread_id);
    assert_eq!(imgs.len(), 1);
    // Classified like an SMS (OTP detected).
    assert!(s.message(mms.id).unwrap().unwrap().otp.is_some());
    // Namespaces: SMS refs see only the SMS row, MMS refs only the MMS row.
    assert_eq!(s.provider_refs(&[sms.id, mms.id]).unwrap().len(), 1);
    let mr = s.mms_refs(&[sms.id, mms.id]).unwrap();
    assert_eq!((mr.len(), mr[0].id, mr[0].provider_id), (1, mms.id, 5));
    // Idempotent per mms_id.
    let (again, ins2, imgs2) = s
        .ingest_mms(&incoming("+919812345678", "dup"), Some(5), &atts, None, NOW)
        .unwrap();
    assert!(!ins2 && imgs2.is_empty());
    assert_eq!(again.id, mms.id);
    // Attachment metadata + image bytes; unsupported never returns bytes.
    let meta = s.mms_attachments(&[mms.id, sms.id]).unwrap();
    let m = &meta[&mms.id];
    assert_eq!(meta.len(), 1);
    assert_eq!(m.len(), 2);
    assert_eq!((m[0].kind.as_str(), m[0].has_thumb), ("image", false));
    assert_eq!(m[1].kind, "unsupported");
    // R5 #7: nothing is served before the platform thumbnailer validated
    // the image (no original-byte fallback for a missing thumbnail).
    assert!(s.mms_image(m[0].id, true).unwrap().is_none());
    assert!(s.mms_image(m[0].id, false).unwrap().is_none());
    assert!(s.mms_image(m[1].id, false).unwrap().is_none());
    assert!(s.mms_set_thumb(m[0].id, &[0xFF, 0xD8, 0xFF]).unwrap());
    assert!(!s.mms_set_thumb(m[1].id, &[0xFF, 0xD8, 0xFF]).unwrap());
    assert!(s.mms_set_thumb(m[0].id, &vec![0; 300 * 1024]).is_err());
    assert_eq!(s.mms_image(m[0].id, true).unwrap().unwrap().0, "image/jpeg");
    let (ct, bytes) = s.mms_image(m[0].id, false).unwrap().unwrap();
    assert_eq!((ct.as_str(), bytes), ("image/png", png()));
    // Delete cascades to parts.
    s.delete_messages(&[mms.id]).unwrap();
    assert!(s.mms_image(m[0].id, false).unwrap().is_none());
    let left: i64 = s
        .connection()
        .query_row("SELECT COUNT(*) FROM mms_parts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn image_caps() {
    let mut s = Store::open_in_memory(common::TZ).unwrap();
    let big = AttachmentIn {
        seq: 1,
        content_type: "image/jpeg".into(),
        name: None,
        image_type: Some("image/jpeg"),
        data: vec![0; sms_store::mms::MAX_IMAGE_BYTES + 1],
    };
    let many: Vec<AttachmentIn> = (0..40)
        .map(|i| AttachmentIn {
            seq: i,
            content_type: "image/png".into(),
            name: None,
            image_type: Some("image/png"),
            data: png(),
        })
        .collect();
    let (m, _, imgs) = s
        .ingest_mms(&incoming("a", "x"), Some(1), &[big], None, NOW)
        .unwrap();
    assert!(imgs.is_empty());
    assert_eq!(
        s.mms_attachments(&[m.id]).unwrap()[&m.id][0].kind,
        "unsupported"
    );
    let (m2, _, imgs2) = s
        .ingest_mms(&incoming("a", "y"), Some(2), &many, None, NOW)
        .unwrap();
    assert_eq!(imgs2.len(), sms_store::mms::MAX_ATTACHMENTS);
    assert_eq!(
        s.mms_attachments(&[m2.id]).unwrap()[&m2.id].len(),
        sms_store::mms::MAX_ATTACHMENTS
    );
}

#[test]
fn pending_lifecycle_retry_and_give_up() {
    let mut s = Store::open_in_memory(common::TZ).unwrap();
    let (p, a) = s.mms_notice(&notice("T1")).unwrap();
    assert_eq!(a, NoticeAction::Download);
    // Same push again while pending: download again, same row.
    let (p2, a2) = s.mms_notice(&notice("T1")).unwrap();
    assert_eq!((p2.pending_id, a2), (p.pending_id, NoticeAction::Download));
    assert_eq!(s.mms_retryable(NOW).unwrap().len(), 1);
    for i in 1..=3 {
        assert_eq!(s.mms_attempt(p.pending_id, NOW).unwrap(), i);
        let want = if i < 3 {
            FailOutcome::Retry
        } else {
            FailOutcome::GaveUp
        };
        assert_eq!(s.mms_fail(p.pending_id, NOW).unwrap(), want);
    }
    assert!(s.mms_retryable(NOW).unwrap().is_empty());
    assert_eq!(s.mms_exhausted(NOW).unwrap().len(), 1);
    // Placeholder (no mms_id) closes it as failed.
    let (ph, ins, _) = s
        .ingest_mms(
            &incoming(
                "+919812345678",
                "Multimedia message could not be downloaded.",
            ),
            None,
            &[],
            Some(p.pending_id),
            NOW,
        )
        .unwrap();
    assert!(ins);
    assert_eq!(
        s.mms_pending(p.pending_id).unwrap().unwrap().state,
        "failed"
    );
    assert!(s.mms_exhausted(NOW).unwrap().is_empty());
    // Re-push after give-up is a duplicate; a re-ingest returns the stored row.
    assert_eq!(
        s.mms_notice(&notice("T1")).unwrap().1,
        NoticeAction::Duplicate
    );
    let (same, ins2, _) = s
        .ingest_mms(&incoming("x", "y"), Some(9), &[], Some(p.pending_id), NOW)
        .unwrap();
    assert!(!ins2);
    assert_eq!(same.id, ph.id);
    assert!(s.mms_attempt(p.pending_id, NOW).is_err());
}

#[test]
fn pending_success_and_expiry() {
    let mut s = Store::open_in_memory(common::TZ).unwrap();
    let (p, _) = s.mms_notice(&notice("T2")).unwrap();
    s.mms_attempt(p.pending_id, NOW).unwrap();
    let (m, ins, _) = s
        .ingest_mms(
            &incoming("+1", "hi"),
            Some(77),
            &[],
            Some(p.pending_id),
            NOW,
        )
        .unwrap();
    assert!(ins);
    let st = s.mms_pending(p.pending_id).unwrap().unwrap();
    assert_eq!(st.state, "done");
    assert_eq!(
        s.mms_notice(&notice("T2")).unwrap().1,
        NoticeAction::Duplicate
    );
    assert_eq!(s.mms_fail(p.pending_id, NOW).unwrap(), FailOutcome::Ignored);
    assert_eq!(s.mms_refs(&[m.id]).unwrap()[0].provider_id, 77);
    // Expiry: first failure after expiry gives up at once.
    let (p3, _) = s.mms_notice(&notice("T3")).unwrap();
    s.mms_attempt(p3.pending_id, NOW).unwrap();
    assert_eq!(
        s.mms_fail(p3.pending_id, NOW + 3_600_001).unwrap(),
        FailOutcome::GaveUp
    );
    assert!(s.mms_retryable(NOW + 3_600_001).unwrap().is_empty());
}

#[test]
fn notice_validation() {
    let mut s = Store::open_in_memory(common::TZ).unwrap();
    let mut n = notice("");
    assert!(s.mms_notice(&n).is_err());
    n.transaction_id = "T".into();
    n.location = "".into();
    assert!(s.mms_notice(&n).is_err());
    n.location = "x".repeat(3000);
    assert!(s.mms_notice(&n).is_err());
    n.location = "http://m".into();
    n.sim_slot = 2;
    assert!(s.mms_notice(&n).is_err());
}

#[test]
fn r5_inflight_attempt_is_not_finalized_and_ids_are_scoped_per_subscription() {
    let mut s = Store::open_in_memory(330).unwrap();
    let notice = |sub: Option<i64>, loc: &str| sms_store::MmsNotice {
        transaction_id: "T1".into(),
        location: loc.into(),
        sender: Some("+919800000002".into()),
        sub_id: sub,
        sim_slot: 0,
        received_ms: NOW,
        expiry_ms: None,
    };
    // R5 #6: same transaction id on two subscriptions -> two downloads.
    let (a, act_a) = s.mms_notice(&notice(Some(1), "http://mmsc/a")).unwrap();
    let (b, act_b) = s.mms_notice(&notice(Some(2), "http://mmsc/b")).unwrap();
    assert_ne!(a.pending_id, b.pending_id);
    assert_eq!(
        (act_a, act_b),
        (
            sms_store::NoticeAction::Download,
            sms_store::NoticeAction::Download
        )
    );
    assert_eq!(b.sub_id, Some(2));
    // A re-sent notice for the same exchange is the same row.
    let (a2, _) = s.mms_notice(&notice(Some(1), "http://mmsc/a")).unwrap();
    assert_eq!(a2.pending_id, a.pending_id);
    // R5 #5: the final attempt is in flight -> maintenance must not finalize it.
    for _ in 0..sms_store::mms::MAX_ATTEMPTS {
        s.mms_attempt(a.pending_id, NOW).unwrap();
    }
    assert!(s.mms_exhausted(NOW + 1000).unwrap().is_empty());
    // After the attempt fails (or times out) it can be finalized.
    s.mms_fail(a.pending_id, NOW + 2000).unwrap();
    assert_eq!(s.mms_exhausted(NOW + 3000).unwrap().len(), 1);
    // A lost callback: finalized after the in-flight timeout.
    for _ in 0..sms_store::mms::MAX_ATTEMPTS {
        s.mms_attempt(b.pending_id, NOW).unwrap();
    }
    assert_eq!(s.mms_exhausted(NOW + 11 * 60 * 1000).unwrap().len(), 2);
}
