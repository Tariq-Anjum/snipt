#![allow(dead_code)]

use serde_json::Value;
use sms_types::RawSms;

pub const TZ: i32 = 330;

pub fn fixture(name: &str) -> Value {
    let p = format!(
        "{}/../sms-engine/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

pub fn seed() -> (i64, Vec<RawSms>) {
    let fx = fixture("seed_raw.json");
    (
        fx["nowMs"].as_i64().unwrap(),
        serde_json::from_value(fx["messages"].clone()).unwrap(),
    )
}

pub fn json_close(got: &Value, want: &Value, path: &str) -> Result<(), String> {
    match (got, want) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0) {
                Ok(())
            } else {
                Err(format!("{path}: {a} != {b}"))
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: len {} != {}", a.len(), b.len()));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .try_for_each(|(i, (x, y))| json_close(x, y, &format!("{path}[{i}]")))
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut ka: Vec<_> = a.keys().collect();
            let mut kb: Vec<_> = b.keys().collect();
            ka.sort();
            kb.sort();
            if ka != kb {
                return Err(format!("{path}: keys {ka:?} != {kb:?}"));
            }
            ka.into_iter()
                .try_for_each(|k| json_close(&a[k], &b[k], &format!("{path}.{k}")))
        }
        (a, b) if a == b => Ok(()),
        (a, b) => Err(format!("{path}: {a} != {b}")),
    }
}
