//! Capacity + latency targets (ARCHITECTURE §4) and keyset paging correctness
//! over 10,000 messages. Run: cargo test -p sms-store --release -- --nocapture

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::*;
use sms_engine::inbox::InboxFilter;
use sms_store::{MessageCursor, Store, ThreadCursor};
use sms_testdata::{create_seed_messages, generate_bulk_history_messages};

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
fn bench_10k() {
    let release = !cfg!(debug_assertions);
    let (now, _) = seed();
    let archive = generate_bulk_history_messages(10_000, create_seed_messages(now), now);
    assert_eq!(archive.len(), 10_000);

    let mut s = Store::open_in_memory(TZ).unwrap();
    let t = Instant::now();
    let rep = s.ingest_batch(&archive).unwrap();
    let ingest = t.elapsed();
    assert_eq!(rep.inserted, 10_000);
    println!("bench_10k: ingest 10,000 (classify + insert + threads) {ingest:?}");

    // file-backed (WAL) ingest, closer to the device path
    let dir = std::env::temp_dir().join(format!("mysms-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut f = Store::open(dir.join("b.db"), TZ).unwrap();
    let t = Instant::now();
    f.ingest_batch(&archive).unwrap();
    let ingest_file = t.elapsed();
    println!("bench_10k: ingest 10,000 into WAL file DB {ingest_file:?}");
    drop(f);
    std::fs::remove_dir_all(&dir).unwrap();

    let page = median(
        (0..50)
            .map(|_| {
                let t = Instant::now();
                let p = s.threads_page(InboxFilter::All, None, None, 50).unwrap();
                assert!(!p.threads.is_empty());
                t.elapsed()
            })
            .collect(),
    );
    let msgs = median(
        (0..50)
            .map(|_| {
                let t = Instant::now();
                let (m, _) = s.messages_page(201, None, 50).unwrap();
                assert_eq!(m.len(), 50);
                t.elapsed()
            })
            .collect(),
    );
    let mut search_times = Vec::new();
    for q in [
        "swiggy instamart",
        "kyc",
        "otp",
        "awb-40",
        "zz",
        "rcs aarav",
    ] {
        let t = Instant::now();
        let _ = s.search(InboxFilter::All, None, q, 50).unwrap();
        let d = t.elapsed();
        println!("bench_10k: search {q:?} {d:?}");
        search_times.push(d);
    }
    let search_max = *search_times.iter().max().unwrap();
    println!(
        "bench_10k: thread page (median of 50) {page:?}; message page {msgs:?}; search max {search_max:?}"
    );

    if release {
        assert!(ingest <= Duration::from_secs(2), "ingest {ingest:?} > 2 s");
        assert!(
            page <= Duration::from_millis(5),
            "thread page {page:?} > 5 ms"
        );
        assert!(
            search_max <= Duration::from_millis(20),
            "search {search_max:?} > 20 ms"
        );
    }

    // Keyset paging: thread list, no gaps/dupes.
    let mut seen = HashSet::new();
    let mut cursor: Option<ThreadCursor> = None;
    let mut all = Vec::new();
    loop {
        let p = s.threads_page(InboxFilter::All, None, cursor, 3).unwrap();
        for t in &p.threads {
            assert!(seen.insert(t.thread_id), "dup thread {}", t.thread_id);
            all.push(t.clone());
        }
        match p.next {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    let full = s
        .threads_page(InboxFilter::All, None, None, 500)
        .unwrap()
        .threads;
    assert_eq!(all, full);

    // Keyset paging over every message of every thread: covers all 10,000.
    let mut ids = HashSet::new();
    for t in s.all_threads().unwrap() {
        let mut cur: Option<MessageCursor> = None;
        let mut prev: Option<(i64, i64)> = None;
        loop {
            let (page, next) = s.messages_page(t.thread_id, cur, 37).unwrap();
            for m in &page {
                let key = (m.sms.timestamp_ms, m.sms.id);
                if let Some(p) = prev {
                    assert!(key < p, "order");
                }
                prev = Some(key);
                assert!(ids.insert(m.sms.id), "dup message {}", m.sms.id);
            }
            match next {
                Some(c) => cur = Some(c),
                None => break,
            }
        }
    }
    assert_eq!(ids.len(), 10_000);
}
