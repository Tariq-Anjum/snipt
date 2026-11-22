//! Parser tests on PDUs constructed per OMA-MMS-ENC 1.3 / WAP-230-WSP, plus
//! fuzz-style robustness (truncation, garbage, bit flips, caps).

use super::*;
use crate::fixture as fx;

fn sample_retrieve() -> (Vec<u8>, Vec<u8>) {
    let png = fx::png(8, 6);
    (
        fx::retrieve_conf(
            "T-42",
            "+919876543210/TYPE=PLMN",
            "Your OTP is 482913. Photo attached",
            &png,
        ),
        png,
    )
}

#[test]
fn notification_ind_fields() {
    let pdu = fx::notification_ind(
        "T-42",
        "+919876543210/TYPE=PLMN",
        "http://mmsc.example/m/abc",
        172_800,
    );
    let Pdu::Notification(n) = parse(&pdu).unwrap() else {
        panic!("not a notification")
    };
    assert_eq!(n.transaction_id, "T-42");
    assert_eq!(n.version, 0x12);
    assert_eq!(n.from.as_deref(), Some("+919876543210"));
    assert_eq!(n.subject.as_deref(), Some("Photo"));
    assert_eq!(n.message_size, Some(48_213));
    assert_eq!(n.expiry, Some(Expiry::Relative(172_800)));
    assert_eq!(n.expiry.unwrap().at_ms(1_000), 172_801_000);
    assert_eq!(n.content_location, "http://mmsc.example/m/abc");
}

/// Hand-assembled bytes (no builder) for the minimal mandatory set, so the
/// builder itself is checked against the spec tables.
#[test]
fn notification_ind_hand_bytes() {
    let mut pdu = vec![
        0x8C, 0x82, // X-Mms-Message-Type: m-notification-ind (130)
        0x98, b'a', b'b', 0x00, // X-Mms-Transaction-Id: "ab"
        0x8D, 0x90, // X-Mms-MMS-Version: 1.0
        0x8A, 0x80, // X-Mms-Message-Class: Personal
        0x8E, 0x02, 0x01, 0x00, // X-Mms-Message-Size: 256
        0x88, 0x05, 0x80, 0x03, 0x01, 0x02, 0x03, // X-Mms-Expiry: absolute 0x010203
        0x83, // X-Mms-Content-Location
    ];
    pdu.extend(b"http://m/1\0");
    let Pdu::Notification(n) = parse(&pdu).unwrap() else {
        panic!()
    };
    assert_eq!(n.transaction_id, "ab");
    assert_eq!(n.version, 0x10);
    assert_eq!(n.from, None);
    assert_eq!(n.message_size, Some(256));
    assert_eq!(n.expiry, Some(Expiry::Absolute(0x010203)));
    assert_eq!(n.expiry.unwrap().at_ms(5), 0x010203 * 1000);
}

#[test]
fn notification_missing_location_or_tid() {
    let pdu = [0x8C, 0x82, 0x98, b'a', 0x00];
    assert_eq!(parse(&pdu), Err(Error::Missing("X-Mms-Content-Location")));
    let mut pdu = vec![0x8C, 0x82, 0x83];
    pdu.extend(b"http://m\0");
    assert_eq!(parse(&pdu), Err(Error::Missing("X-Mms-Transaction-Id")));
}

#[test]
fn first_header_must_be_message_type() {
    let mut pdu = vec![0x98, b'a', 0, 0x8C, 0x82, 0x83];
    pdu.extend(b"http://m\0");
    assert!(matches!(parse(&pdu), Err(Error::Malformed(_))));
}

#[test]
fn retrieve_conf_multipart_related() {
    let (pdu, png) = sample_retrieve();
    let Pdu::RetrieveConf(m) = parse(&pdu).unwrap() else {
        panic!()
    };
    assert!(m.retrieve_ok());
    assert_eq!(m.transaction_id.as_deref(), Some("T-42"));
    assert_eq!(m.message_id.as_deref(), Some("msg-0001@mmsc.example"));
    assert_eq!(m.date, Some(1_795_000_000));
    assert_eq!(m.from.as_deref(), Some("+919876543210"));
    assert_eq!(m.to, vec!["+919000000001".to_string()]);
    assert_eq!(m.subject.as_deref(), Some("Photo"));
    assert_eq!(
        m.content_type.media,
        "application/vnd.wap.multipart.related"
    );
    assert_eq!(m.content_type.start.as_deref(), Some("<smil>"));
    assert_eq!(
        m.content_type.related_type.as_deref(),
        Some("application/smil")
    );
    assert_eq!(m.parts.len(), 3);
    assert!(m.parts[0].is_smil());
    assert_eq!(m.parts[0].content_id.as_deref(), Some("smil"));
    assert!(m.parts[1].is_text());
    assert_eq!(m.parts[1].text(&pdu), "Your OTP is 482913. Photo attached");
    assert_eq!(m.parts[1].name.as_deref(), Some("text.txt"));
    assert_eq!(m.parts[2].content_type.media, "image/png");
    assert_eq!(m.parts[2].name.as_deref(), Some("image.png"));
    assert_eq!(m.parts[2].data(&pdu), &png[..]);
    assert_eq!(sniff_image(m.parts[2].data(&pdu)), Some("image/png"));
    assert!(!m.truncated);
}

#[test]
fn retrieve_conf_ucs2_and_single_part_body() {
    // Body is a single text/plain part (no multipart), charset UCS-2.
    let mut p = vec![0x8C, 0x84, 0x8D, 0x92, 0x89];
    p.extend(fx::from("12345"));
    p.push(0x84);
    p.extend(fx::ct_general(0x03, &[0x81, 0x02, 0x03, 0xE8]));
    p.extend([0x09, 0x39, 0x09, 0x3F, 0x00, 0x21]); // "हि!"
    let Pdu::RetrieveConf(m) = parse(&p).unwrap() else {
        panic!()
    };
    assert_eq!(m.parts.len(), 1);
    assert_eq!(m.parts[0].text(&p), "हि!");
    assert_eq!(m.from.as_deref(), Some("12345"));
}

#[test]
fn retrieve_conf_error_status_without_body() {
    let p = [0x8C, 0x84, 0x8D, 0x92, 0x99, 0xE2]; // Error-permanent-message-not-found
    let Pdu::RetrieveConf(m) = parse(&p).unwrap() else {
        panic!()
    };
    assert!(!m.retrieve_ok());
    assert!(m.parts.is_empty());
}

#[test]
fn nested_alternative_prefers_text() {
    let html = fx::entry(&fx::ct_general(0x02, &[]), &[], b"<b>hi</b>");
    let plain = fx::entry(&fx::ct_general(0x03, &[0x81, 0xEA]), &[], b"hi");
    let mut alt_body = fx::uintvar(2);
    alt_body.extend(html);
    alt_body.extend(plain);
    let alt = fx::entry(&fx::ct_general(0x26, &[]), &[], &alt_body);
    let img = fx::entry(&fx::ct_general(0x1E, &[]), &[], &[0xFF, 0xD8, 0xFF, 0xE0]);
    let mut p = vec![0x8C, 0x84, 0x84];
    p.extend(fx::ct_general(0x23, &[]));
    p.extend(fx::uintvar(2));
    p.extend(alt);
    p.extend(img);
    let Pdu::RetrieveConf(m) = parse(&p).unwrap() else {
        panic!()
    };
    let media: Vec<&str> = m
        .parts
        .iter()
        .map(|x| x.content_type.media.as_str())
        .collect();
    assert_eq!(media, ["text/plain", "image/jpeg"]);
    assert_eq!(m.parts[0].text(&p), "hi");
}

#[test]
fn multipart_nesting_depth_cap() {
    // related > mixed > mixed: depth 3 is refused.
    let leaf = fx::entry(&fx::ct_general(0x03, &[]), &[], b"x");
    let mut lvl2 = fx::uintvar(1);
    lvl2.extend(leaf);
    let mut lvl1 = fx::uintvar(1);
    lvl1.extend(fx::entry(&fx::ct_general(0x23, &[]), &[], &lvl2));
    let mut p = vec![0x8C, 0x84, 0x84];
    p.extend(fx::ct_general(0x33, &[]));
    p.extend(fx::uintvar(1));
    p.extend(fx::entry(&fx::ct_general(0x23, &[]), &[], &lvl1));
    assert!(matches!(parse(&p), Err(Error::Malformed(_))));
}

#[test]
fn part_count_cap() {
    let mut p = vec![0x8C, 0x84, 0x84];
    p.extend(fx::ct_general(0x23, &[]));
    p.extend(fx::uintvar(100));
    for _ in 0..100 {
        p.extend(fx::entry(&fx::ct_general(0x03, &[]), &[], b"a"));
    }
    let Pdu::RetrieveConf(m) = parse(&p).unwrap() else {
        panic!()
    };
    assert_eq!(m.parts.len(), MAX_PARTS);
    assert!(m.truncated);
}

#[test]
fn size_cap_and_header_cap() {
    let big = vec![0u8; MAX_PDU_BYTES + 1];
    assert_eq!(parse(&big), Err(Error::TooLarge));
    let mut p = vec![0x8C, 0x82];
    for _ in 0..200 {
        p.extend([0x8F, 0x81]); // X-Mms-Priority: Normal
    }
    assert!(matches!(parse(&p), Err(Error::Malformed(_))));
}

#[test]
fn hostile_lengths_do_not_allocate_or_panic() {
    // DataLen = u32::MAX in a multipart entry.
    let mut p = vec![0x8C, 0x84, 0x84, 0xA3, 0x01, 0x01];
    p.extend([0x8F, 0xFF, 0xFF, 0xFF, 0x7F]);
    p.extend([0x83, b'x']);
    assert_eq!(parse(&p), Err(Error::Truncated));
    // Value-length via Length-quote larger than input.
    let p = [0x8C, 0x82, 0x89, 0x1F, 0x8F, 0xFF, 0xFF, 0xFF, 0x7F, 0x80];
    assert_eq!(parse(&p), Err(Error::Truncated));
    // Unterminated text.
    let p = [0x8C, 0x82, 0x98, b'a', b'b'];
    assert_eq!(parse(&p), Err(Error::Truncated));
}

#[test]
fn notify_resp_encoding() {
    let b = encode_notify_resp("T-42", NotifyStatus::Retrieved).unwrap();
    assert_eq!(
        b,
        [
            0x8C, 0x83, 0x98, b'T', b'-', b'4', b'2', 0x00, 0x8D, 0x92, 0x95, 0x81, 0x91, 0x81
        ]
    );
    assert_eq!(parse(&b).unwrap(), Pdu::Other(message_type::NOTIFYRESP_IND));
    assert!(encode_notify_resp("", NotifyStatus::Retrieved).is_err());
    assert!(encode_notify_resp("a\0b", NotifyStatus::Retrieved).is_err());
    assert!(encode_notify_resp("é", NotifyStatus::Retrieved).is_err());
}

#[test]
fn image_sniffing() {
    assert_eq!(sniff_image(&fx::png(1, 1)), Some("image/png"));
    assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xDB]), Some("image/jpeg"));
    assert_eq!(sniff_image(b"GIF89a...."), Some("image/gif"));
    assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    assert_eq!(sniff_image(b"<svg xmlns=..."), None);
    assert_eq!(sniff_image(b""), None);
}

// ---- fuzz-style robustness -------------------------------------------

/// xorshift64*: deterministic, dependency-free.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Every parsed range must lie inside the input.
fn check_invariants(pdu: &[u8]) {
    if let Ok(Pdu::RetrieveConf(m)) = parse(pdu) {
        assert!(m.parts.len() <= MAX_PARTS);
        for p in &m.parts {
            assert!(p.offset.checked_add(p.len).is_some_and(|e| e <= pdu.len()));
            let _ = p.text(pdu);
            let _ = sniff_image(p.data(pdu));
        }
    }
}

#[test]
fn every_truncation_of_valid_pdus() {
    let (r, _) = sample_retrieve();
    let n = fx::notification_ind("T-1", "+1555/TYPE=PLMN", "http://m/x", 60);
    for pdu in [&r, &n] {
        for i in 0..pdu.len() {
            check_invariants(&pdu[..i]);
        }
        assert!(parse(pdu).is_ok());
    }
}

#[test]
fn random_garbage_never_panics() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..20_000 {
        let len = (rng.next() % 300) as usize;
        let mut buf: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        // Half the inputs start with a plausible header to get deeper.
        if rng.next().is_multiple_of(2) && buf.len() > 2 {
            buf[0] = 0x8C;
            buf[1] = if rng.next().is_multiple_of(2) {
                0x82
            } else {
                0x84
            };
        }
        check_invariants(&buf);
    }
}

#[test]
fn bit_flips_and_byte_mutations_never_panic() {
    let (base, _) = sample_retrieve();
    let notif = fx::notification_ind("T-9", "a@b.c", "http://mmsc/q", 3600);
    let mut rng = Rng(42);
    for base in [&base, &notif] {
        for _ in 0..20_000 {
            let mut m = base.clone();
            let edits = 1 + rng.next() % 4;
            for _ in 0..edits {
                let i = (rng.next() as usize) % m.len();
                match rng.next() % 3 {
                    0 => m[i] ^= 1 << (rng.next() % 8),
                    1 => m[i] = rng.next() as u8,
                    _ => {
                        m.truncate(i.max(1));
                    }
                }
                if m.is_empty() {
                    break;
                }
            }
            check_invariants(&m);
        }
    }
}
