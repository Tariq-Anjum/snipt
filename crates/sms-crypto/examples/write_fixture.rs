//! Writes a Rust-made `.mysms.enc` of the seed messages (writer default
//! iterations) for the node cross-check: `scripts/backup-xcheck.ts`.
//! Usage: cargo run -p sms-crypto --example write_fixture -- <out path>

use sms_crypto::backup::{
    CloudBackupProvider, MySmsBackupPayload, WRITER_ITERATIONS, create_encrypted_backup,
};
use sms_types::{BudgetLimits, PaymentCycleConfig};

/// Test passphrase constant shared with backup-xcheck.ts (not a secret).
const PASS: &str = "my-secret-pass-2026";

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: write_fixture <out path>");
    let now = 1_795_242_600_000;
    let payload = MySmsBackupPayload {
        schema_version: 1,
        app: "mySMS".into(),
        created_at_ms: now,
        provider: CloudBackupProvider::DevicePicker,
        messages: sms_testdata::create_seed_messages(now),
        archived_thread_ids: vec![110],
        blocked_senders: vec!["+918800291844".into()],
        completed_reminder_sms_ids: vec![1011],
        cycle_config: PaymentCycleConfig::default(),
        budget_limits: BudgetLimits::default(),
        scan_schedule_day: "Every Sunday · 9:00 AM".into(),
        auto_translate_to_english: false,
    };
    let t = std::time::Instant::now();
    let (env, serialized) =
        create_encrypted_backup(&payload, PASS, WRITER_ITERATIONS).expect("create backup");
    std::fs::write(&out, serialized).expect("write file");
    println!(
        "wrote {out}: messageCount={} iterations={} in {:?}",
        env.message_count,
        env.iterations,
        t.elapsed()
    );
}
