#![allow(dead_code)]

use serde_json::Value;
use sms_types::{Classification, RawSms};

pub const TOL: f64 = 1e-9;

pub fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

pub fn load(name: &str) -> Value {
    let text = std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|e| panic!("{name}: {e} (run scripts/gen-fixtures.ts)"));
    serde_json::from_str(&text).unwrap()
}

pub fn seed_fixture() -> Value {
    load("seed_raw.json")
}

pub fn seed_messages() -> Vec<RawSms> {
    serde_json::from_value(seed_fixture()["messages"].clone()).unwrap()
}

pub fn now_ms() -> i64 {
    seed_fixture()["nowMs"].as_i64().unwrap()
}

pub fn make_sms(sender: &str, body: &str) -> RawSms {
    RawSms {
        id: 1,
        thread_id: 10,
        sender: sender.into(),
        contact_name: None,
        body: body.into(),
        timestamp_ms: 1_763_600_000_000,
        is_known_contact: false,
        is_outgoing: false,
        sim_slot: 0,
        is_read: None,
        is_starred: None,
        protocol: None,
        rcs_status: None,
        rcs_rich_card: None,
    }
}

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

/// Categories/reasons exact; confidence and scores within 1e-9 (ARCHITECTURE §3).
pub fn classification_eq(got: &Classification, want: &Classification) -> Result<(), String> {
    if got.category != want.category || got.reason != want.reason {
        return Err(format!(
            "class {:?}/{:?} != {:?}/{:?}",
            got.category, got.reason, want.category, want.reason
        ));
    }
    if !close(got.confidence, want.confidence) {
        return Err(format!(
            "confidence {} != {}",
            got.confidence, want.confidence
        ));
    }
    for i in 0..5 {
        if !close(got.scores[i], want.scores[i]) {
            return Err(format!("scores {:?} != {:?}", got.scores, want.scores));
        }
    }
    Ok(())
}
