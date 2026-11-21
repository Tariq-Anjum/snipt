use sha2::{Digest, Sha256};
use sms_testdata::{create_seed_messages, generate_bulk_history_messages};
use sms_types::RawSms;

fn fixture(name: &str) -> serde_json::Value {
    let p = format!(
        "{}/../sms-engine/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn seed_equals_ts_fixture() {
    let fx = fixture("seed_raw.json");
    let now = fx["nowMs"].as_i64().unwrap();
    let want: Vec<RawSms> = serde_json::from_value(fx["messages"].clone()).unwrap();
    assert_eq!(create_seed_messages(now), want);
}

/// Port of TS test 6 + byte equality of the whole 10k archive with the TS generator.
#[test]
fn bulk_10k() {
    let fx = fixture("bulk_sample.json");
    let now = fx["nowMs"].as_i64().unwrap();
    let start = std::time::Instant::now();
    let archive = generate_bulk_history_messages(10_000, create_seed_messages(now), now);
    let elapsed = start.elapsed();
    println!(
        "bulk_10k: generated {} messages in {:?}",
        archive.len(),
        elapsed
    );
    assert_eq!(archive.len(), 10_000);
    assert_eq!(archive.len() as u64, fx["count"].as_u64().unwrap());
    assert!(elapsed.as_millis() < 1000, "expected <1s, got {elapsed:?}");

    let seed_count = fx["seedCount"].as_u64().unwrap() as usize;
    let first: Vec<RawSms> = serde_json::from_value(fx["firstGenerated"].clone()).unwrap();
    assert_eq!(&archive[seed_count..seed_count + first.len()], &first[..]);
    let last: Vec<RawSms> = serde_json::from_value(fx["lastGenerated"].clone()).unwrap();
    assert_eq!(&archive[archive.len() - last.len()..], &last[..]);

    let json = serde_json::to_string(&archive).unwrap();
    assert_eq!(
        hex(&Sha256::digest(json.as_bytes())),
        fx["archiveSha256"].as_str().unwrap(),
        "serde JSON of the archive must equal TS JSON.stringify byte-for-byte"
    );
}

#[test]
fn no_generation_when_target_met() {
    let seed = create_seed_messages(0);
    assert_eq!(generate_bulk_history_messages(10, seed.clone(), 0), seed);
}
