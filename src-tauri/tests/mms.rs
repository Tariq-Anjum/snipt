//! S15 core path: WAP push -> pending -> retrieved parts -> ingest (store,
//! OTP masking, attachments) -> thumbnail -> NotifyResp; failures/retries;
//! the UI's image read only returns allow-listed image bytes.

use base64::Engine as _;
use sms_mms::fixture as fx;
use sms_store::{FailOutcome, NoticeAction, Store};
use snipt_lib::core::{Capabilities, Core, platform_capabilities};

fn phone() -> Core {
    Core::with_capabilities(
        Store::open_in_memory(330).unwrap(),
        Capabilities {
            platform: "android",
            can_send: true,
            read_only: false,
            ..platform_capabilities()
        },
    )
}

const FROM: &str = "+919812345678/TYPE=PLMN";

#[test]
fn notification_to_ingest_end_to_end() {
    let mut c = phone();
    c.set_vault_key([7; 32]);
    let notif = fx::notification_ind("TX-1", FROM, "http://mmsc.invalid/1", 3600);
    let n = c.mms_notification(&notif, Some(3), 0).unwrap();
    assert_eq!(n.action, NoticeAction::Download);
    assert_eq!(n.location, "http://mmsc.invalid/1");
    assert_eq!(c.mms_attempt(n.pending_id).unwrap(), 1);

    let png = fx::png(32, 24);
    let pdu = fx::retrieve_conf("TX-1", FROM, "Your OTP is 482913. Photo attached", &png);
    let r = c.mms_retrieved(n.pending_id, &pdu).unwrap();
    assert_eq!(r.from, "+919812345678");
    assert_eq!(r.sub_id, Some(3));
    assert_eq!(r.parts.len(), 3);
    assert_eq!(r.parts[0].content_type, "application/smil");
    assert_eq!(
        r.parts[1].text.as_deref(),
        Some("Your OTP is 482913. Photo attached")
    );
    assert_eq!(
        &pdu[r.parts[2].offset..r.parts[2].offset + r.parts[2].len],
        &png[..]
    );

    let out = c.mms_ingest(n.pending_id, &pdu, Some(41)).unwrap();
    assert!(out.result.inserted && out.result.notification.notify);
    let msg = &out.result.notification.message;
    // OTP rules apply to MMS text: masked in the payload, code only in the vault.
    assert!(!msg.sms.body.contains("482913"), "{}", msg.sms.body);
    assert!(out.result.notification.otp_vault.is_some());
    assert_eq!(out.thumbs.len(), 1);
    let t = &out.thumbs[0];
    assert_eq!(&pdu[t.offset..t.offset + t.len], &png[..]);

    // Thread page: masked body + one image attachment.
    let page = c
        .messages_page(out.result.thread.thread_id, None, None)
        .unwrap();
    let m = &page.messages[0];
    assert!(!m.sms.body.contains("482913"));
    assert_eq!(m.attachments.len(), 1);
    assert_eq!(m.attachments[0].kind, "image");
    let json = serde_json::to_value(m).unwrap();
    assert_eq!(json["attachments"][0]["contentType"], "image/png");
    assert!(json["attachments"][0].get("data").is_none());

    // Image read (R5 #7): nothing until the platform thumbnailer validated
    // the image; then the JPEG thumbnail and the original.
    assert!(c.mms_image(t.part_id, true).is_err());
    assert!(c.mms_image(t.part_id, false).is_err());
    assert!(
        c.mms_set_thumb(t.part_id, &png).is_err(),
        "thumb must be JPEG"
    );
    assert!(
        c.mms_set_thumb(t.part_id, &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0])
            .unwrap()
    );
    assert_eq!(
        c.mms_image(t.part_id, true).unwrap().content_type,
        "image/jpeg"
    );
    let img = c.mms_image(t.part_id, false).unwrap();
    assert_eq!(img.content_type, "image/png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(img.data_b64)
        .unwrap();
    assert_eq!(bytes, png);
    assert!(c.mms_image(9999, false).is_err());

    // Provider namespaces and notification action target.
    let id = msg.sms.id;
    assert!(c.provider_refs(&[id]).unwrap().is_empty());
    assert_eq!(c.mms_refs(&[id]).unwrap()[0].provider_id, 41);
    let at = c.action_target(id, "+919812345678").unwrap().unwrap();
    assert_eq!((at.provider_id, at.mms_id), (None, Some(41)));

    // NotifyResp carries the transaction id; a repeated push is a duplicate
    // and a second retrieve/ingest is refused / returns the stored row.
    let nr = c.mms_notify_resp(n.pending_id).unwrap();
    assert_eq!(&nr[..2], &[0x8C, 0x83]);
    assert!(nr.windows(4).any(|w| w == b"TX-1"));
    assert_eq!(
        c.mms_notification(&notif, Some(3), 0).unwrap().action,
        NoticeAction::Duplicate
    );
    assert!(c.mms_retrieved(n.pending_id, &pdu).is_err());
    assert!(
        !c.mms_ingest(n.pending_id, &pdu, Some(41))
            .unwrap()
            .result
            .inserted
    );
}

#[test]
fn image_only_mms_gets_a_label_body_and_unsupported_parts_are_listed() {
    let mut c = phone();
    let n = c
        .mms_notification(
            &fx::notification_ind("TX-2", FROM, "http://m/2", 60),
            None,
            1,
        )
        .unwrap();
    // Build: related { image/jpeg (real JPEG magic), audio/amr }.
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4];
    let mut p = vec![0x8C, 0x84, 0x89];
    p.extend(fx::from(FROM));
    p.push(0x84);
    p.extend(fx::ct_general(0x33, &[]));
    p.extend(fx::uintvar(2));
    p.extend(fx::entry(&fx::ct_general(0x1E, &[]), &[], &jpeg));
    p.extend(fx::entry(
        &fx::ct_text_media("audio/amr", &[]),
        &[],
        &[9; 100],
    ));
    let out = c.mms_ingest(n.pending_id, &p, Some(5)).unwrap();
    assert_eq!(out.result.notification.message.sms.body, "[Photo]");
    let page = c
        .messages_page(out.result.thread.thread_id, None, None)
        .unwrap();
    let a = &page.messages[0].attachments;
    assert_eq!(a.len(), 2);
    assert_eq!(
        (a[0].kind.as_str(), a[1].kind.as_str()),
        ("image", "unsupported")
    );
    assert_eq!((a[1].content_type.as_str(), a[1].size), ("audio/amr", 100));
    assert!(c.mms_image(a[1].id, false).is_err());
}

#[test]
fn declared_image_with_wrong_magic_is_not_displayed() {
    let mut c = phone();
    let n = c
        .mms_notification(
            &fx::notification_ind("TX-3", FROM, "http://m/3", 60),
            None,
            0,
        )
        .unwrap();
    let mut p = vec![0x8C, 0x84, 0x84];
    p.extend(fx::ct_general(0x23, &[]));
    p.extend(fx::uintvar(1));
    p.extend(fx::entry(
        &fx::ct_general(0x1E, &[]),
        &[],
        b"<svg onload=alert(1)>",
    ));
    let out = c.mms_ingest(n.pending_id, &p, None).unwrap();
    assert!(out.thumbs.is_empty());
    let page = c
        .messages_page(out.result.thread.thread_id, None, None)
        .unwrap();
    assert_eq!(page.messages[0].attachments[0].kind, "unsupported");
}

#[test]
fn failures_retry_then_placeholder_and_bad_pdus_are_rejected() {
    let mut c = phone();
    let n = c
        .mms_notification(
            &fx::notification_ind("TX-4", FROM, "http://m/4", 3600),
            None,
            0,
        )
        .unwrap();
    for i in 1..=3 {
        c.mms_attempt(n.pending_id).unwrap();
        let (o, ph) = c.mms_failed(n.pending_id).unwrap();
        if i < 3 {
            assert_eq!(o, FailOutcome::Retry);
            assert!(ph.is_none());
            assert_eq!(c.mms_retryable().unwrap().len(), 1);
        } else {
            assert_eq!(o, FailOutcome::GaveUp);
            let ph = ph.unwrap();
            assert_eq!(ph.notification.message.sms.sender, "+919812345678");
            assert!(
                ph.notification
                    .message
                    .sms
                    .body
                    .contains("could not be downloaded")
            );
        }
    }
    assert!(c.mms_retryable().unwrap().is_empty());
    assert!(c.mms_give_up_exhausted().unwrap().is_empty());
    // Garbage / wrong PDU types never ingest.
    assert!(c.mms_notification(b"garbage", None, 0).is_err());
    let rc = fx::retrieve_conf("X", FROM, "t", &fx::png(1, 1));
    assert!(c.mms_notification(&rc, None, 0).is_err());
    let n2 = c
        .mms_notification(
            &fx::notification_ind("TX-5", FROM, "http://m/5", 3600),
            None,
            0,
        )
        .unwrap();
    assert!(c.mms_retrieved(n2.pending_id, b"\x8c\x84\x84").is_err());
    assert!(
        c.mms_ingest(n2.pending_id, &[0x8C, 0x84, 0x99, 0xE0], None)
            .is_err()
    );
    // Expired notification (relative 0 s) is given up at the next check.
    let n3 = c
        .mms_notification(
            &fx::notification_ind("TX-6", FROM, "http://m/6", 0),
            None,
            0,
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let ph = c.mms_give_up_exhausted().unwrap();
    assert_eq!(ph.len(), 1);
    assert!(c.mms_attempt(n3.pending_id).is_err());
}
