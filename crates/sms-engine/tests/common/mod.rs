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

/// Structural JSON equality; numbers compared with relative tolerance 1e-9
/// (JS prints `3200`, serde prints `3200.0`; sums may differ in the last ulp).
pub fn json_close(got: &Value, want: &Value, path: &str) -> Result<(), String> {
    match (got, want) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() <= TOL * a.abs().max(b.abs()).max(1.0) {
                Ok(())
            } else {
                Err(format!("{path}: {a} != {b}"))
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: len {} != {}", a.len(), b.len()));
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                json_close(x, y, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut ka: Vec<&String> = a.keys().collect();
            let mut kb: Vec<&String> = b.keys().collect();
            ka.sort();
            kb.sort();
            if ka != kb {
                return Err(format!("{path}: keys {ka:?} != {kb:?}"));
            }
            for k in ka {
                json_close(&a[k], &b[k], &format!("{path}.{k}"))?;
            }
            Ok(())
        }
        (a, b) if a == b => Ok(()),
        (a, b) => Err(format!("{path}: {a} != {b}")),
    }
}

pub fn assert_json_close<T: serde::Serialize>(got: &T, want: &Value, ctx: &str) {
    let g = serde_json::to_value(got).unwrap();
    if let Err(e) = json_close(&g, want, ctx) {
        panic!("{e}");
    }
}
