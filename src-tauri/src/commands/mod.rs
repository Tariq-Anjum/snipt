//! Tauri command surface (PLAN S8). Each command is a thin async wrapper over
//! a [`Core`](crate::core::Core) method run on a blocking thread.

use sms_crypto::EncryptedOtpPayload;
use sms_engine::budget::BudgetPlannerSummary;
use sms_engine::cleanup::{CleanupBucketId, WeeklyScanReport};
use sms_engine::translate::TranslationResult;
use sms_store::{IngestReport, MessageCursor, ThreadCursor, ThreadPage, ThreadRow};
use sms_types::{BudgetLimits, FinanceSummary, PaymentCycleConfig};
use tauri::State;

use crate::core::{
    AppState, BackupFile, Capabilities, MessagesPage, OtpItemView, RestoreReport,
    platform_capabilities,
};
use crate::error::CmdResult;

#[tauri::command]
pub async fn threads_page(
    state: State<'_, AppState>,
    filter: String,
    sim: Option<u8>,
    after: Option<ThreadCursor>,
    limit: Option<usize>,
) -> CmdResult<ThreadPage> {
    state
        .run(move |c| c.threads_page(&filter, sim, after, limit))
        .await
}

#[tauri::command]
pub async fn messages_page(
    state: State<'_, AppState>,
    thread_id: i64,
    before: Option<MessageCursor>,
    limit: Option<usize>,
) -> CmdResult<MessagesPage> {
    state
        .run(move |c| c.messages_page(thread_id, before, limit))
        .await
}

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    filter: String,
    sim: Option<u8>,
    query: String,
    limit: Option<usize>,
) -> CmdResult<Vec<ThreadRow>> {
    state
        .run(move |c| c.search(&filter, sim, &query, limit))
        .await
}

#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, sms_id: i64) -> CmdResult<bool> {
    state.run(move |c| c.mark_read(sms_id)).await
}

#[tauri::command]
pub async fn star(state: State<'_, AppState>, sms_id: i64, starred: bool) -> CmdResult<bool> {
    state.run(move |c| c.star(sms_id, starred)).await
}

#[tauri::command]
pub async fn archive(state: State<'_, AppState>, thread_id: i64, archived: bool) -> CmdResult<()> {
    state.run(move |c| c.archive(thread_id, archived)).await
}

#[tauri::command]
pub async fn block(state: State<'_, AppState>, sender: String, blocked: bool) -> CmdResult<()> {
    state.run(move |c| c.block(&sender, blocked)).await
}

#[tauri::command]
pub async fn delete_messages(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<usize> {
    state.run(move |c| c.delete_messages(&ids)).await
}

#[tauri::command]
pub async fn finance_summary(
    state: State<'_, AppState>,
    only_active_cycle: bool,
) -> CmdResult<FinanceSummary> {
    state
        .run(move |c| c.finance_summary(only_active_cycle))
        .await
}

#[tauri::command]
pub async fn budget_stats(
    state: State<'_, AppState>,
    only_active_cycle: bool,
) -> CmdResult<BudgetPlannerSummary> {
    state.run(move |c| c.budget_stats(only_active_cycle)).await
}

#[tauri::command]
pub async fn set_cycle(
    state: State<'_, AppState>,
    config: PaymentCycleConfig,
) -> CmdResult<PaymentCycleConfig> {
    state.run(move |c| c.set_cycle(config)).await
}

#[tauri::command]
pub async fn set_budget(
    state: State<'_, AppState>,
    limits: BudgetLimits,
) -> CmdResult<BudgetLimits> {
    state.run(move |c| c.set_budget(limits)).await
}

#[tauri::command]
pub async fn weekly_scan(state: State<'_, AppState>) -> CmdResult<WeeklyScanReport> {
    state.run(|c| c.weekly_scan()).await
}

#[tauri::command]
pub async fn apply_cleanup(
    state: State<'_, AppState>,
    buckets: Vec<CleanupBucketId>,
) -> CmdResult<usize> {
    state.run(move |c| c.apply_cleanup(&buckets)).await
}

#[tauri::command]
pub async fn otp_items(state: State<'_, AppState>) -> CmdResult<Vec<OtpItemView>> {
    state.run(|c| c.otp_items()).await
}

#[tauri::command]
pub async fn vault_reveal(
    state: State<'_, AppState>,
    payload: EncryptedOtpPayload,
) -> CmdResult<String> {
    state.run(move |c| c.vault_reveal(&payload)).await
}

#[tauri::command]
pub async fn translate(state: State<'_, AppState>, text: String) -> CmdResult<TranslationResult> {
    state.run(move |c| c.translate(&text)).await
}

#[tauri::command]
pub async fn backup_create(
    state: State<'_, AppState>,
    passphrase: String,
) -> CmdResult<BackupFile> {
    state.run(move |c| c.backup_create(&passphrase)).await
}

#[tauri::command]
pub async fn backup_restore(
    state: State<'_, AppState>,
    content: String,
    passphrase: String,
) -> CmdResult<RestoreReport> {
    state
        .run(move |c| c.backup_restore(&content, &passphrase))
        .await
}

#[tauri::command]
pub async fn capabilities() -> CmdResult<Capabilities> {
    Ok(platform_capabilities())
}

/// Dev/debug only: rejects unless `debug_assertions` or `dev-seed` is on.
#[tauri::command]
pub async fn import_seed(state: State<'_, AppState>) -> CmdResult<IngestReport> {
    state.run(|c| c.import_seed()).await
}

pub fn register<R: tauri::Runtime>(b: tauri::Builder<R>) -> tauri::Builder<R> {
    #[cfg(any(debug_assertions, feature = "dev-seed"))]
    let b = b.invoke_handler(tauri::generate_handler![
        threads_page,
        messages_page,
        search,
        mark_read,
        star,
        archive,
        block,
        delete_messages,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        weekly_scan,
        apply_cleanup,
        otp_items,
        vault_reveal,
        translate,
        backup_create,
        backup_restore,
        capabilities,
        import_seed,
    ]);
    #[cfg(not(any(debug_assertions, feature = "dev-seed")))]
    let b = b.invoke_handler(tauri::generate_handler![
        threads_page,
        messages_page,
        search,
        mark_read,
        star,
        archive,
        block,
        delete_messages,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        weekly_scan,
        apply_cleanup,
        otp_items,
        vault_reveal,
        translate,
        backup_create,
        backup_restore,
        capabilities,
    ]);
    b
}
