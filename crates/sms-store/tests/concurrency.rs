//! Two connections on one file DB (Android: Tauri state + JNI receiver).

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use common::*;
use sms_store::{IncomingSms, Store};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("mysms-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        TempDir(d)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn incoming(sender: &str, i: usize, ts: i64) -> IncomingSms {
    IncomingSms {
        sender: sender.into(),
        contact_name: None,
        body: format!("Your OTP is {:06}. Valid for 10 minutes.", 100_000 + i),
        is_known_contact: false,
        sim_slot: 0,
        protocol: None,
        timestamp_ms: Some(ts),
    }
}

/// R1 blocker: read-modify-write paths (ingest, purge) on two WAL connections
/// must not fail with SQLITE_BUSY_SNAPSHOT; every operation succeeds.
#[test]
fn two_connections_wal_read_modify_write() {
    let dir = TempDir::new("wal2");
    let path = dir.0.join("t.db");
    let (now, seed) = seed();
    Store::open(&path, TZ).unwrap().ingest_batch(&seed).unwrap();

    const N: usize = 150;
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = ["VM-ALPHA-S", "VM-BRAVO-S"]
        .into_iter()
        .enumerate()
        .map(|(w, sender)| {
            let (path, barrier) = (path.clone(), barrier.clone());
            thread::spawn(move || {
                let mut s = Store::open(&path, TZ).unwrap();
                barrier.wait();
                for i in 0..N {
                    s.ingest_incoming(&incoming(sender, i, now - 86_400_000), now)
                        .unwrap_or_else(|e| panic!("worker {w} ingest {i}: {e}"));
                    if i % 10 == 0 {
                        s.purge_expired_otps(now)
                            .unwrap_or_else(|e| panic!("worker {w} purge {i}: {e}"));
                    }
                }
            })
        })
        .collect();
    for h in workers {
        h.join().unwrap();
    }
    // every ingested OTP is a day old (expired) and is gone after a final
    // purge; insertion sequence numbers stay unique across both connections.
    let mut s = Store::open(&path, TZ).unwrap();
    s.purge_expired_otps(now).unwrap();
    let dup: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) - COUNT(DISTINCT ins_seq) FROM messages",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dup, 0);
    let left: i64 = s
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE sender IN ('VM-ALPHA-S', 'VM-BRAVO-S')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
}

/// R1 blocker: simultaneous first opens of a fresh file must not both run the
/// v0→v1 migration (`table messages already exists`).
#[test]
fn simultaneous_first_open_migrates_once() {
    for round in 0..10 {
        let dir = TempDir::new(&format!("open{round}"));
        let path = dir.0.join("t.db");
        const K: usize = 6;
        let barrier = Arc::new(Barrier::new(K));
        let opens: Vec<_> = (0..K)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    let s = Store::open(&path, TZ).map_err(|e| e.to_string())?;
                    s.schema_version().map_err(|e| e.to_string())
                })
            })
            .collect();
        for h in opens {
            assert_eq!(h.join().unwrap(), Ok(1), "round {round}");
        }
    }
}
