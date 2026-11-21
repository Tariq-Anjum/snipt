//! Parity with the TS oracle: ports of engine.test.ts tests 1, 2, 3, the FNV and
//! reminder parts of 8, plus fixture equality over analyze.json / threads.json.

mod common;

use common::*;
use serde_json::Value;
use sms_engine::{
    Analysis, SmsEngine,
    dlt::parse_sender,
    finance::extract_transaction,
    ml::fnv1a32,
    otp::extract_otp,
    reminder::extract_reminder,
    thread::{ThreadState, build_threads},
};
use sms_types::*;

#[test]
fn dlt_sender_parsing() {
    let dlt = |p: &str, e: &str, s: Option<DltSuffix>| SenderKind::DltHeader {
        prefix: p.into(),
        entity: e.into(),
        suffix: s,
    };
    assert_eq!(
        parse_sender("AX-HDFCBK-S", false),
        dlt("AX", "HDFCBK", Some(DltSuffix::Service))
    );
    assert_eq!(
        parse_sender("JM-ZOMATO-P", false),
        dlt("JM", "ZOMATO", Some(DltSuffix::Promotional))
    );
    assert_eq!(parse_sender("VM-AMAZON", false), dlt("VM", "AMAZON", None));
    assert_eq!(parse_sender("+919845012345", true), SenderKind::Contact);
    assert_eq!(
        parse_sender("+919845012345", false),
        SenderKind::PhoneNumber
    );
    assert_eq!(parse_sender("567678", false), SenderKind::Shortcode);
}

#[test]
fn otp_extractor() {
    let sbi = parse_sender("VM-SBIBNK-T", false);
    let otp = extract_otp(
        "849201 is your OTP for transaction of Rs. 2,500 on SBI Card ending 9912. Valid for 5 mins.",
        &sbi,
    )
    .expect("otp");
    assert_eq!(otp.code, "849201");
    assert_eq!(otp.service_hint.as_deref(), Some("SBIBNK"));

    let g = extract_otp(
        "G-392814 is your Google verification code.",
        &parse_sender("AD-GOOGLE-T", false),
    )
    .expect("g otp");
    assert_eq!(g.code, "392814");

    assert_eq!(
        extract_otp("Your card ending 9912 was charged Rs. 2500 in 2026.", &sbi),
        None
    );
}

#[test]
fn finance_methods() {
    let upi = extract_transaction(
        "Sent Rs.450.50 From HDFC Bank A/C *4821 To SWIGGY INSTAMART On 20/11/26 Ref 632418902311. Avl Bal Rs.12,340.25",
    )
    .expect("upi");
    assert_eq!(upi.payment_method, PaymentMethod::Upi);
    assert!(upi.is_upi);

    let cc = extract_transaction(
        "Spent Rs.5,400.00 on ICICI Bank Coral Credit Card XX7104 at BLUE TOKAI ROASTERS on 19-Nov-26. Avl Lmt Rs.1,84,600.00.",
    )
    .expect("cc");
    assert_eq!(cc.payment_method, PaymentMethod::CreditCard);
    assert_eq!(cc.available_balance, Some(184_600.0));

    let sav = extract_transaction(
        "Rs.3,200.00 debited from ICICI Bank Savings A/C XX7104 towards BESCOM ELECTRICITY via NEFT on 17-Nov-26. Avl Bal Rs.1,15,270.00.",
    )
    .expect("savings");
    assert_eq!(sav.payment_method, PaymentMethod::SavingsAccount);
    assert!(!sav.is_upi);

    assert_eq!(
        extract_transaction(
            "Pre-approved Rs. 5,00,000 instant loan! Credited in 5 mins. Apply now!"
        ),
        None
    );
}

#[test]
fn reminder_fnv() {
    assert_eq!(fnv1a32(""), 0x811c_9dc5);
    assert_eq!(fnv1a32("foobar"), 0xbf9c_f968);

    let pnr = extract_reminder("PNR: 4521890341 Train: 12952 RAJDHANI EXP.").expect("pnr");
    assert_eq!(pnr.reference.as_deref(), Some("4521890341"));

    let engine = SmsEngine::new();
    let promo = engine.analyze_message(&make_sms(
        "JM-ZOMATO-P",
        "Flat 60% OFF tonight! Order https://zoma.to/deal",
    ));
    assert_eq!(promo.classification.category, Category::Promotions);
}

#[test]
fn fnv_fixture_utf16_low_bytes() {
    let fx = load("analyze.json");
    let cases = fx["fnv"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let s = c["s"].as_str().unwrap();
        assert_eq!(
            u64::from(fnv1a32(s)),
            c["h"].as_u64().unwrap(),
            "fnv({s:?})"
        );
    }
}

#[test]
fn analyze_fixture_equal() {
    let fx = load("analyze.json");
    let engine = SmsEngine::new();
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() >= 2027);
    let mut failures = Vec::new();
    for c in cases {
        let sms: RawSms = serde_json::from_value(c["input"].clone()).unwrap();
        let want: Analysis = serde_json::from_value(c["output"].clone()).unwrap();
        let got = engine.analyze(&sms);
        if let Err(e) = analysis_eq(&got, &want) {
            failures.push(format!("id={} body={:?}: {e}", sms.id, sms.body));
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {} mismatches:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

fn analysis_eq(got: &Analysis, want: &Analysis) -> Result<(), String> {
    if got.sender_kind != want.sender_kind {
        return Err(format!(
            "senderKind {:?} != {:?}",
            got.sender_kind, want.sender_kind
        ));
    }
    classification_eq(&got.classification, &want.classification)?;
    if got.otp != want.otp {
        return Err(format!("otp {:?} != {:?}", got.otp, want.otp));
    }
    if got.transaction != want.transaction {
        return Err(format!(
            "txn {:?} != {:?}",
            got.transaction, want.transaction
        ));
    }
    if got.reminder != want.reminder {
        return Err(format!(
            "reminder {:?} != {:?}",
            got.reminder, want.reminder
        ));
    }
    Ok(())
}

#[test]
fn threads_fixture_equal() {
    let seed = seed_messages();
    let fx = load("threads.json");
    let engine = SmsEngine::new();
    for state_name in ["base", "blocked"] {
        let st = &fx["states"][state_name];
        let archived: Vec<i64> = serde_json::from_value(st["archivedThreadIds"].clone()).unwrap();
        let blocked: Vec<String> = serde_json::from_value(st["blockedSenders"].clone()).unwrap();
        let threads = build_threads(
            &seed,
            &ThreadState {
                archived_thread_ids: &archived,
                blocked_senders: &blocked,
            },
            &engine,
        );
        let want = fx["threads"][state_name].as_array().unwrap();
        assert_eq!(threads.len(), want.len(), "{state_name}: thread count");
        for (t, w) in threads.iter().zip(want) {
            assert_thread_row(t, w, state_name);
        }
    }
}

fn assert_thread_row(t: &ThreadSummary, w: &Value, ctx: &str) {
    let ctx = format!("{ctx} thread {}", t.thread_id);
    assert_eq!(t.thread_id, w["threadId"].as_i64().unwrap(), "{ctx}");
    assert_eq!(t.sender, w["sender"].as_str().unwrap(), "{ctx}");
    assert_eq!(t.display_name, w["displayName"].as_str().unwrap(), "{ctx}");
    let kind: SenderKind = serde_json::from_value(w["senderKind"].clone()).unwrap();
    assert_eq!(t.sender_kind, kind, "{ctx}");
    assert_eq!(
        t.latest_message.sms.id,
        w["latestMessageId"].as_i64().unwrap(),
        "{ctx}"
    );
    let ids: Vec<i64> = t.messages.iter().map(|m| m.sms.id).collect();
    let want_ids: Vec<i64> = serde_json::from_value(w["messageIds"].clone()).unwrap();
    assert_eq!(ids, want_ids, "{ctx}");
    let cls: Classification = serde_json::from_value(w["classification"].clone()).unwrap();
    classification_eq(&t.classification, &cls).unwrap_or_else(|e| panic!("{ctx}: {e}"));
    assert_eq!(t.unread_count, w["unreadCount"].as_i64().unwrap(), "{ctx}");
    assert_eq!(t.has_starred, w["hasStarred"].as_bool().unwrap(), "{ctx}");
    assert_eq!(t.is_archived, w["isArchived"].as_bool().unwrap(), "{ctx}");
    assert_eq!(t.is_blocked, w["isBlocked"].as_bool().unwrap(), "{ctx}");
    assert_eq!(
        i64::from(t.sim_slot),
        w["simSlot"].as_i64().unwrap(),
        "{ctx}"
    );
    assert_eq!(
        t.is_rcs_capable,
        w["isRcsCapable"].as_bool().unwrap(),
        "{ctx}"
    );
}
