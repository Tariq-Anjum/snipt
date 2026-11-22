// Explicit application ACL (R2 #7). With an app manifest, Tauri checks every
// app command against the capabilities, so only commands granted there
// (`permissions/ui.toml`, set "ui") are callable from the webview. Native-only
// paths (receivers, send callbacks, provider sync) go through the JNI core
// and have no webview command at all.
const COMMANDS: &[&str] = &[
    "threads_page",
    "messages_page",
    "search",
    "mark_read",
    "star",
    "archive",
    "block",
    "delete_messages",
    "get_thread",
    "inbox_counts",
    "reminders",
    "set_reminder_done",
    "get_settings",
    "queue_outgoing",
    "finance_summary",
    "budget_stats",
    "set_cycle",
    "set_budget",
    "weekly_scan",
    "apply_cleanup",
    "purge_expired_otps",
    "otp_items",
    "vault_reveal",
    "translate",
    "backup_create",
    "backup_restore",
    "backup_save",
    "backup_restore_picked",
    "capabilities",
    "set_back_handler",
    "power_state",
    // Registered only in debug / `dev-seed` builds (dev simulator, sample data).
    "import_seed",
    "ingest_incoming",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed");
}
