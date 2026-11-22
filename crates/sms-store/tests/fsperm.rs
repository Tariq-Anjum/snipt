//! R2 #1: the plaintext SMS database is owner-only on Linux even with a
//! permissive umask. The checks run in a child process started with
//! `umask 022` (process-wide state; no libc/unsafe in tests either).

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use sms_store::{IncomingSms, Store, fsperm};

const CHILD_ENV: &str = "SNIPT_FSPERM_CHILD_DIR";

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

fn side(db: &Path, suffix: &str) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn write_one(db: &Path) {
    let mut s = Store::open(db, 0).unwrap();
    s.ingest_incoming(
        &IncomingSms {
            sender: "VM-HDFCBK".into(),
            contact_name: None,
            body: "Your OTP is 482913.".into(),
            is_known_contact: false,
            sim_slot: 0,
            protocol: None,
            timestamp_ms: Some(1_700_000_000_000),
        },
        1_700_000_000_000,
    )
    .unwrap();
    // Keep the connection open so -wal/-shm exist while modes are checked.
    let app = db.parent().unwrap();
    assert_eq!(mode(app), 0o700, "app dir");
    assert_eq!(mode(db), 0o600, "db");
    for sfx in ["-wal", "-shm"] {
        let p = side(db, sfx);
        assert!(p.exists(), "{sfx} missing");
        assert_eq!(mode(&p), 0o600, "{sfx}");
    }
    drop(s);
}

/// Child body: only runs when spawned by the parent test below.
#[test]
fn child_checks_modes_under_umask_022() {
    let Ok(root) = std::env::var(CHILD_ENV) else {
        return;
    };
    let root = PathBuf::from(root);

    // Fresh install.
    let app = root.join("fresh").join("app.snipt.sms");
    fsperm::create_private_dir(&app).unwrap();
    let db = app.join("snipt.db");
    write_one(&db);

    // Upgrade: an older build left a 0755 dir and 0644 db/wal/shm.
    let old = root.join("old").join("app.snipt.sms");
    std::fs::create_dir_all(&old).unwrap();
    let db = old.join("snipt.db");
    drop(Store::open(&db, 0).unwrap());
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();
    for p in [db.clone(), side(&db, "-wal"), side(&db, "-shm")] {
        if !p.exists() {
            std::fs::write(&p, b"").unwrap_or(());
        }
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    fsperm::create_private_dir(&old).unwrap();
    write_one(&db);
    println!("FSPERM-CHILD-OK");
}

#[test]
fn database_is_owner_only_with_umask_022() {
    if std::env::var(CHILD_ENV).is_ok() {
        return;
    }
    let root = std::env::temp_dir().join(format!("snipt-fsperm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let exe = std::env::current_exe().unwrap();
    let out = Command::new("sh")
        .arg("-c")
        .arg("umask 022 && exec \"$0\" --exact child_checks_modes_under_umask_022 --nocapture --test-threads=1")
        .arg(&exe)
        .env(CHILD_ENV, &root)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("FSPERM-CHILD-OK"),
        "child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
