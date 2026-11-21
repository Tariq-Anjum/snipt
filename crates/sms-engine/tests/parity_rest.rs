//! Parity with the TS oracle: ports of engine.test.ts tests 4, 5, 9 plus fixture
//! equality for cycles, finance/budget, weekly scan, translation and the
//! store.ts selectors (filters, OTP items, reminders, purge).

mod common;

use common::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sms_engine::{
    SmsEngine,
    budget::{
        BudgetPlannerSummary, BudgetStatus, categorize_merchant_expense, compute_budget_stats,
        get_cycle_window, local_date, make_local,
    },
    cleanup::{CleanupBucketId, run_weekly_useless_scan},
    inbox::{
        InboxFilter, collect_otp_items, collect_reminders, compute_finance_summary, filter_threads,
        purge_expired_otp_messages,
    },
    thread::{ThreadState, build_threads},
    translate::{DetectedLanguageCode, detect_message_language, translate_sms_to_english},
};
use sms_testdata::{create_seed_messages, generate_bulk_history_messages};
use sms_types::*;

const TZ: i32 = 330;

fn threads_for(state: &str, seed: &[RawSms]) -> Vec<ThreadSummary> {
    let fx = load("threads.json");
    let st = &fx["states"][state];
    let archived: Vec<i64> = serde_json::from_value(st["archivedThreadIds"].clone()).unwrap();
    let blocked: Vec<String> = serde_json::from_value(st["blockedSenders"].clone()).unwrap();
    build_threads(
        seed,
        &ThreadState {
            archived_thread_ids: &archived,
            blocked_senders: &blocked,
        },
        &SmsEngine::new(),
    )
}

/// Port of TS test 4.
#[test]
fn cycles_and_budget() {
    let now = make_local(2026, 10, 21, 12, 0, 0, 0, TZ); // 21 Nov 2026 12:00 IST
    assert_eq!(now, now_ms());
    let cc = get_cycle_window(15.0, now, TZ);
    let (_, m, d) = local_date(cc.start_ms, TZ);
    assert_eq!(d, 15);
    assert_eq!(m - 1, 10);

    let seed = create_seed_messages(now);
    let threads = build_threads(&seed, &ThreadState::default(), &SmsEngine::new());
    let fin = compute_finance_summary(&threads, &PaymentCycleConfig::default(), now, true, TZ);
    assert!(fin.upi_debit_total > 3000.0);
    assert!(fin.credit_card_debit_total > 9000.0);
    assert_eq!(fin.savings_debit_total, 3200.0);

    let limits = BudgetLimits {
        food_dining: 5000.0,
        ..BudgetLimits::default()
    };
    let budget = compute_budget_stats(&fin.transactions, &limits, true);
    let food = budget
        .categories
        .iter()
        .find(|c| c.category == BudgetCategoryId::FoodDining)
        .unwrap();
    assert_eq!(food.status, BudgetStatus::OverBudget);
    assert!(food.spent > 5000.0);
}

#[test]
fn cycles_fixture_equal() {
    let fx = load("cycles.json");
    assert_eq!(fx["tzOffsetMin"].as_i64(), Some(i64::from(TZ)));
    let cases = fx["cases"].as_array().unwrap();
    assert!(cases.len() > 1000);
    for c in cases {
        let day = c["cycleStartDay"].as_f64().unwrap();
        let now = c["nowMs"].as_i64().unwrap();
        let w = get_cycle_window(day, now, TZ);
        assert_eq!(
            w.start_ms,
            c["startMs"].as_i64().unwrap(),
            "start day={day} now={now}"
        );
        assert_eq!(
            w.end_ms,
            c["endMs"].as_i64().unwrap(),
            "end day={day} now={now}"
        );
        assert_eq!(
            w.label,
            c["label"].as_str().unwrap(),
            "label day={day} now={now}"
        );
    }
}

#[test]
fn finance_and_budget_fixture_equal() {
    let fx = load("budget.json");
    let seed = seed_messages();
    let now = now_ms();
    let finances = fx["finances"].as_array().unwrap();
    let mut summaries = Vec::new();
    for (i, f) in finances.iter().enumerate() {
        let threads = threads_for(f["state"].as_str().unwrap(), &seed);
        let cfg: PaymentCycleConfig = serde_json::from_value(f["cycleConfig"].clone()).unwrap();
        let only = f["onlyActiveCycle"].as_bool().unwrap();
        let s = compute_finance_summary(&threads, &cfg, now, only, TZ);
        assert_json_close(&s, &f["summary"], &format!("finances[{i}]"));
        summaries.push(s);
    }
    for (i, b) in fx["budgets"].as_array().unwrap().iter().enumerate() {
        let s = &summaries[b["financeIndex"].as_u64().unwrap() as usize];
        let limits: BudgetLimits = serde_json::from_value(b["limits"].clone()).unwrap();
        let stats: BudgetPlannerSummary = compute_budget_stats(
            &s.transactions,
            &limits,
            b["onlyActiveCycle"].as_bool().unwrap(),
        );
        assert_json_close(&stats, &b["stats"], &format!("budgets[{i}]"));
    }
    for m in fx["merchants"].as_array().unwrap() {
        let got = categorize_merchant_expense(m["merchant"].as_str(), m["body"].as_str().unwrap());
        assert_eq!(got.as_str(), m["category"].as_str().unwrap(), "{m}");
    }
}

/// Port of TS test 5.
#[test]
fn weekly_scan_safety() {
    let now = now_ms();
    let seed = create_seed_messages(now);
    let engine = SmsEngine::new();
    let report = run_weekly_useless_scan(&seed, &[], now, &engine);
    assert!(report.total_useless_count >= 6);
    let ids = |id: CleanupBucketId| {
        report
            .buckets
            .iter()
            .find(|b| b.id == id)
            .unwrap()
            .message_ids
            .clone()
    };
    assert_eq!(ids(CleanupBucketId::ExpiredOtps).len(), 2);
    let promos = ids(CleanupBucketId::OldPromotions);
    assert_eq!(promos.len(), 3);
    assert_eq!(ids(CleanupBucketId::SpamPhishing).len(), 1);
    assert_eq!(ids(CleanupBucketId::OldDeliveries).len(), 1);

    let flagged: Vec<i64> = report
        .buckets
        .iter()
        .flat_map(|b| b.message_ids.clone())
        .collect();
    assert!(!flagged.contains(&1001)); // HDFC UPI
    assert!(!flagged.contains(&1008)); // Personal RCS
    // Never personal, transactions, starred or active reminders.
    for m in seed.iter().filter(|m| flagged.contains(&m.id)) {
        let a = engine.analyze(m);
        assert!(!m.starred() && !m.is_outgoing && !m.is_known_contact);
        assert!(a.transaction.is_none() || a.classification.category == Category::Spam);
        assert!(!matches!(
            a.reminder.as_ref().map(|r| r.kind),
            Some(ReminderKind::BillDue | ReminderKind::TravelPnr)
        ));
    }

    let kept: Vec<&RawSms> = seed.iter().filter(|m| !promos.contains(&m.id)).collect();
    assert_eq!(seed.len() - kept.len(), 3);
}

fn sha_ids(ids: &[i64]) -> String {
    let json = serde_json::to_string(ids).unwrap();
    Sha256::digest(json.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn scan_fixture_equal() {
    let fx = load("scan.json");
    let now = now_ms();
    let engine = SmsEngine::new();
    let seed = seed_messages();
    let r = run_weekly_useless_scan(&seed, &[], now, &engine);
    assert_json_close(&r, &fx["seed"], "seed");

    let blocked: Vec<String> =
        serde_json::from_value(fx["seedBlocked"]["blockedSenders"].clone()).unwrap();
    let r = run_weekly_useless_scan(&seed, &blocked, now, &engine);
    assert_json_close(&r, &fx["seedBlocked"]["report"], "seedBlocked");

    let archive = generate_bulk_history_messages(10_000, seed, now);
    let r = run_weekly_useless_scan(&archive, &[], now, &engine);
    let want = &fx["bulk10k"];
    assert_eq!(r.total_scanned, want["totalScanned"].as_i64().unwrap());
    assert_eq!(
        r.total_useless_count,
        want["totalUselessCount"].as_i64().unwrap()
    );
    assert_eq!(
        r.total_estimated_bytes,
        want["totalEstimatedBytes"].as_i64().unwrap()
    );
    for (b, w) in r.buckets.iter().zip(want["buckets"].as_array().unwrap()) {
        assert_eq!(b.message_ids.len() as u64, w["count"].as_u64().unwrap());
        assert_eq!(sha_ids(&b.message_ids), w["idsSha256"].as_str().unwrap());
        assert_eq!(b.estimated_bytes, w["estimatedBytes"].as_i64().unwrap());
        assert_json_close(&b.samples, &w["samples"], "bulk samples");
    }
}

/// Port of TS test 9.
#[test]
fn translate_seven() {
    use DetectedLanguageCode::*;
    assert_eq!(
        detect_message_language("Sent Rs.450.50 via UPI to Swiggy.").code,
        En
    );

    let hi = translate_sms_to_english("आपका OTP 482910 है। किसी के साथ साझा न करें। धन्यवाद।");
    assert_eq!(hi.language.code, Hi);
    assert!(hi.needs_translation);
    assert!(hi.translated_text.contains("Your OTP"));
    assert!(hi.translated_text.contains("482910"));
    assert!(hi.translated_text.contains("Do not share with anyone"));

    let ur = translate_sms_to_english(
        "السلام علیکم، میں گھر پہنچ گیا ہوں۔ کل شام 7 بجے ملتے ہیں۔ شکریہ۔",
    );
    assert_eq!(ur.language.code, Ur);
    assert_eq!(ur.language.is_rtl, Some(true));
    assert!(ur.translated_text.contains("reached home"));
    assert!(ur.translated_text.contains("Thank you"));

    let hg = translate_sms_to_english(
        "Bhai kal shaam 8 baje milte hain khana khane, paise bhej diye hain.",
    );
    assert_eq!(hg.language.code, Hinglish);
    assert!(
        hg.translated_text
            .to_lowercase()
            .contains("tomorrow evening")
    );
    assert!(hg.translated_text.to_lowercase().contains("sent the money"));

    let mr = translate_sms_to_english("तुमचे पार्सल आज संध्याकाळी 6 पर्यंत पोहोचेल. धन्यवाद.");
    assert_eq!(mr.language.code, Mr);
    assert!(mr.translated_text.contains("Your parcel"));

    let ta = translate_sms_to_english("உங்கள் ஆர்டர் இன்று மாலை டெலிவரி செய்யப்படும். நன்றி.");
    assert_eq!(ta.language.code, Ta);
    assert!(ta.translated_text.contains("Your order"));
    assert!(ta.translated_text.contains("will be delivered"));

    let te = translate_sms_to_english("మీ ఆర్డర్ ఈరోజు సాయంత్రం డెలివరీ అవుతుంది. ధన్యవాదాలు.");
    assert_eq!(te.language.code, Te);
    assert!(te.translated_text.contains("Your order"));

    let bn = translate_sms_to_english("আপনার অর্ডার আজ সন্ধ্যায় ডেলিভারি করা হবে। ধন্যবাদ।");
    assert_eq!(bn.language.code, Bn);
    assert!(bn.translated_text.contains("Your order"));
}

#[test]
fn translate_fixture_equal() {
    let fx = load("translate.json");
    let cases = fx["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    for c in cases {
        let text = c["text"].as_str().unwrap();
        let d = serde_json::to_value(detect_message_language(text)).unwrap();
        let r = serde_json::to_value(translate_sms_to_english(text)).unwrap();
        if d != c["detected"] || r != c["result"] {
            failures.push(format!("{text:?}\n  got  {r}\n  want {}", c["result"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn selectors_fixture_equal() {
    let fx = load("threads.json");
    let seed = seed_messages();
    let now = now_ms();
    let base = threads_for("base", &seed);
    let blocked = threads_for("blocked", &seed);
    let pick = |s: &str| if s == "base" { &base } else { &blocked };

    for c in fx["filters"].as_array().unwrap() {
        let filter = InboxFilter::parse(c["filter"].as_str().unwrap()).unwrap();
        let sim = c["sim"].as_u64().map(|v| v as u8);
        let got: Vec<i64> = filter_threads(
            pick(c["state"].as_str().unwrap()),
            filter,
            sim,
            c["query"].as_str().unwrap(),
        )
        .iter()
        .map(|t| t.thread_id)
        .collect();
        let want: Vec<i64> = serde_json::from_value(c["threadIds"].clone()).unwrap();
        assert_eq!(got, want, "{c}");
    }

    for state in ["base", "blocked"] {
        let items: Vec<Value> = collect_otp_items(pick(state), now)
            .iter()
            .map(|o| {
                serde_json::json!({"smsId": o.sms_id, "threadId": o.thread_id, "sender": o.sender,
                    "expiresAtMs": o.expires_at_ms, "isExpired": o.is_expired, "code": o.otp.code})
            })
            .collect();
        assert_eq!(Value::Array(items), fx["otpItems"][state], "otp {state}");

        let done: Vec<i64> =
            serde_json::from_value(fx["states"][state]["completedReminderSmsIds"].clone()).unwrap();
        let rems: Vec<Value> = collect_reminders(pick(state), &done)
            .iter()
            .map(|r| {
                serde_json::json!({"smsId": r.sms_id, "threadId": r.thread_id, "sender": r.sender,
                    "isDone": r.is_done, "kind": r.reminder.kind})
            })
            .collect();
        assert_eq!(
            Value::Array(rems),
            fx["reminders"][state],
            "reminders {state}"
        );
    }

    // Port of the RCS-filter / purge part of TS test 8 (in-memory).
    assert_eq!(filter_threads(&base, InboxFilter::Rcs, None, "").len(), 4);
    assert!(collect_otp_items(&base, now).iter().any(|o| o.is_expired));
    let (kept, purged) = purge_expired_otp_messages(&seed, now, &SmsEngine::new());
    assert_eq!(purged, 2);
    assert_eq!(purged as u64, fx["purge"]["purgedCount"].as_u64().unwrap());
    let purged_ids: Vec<i64> = seed
        .iter()
        .filter(|m| !kept.iter().any(|k| k.id == m.id))
        .map(|m| m.id)
        .collect();
    let want: Vec<i64> = serde_json::from_value(fx["purge"]["purgedIds"].clone()).unwrap();
    assert_eq!(purged_ids, want);
}
