//! R1 blocker: extreme `i64` timestamps (e.g. from a crafted backup) must not
//! panic or wrap. Semantic assertions catch wrapping in release builds; run
//! `cargo test -p sms-engine --test timestamps` (debug) to also catch
//! overflow panics.

mod common;

use common::*;
use sms_engine::{
    SmsEngine,
    cleanup::run_weekly_useless_scan,
    inbox::{is_purgeable_expired_otp, purge_expired_otp_messages},
};
use sms_types::time::{MAX_TIMESTAMP_MS, expiry_ms, is_valid_timestamp_ms, local_date};

const OTP_BODY: &str = "482913 is your OTP for login. Valid for 10 minutes. Do not share it.";

fn otp_at(ts: i64) -> sms_types::RawSms {
    let mut m = make_sms("VM-SBIBNK-T", OTP_BODY);
    m.timestamp_ms = ts;
    m
}

#[test]
fn helpers_saturate_and_validate() {
    assert!(is_valid_timestamp_ms(MAX_TIMESTAMP_MS));
    assert!(is_valid_timestamp_ms(-MAX_TIMESTAMP_MS));
    assert!(!is_valid_timestamp_ms(MAX_TIMESTAMP_MS + 1));
    assert!(!is_valid_timestamp_ms(i64::MIN));
    assert!(!is_valid_timestamp_ms(i64::MAX));
    assert_eq!(expiry_ms(i64::MAX - 5, 600), i64::MAX);
    assert_eq!(expiry_ms(i64::MIN, -600), i64::MIN);
    assert_eq!(expiry_ms(1_000, i64::MAX), i64::MAX);
    let _ = local_date(i64::MAX, 330);
    let _ = local_date(i64::MIN, -720);
}

#[test]
fn extreme_timestamps_analyze_without_panic() {
    let engine = SmsEngine::new();
    for ts in [i64::MIN, i64::MIN + 1, -1, 0, i64::MAX - 1, i64::MAX] {
        let a = engine.analyze(&otp_at(ts));
        assert!(a.otp.is_some(), "otp body must parse at ts={ts}");
    }
}

#[test]
fn far_future_otp_never_wraps_into_expired() {
    let engine = SmsEngine::new();
    let now = now_ms();
    for ts in [i64::MAX, i64::MAX - 1_000, MAX_TIMESTAMP_MS] {
        let m = otp_at(ts);
        assert!(!is_purgeable_expired_otp(&m, &engine, now), "ts={ts}");
        let (kept, purged) = purge_expired_otp_messages(std::slice::from_ref(&m), now, &engine);
        assert_eq!((kept.len(), purged), (1, 0), "ts={ts}");
        let r = run_weekly_useless_scan([&m], &[], now, &engine);
        assert_eq!(r.total_useless_count, 0, "future OTP flagged at ts={ts}");
    }
}

#[test]
fn far_past_otp_counts_as_expired_without_overflow() {
    let engine = SmsEngine::new();
    let now = now_ms();
    for ts in [i64::MIN, i64::MIN + 1, -MAX_TIMESTAMP_MS] {
        let m = otp_at(ts);
        assert!(is_purgeable_expired_otp(&m, &engine, now), "ts={ts}");
        let r = run_weekly_useless_scan([&m], &[], now, &engine);
        assert_eq!(r.total_useless_count, 1, "ts={ts}");
    }
    // now itself at the extremes
    let m = otp_at(0);
    let _ = run_weekly_useless_scan([&m], &[], i64::MIN, &engine);
    let _ = run_weekly_useless_scan([&m], &[], i64::MAX, &engine);
}
