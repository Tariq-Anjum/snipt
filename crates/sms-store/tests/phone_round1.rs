//! Phone round 1 (real-device review): v5 re-analysis of stored messages and
//! the budget period / manual reset floor.

use sms_store::Store;
use sms_types::{PaymentCycleConfig, RawSms};

const TZ: i32 = 330;
// 2026-11-22 12:00 IST
const NOW: i64 = 1_795_336_200_000;
const DAY: i64 = 86_400_000;

fn raw(id: i64, thread_id: i64, ts: i64, sender: &str, body: &str) -> RawSms {
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
        is_read: Some(true),
        is_starred: Some(false),
        protocol: None,
        rcs_status: None,
        rcs_rich_card: None,
    }
}

const USD_SPEND: &str = "USD 5.54 spent using ICICI Bank Card XX1111 on 18-Nov-26 on OPENTOOL. Avl Limit: INR 95,221.00. If not you, call 1800 2662";

#[test]
fn v5_reanalyses_rows_stored_by_older_rules() {
    let dir = std::env::temp_dir().join(format!("snipt-r1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v5.db");
    let _ = std::fs::remove_file(&path);
    {
        let mut s = Store::open(&path, TZ).unwrap();
        s.ingest_batch(&[raw(1, 1, NOW - DAY, "BP-ICICIT-S", USD_SPEND)])
            .unwrap();
        // Simulate a v4 database written by the old rules: ₹95,221 debit.
        let c = s.connection();
        c.execute_batch(
            r#"UPDATE messages SET txn_json = '{"amount":95221.0,"currency":"INR","direction":"Debit","paymentMethod":"Credit Card","accountLast4":"1111","merchantOrVpa":null,"referenceId":null,"availableBalance":null,"isUpi":false}' WHERE id = 1;
               UPDATE transactions SET amount = 95221.0 WHERE msg_id = 1;
               PRAGMA user_version = 4;"#,
        )
        .unwrap();
    }
    let s = Store::open(&path, TZ).unwrap();
    assert_eq!(
        s.schema_version().unwrap(),
        sms_store::migrate::LATEST_VERSION
    );
    let fin = s
        .finance_summary(&PaymentCycleConfig::default(), NOW, false)
        .unwrap();
    assert_eq!(fin.transactions.len(), 1);
    let t = &fin.transactions[0].txn;
    assert_eq!((t.amount, t.currency.as_str()), (5.54, "USD"));
    assert_eq!(t.available_balance, Some(95_221.0));
    // Foreign spends are listed but not summed into INR totals.
    assert_eq!(fin.total_debits, 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn budget_period_and_manual_floor() {
    let mut s = Store::open_in_memory(TZ).unwrap();
    let debit = |id, ts| {
        raw(
            id,
            id,
            ts,
            "AX-HDFCBK-S",
            "Rs 100 debited from A/c XX4444 at CAFE on today",
        )
    };
    s.ingest_batch(&[
        debit(1, NOW - 40 * DAY),
        debit(2, NOW - 10 * DAY),
        debit(3, NOW - DAY),
    ])
    .unwrap();
    let cfg = PaymentCycleConfig::default();
    let limits = sms_types::BudgetLimits::default();
    // Budget month from the 1st: 10 and 1 days ago (Nov 12 / Nov 21) count.
    let b = s
        .budget_stats_period(&cfg, &limits, NOW, true, 1, None)
        .unwrap();
    assert_eq!(b.total_spent, 200.0);
    // Budget month from the 15th: only Nov 21.
    let b = s
        .budget_stats_period(&cfg, &limits, NOW, true, 15, None)
        .unwrap();
    assert_eq!(b.total_spent, 100.0);
    // Manual reset 5 days ago: only the last one.
    let b = s
        .budget_stats_period(&cfg, &limits, NOW, true, 1, Some(NOW - 5 * DAY))
        .unwrap();
    assert_eq!(b.total_spent, 100.0);
    // All history ignores both.
    let b = s
        .budget_stats_period(&cfg, &limits, NOW, false, 15, Some(NOW - 5 * DAY))
        .unwrap();
    assert_eq!(b.total_spent, 300.0);
    // Finance totals honour the floor in active mode only.
    let f = s
        .finance_summary_since(&cfg, NOW, true, Some(NOW - 5 * DAY))
        .unwrap();
    assert_eq!(f.total_debits, 100.0);
    let f = s
        .finance_summary_since(&cfg, NOW, false, Some(NOW - 5 * DAY))
        .unwrap();
    assert_eq!(f.total_debits, 300.0);
}

#[test]
fn duplicate_alerts_count_once() {
    let mut s = Store::open_in_memory(TZ).unwrap();
    let t = NOW - DAY;
    s.ingest_batch(&[
        // Same bank SMS delivered twice.
        raw(
            1,
            1,
            t,
            "AX-HDFCBK-S",
            "Rs 250 debited from A/c XX4444 at CAFE on 21-Nov. UPI Ref 123456789012",
        ),
        raw(
            2,
            1,
            t + 60_000,
            "AX-HDFCBK-S",
            "Rs 250 debited from A/c XX4444 at CAFE on 21-Nov. UPI Ref 123456789012",
        ),
        // Bank alert + merchant's own SMS for one purchase (different threads).
        raw(
            3,
            2,
            t + 3_600_000,
            "AX-HDFCBK-S",
            "Rs 999 debited from A/c XX4444 at SHOPX on 21-Nov. UPI Ref 223456789012",
        ),
        raw(
            4,
            3,
            t + 3_660_000,
            "VM-SHOPXX-S",
            "Rs 999 paid to SHOPX for order 55. Thanks",
        ),
        // Two real purchases of the same amount: different references, kept.
        raw(
            5,
            1,
            t + 7_200_000,
            "AX-HDFCBK-S",
            "Rs 70 debited from A/c XX4444 at VEND on 21-Nov. UPI Ref 323456789012",
        ),
        raw(
            6,
            1,
            t + 7_260_000,
            "AX-HDFCBK-S",
            "Rs 70 debited from A/c XX4444 at VEND on 21-Nov. UPI Ref 423456789012",
        ),
        // Same amount on two different cards: kept.
        raw(
            7,
            4,
            t + 9_000_000,
            "BP-ICICIT-S",
            "INR 500 spent using ICICI Bank Card XX1111 at FUEL",
        ),
        raw(
            8,
            5,
            t + 9_060_000,
            "AX-AXISBK-S",
            "INR 500 spent using Axis Bank Card XX2222 at FUEL",
        ),
    ])
    .unwrap();
    let f = s
        .finance_summary(&PaymentCycleConfig::default(), NOW, false)
        .unwrap();
    let ids: Vec<i64> = f.transactions.iter().map(|x| x.sms_id).collect();
    assert!(ids.contains(&1) && !ids.contains(&2), "{ids:?}");
    assert!(ids.contains(&3) && !ids.contains(&4), "{ids:?}");
    assert!(ids.contains(&5) && ids.contains(&6), "{ids:?}");
    assert!(ids.contains(&7) && ids.contains(&8), "{ids:?}");
    assert_eq!(f.total_debits, 250.0 + 999.0 + 70.0 * 2.0 + 500.0 * 2.0);
}
