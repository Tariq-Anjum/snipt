//! Tauri command surface (PLAN S8). Each command is a thin async wrapper over
//! a [`Core`](crate::core::Core) method run on a blocking thread.

use sms_crypto::EncryptedOtpPayload;
use sms_engine::budget::BudgetPlannerSummary;
use sms_engine::cleanup::{CleanupBucketId, WeeklyScanReport};
use sms_engine::inbox::ReminderItem;
use sms_engine::translate::TranslationResult;
use sms_store::{
    InboxCounts, IncomingSms, IngestReport, MessageCursor, OutgoingSms, ProviderRef, SendStatus,
    ThreadCursor, ThreadPage, ThreadRow,
};
use sms_types::{BudgetLimits, FinanceSummary, PaymentCycleConfig};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_telephony::TelephonyExt;
use tauri_plugin_telephony::models::{BackRequest, PowerState, ProviderIds, SendRequest};
use zeroize::Zeroizing;

use crate::core::{
    AppSettings, AppState, BackupFile, Capabilities, IncomingResult, MessagesPage, OtpItemView,
    OutgoingResult, RestoreReport,
};
use crate::error::{CmdError, CmdResult};
use crate::picker;

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

/// Run a blocking native (Kotlin) call off the async runtime. Native
/// failures are not fatal to the store operation that preceded them.
async fn native<R: Runtime, T: Send + 'static>(
    app: &AppHandle<R>,
    f: impl FnOnce(&AppHandle<R>) -> T + Send + 'static,
) -> Option<T> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .ok()
}

/// Store first, then the Telephony provider (Android default-SMS app; no-op
/// on desktop): the provider row of a stored message is marked read too.
#[tauri::command]
pub async fn mark_read<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    sms_id: i64,
) -> CmdResult<bool> {
    let (changed, refs) = state
        .run(move |c| Ok((c.mark_read(sms_id)?, c.provider_refs(&[sms_id])?)))
        .await?;
    if changed && !refs.is_empty() {
        let provider_ids = refs.iter().map(|r| r.provider_id).collect();
        native(&app, move |a| {
            a.telephony()
                .provider_mark_read(ProviderIds { provider_ids })
        })
        .await;
    }
    Ok(changed)
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

/// Deletes from the store and the matching Telephony provider rows.
#[tauri::command]
pub async fn delete_messages<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<usize> {
    let (n, refs) = state
        .run(move |c| {
            let refs = c.provider_refs(&ids)?;
            Ok((c.delete_messages(&ids)?, refs))
        })
        .await?;
    if !refs.is_empty() {
        let provider_ids = refs.iter().map(|r| r.provider_id).collect();
        native(&app, move |a| {
            a.telephony().provider_delete(ProviderIds { provider_ids })
        })
        .await;
    }
    Ok(n)
}

#[tauri::command]
pub async fn get_thread(
    state: State<'_, AppState>,
    thread_id: i64,
) -> CmdResult<Option<ThreadRow>> {
    state.run(move |c| c.get_thread(thread_id)).await
}

#[tauri::command]
pub async fn inbox_counts(state: State<'_, AppState>, sim: Option<u8>) -> CmdResult<InboxCounts> {
    state.run(move |c| c.inbox_counts(sim)).await
}

#[tauri::command]
pub async fn reminders(state: State<'_, AppState>) -> CmdResult<Vec<ReminderItem>> {
    state.run(|c| c.reminders()).await
}

#[tauri::command]
pub async fn set_reminder_done(
    state: State<'_, AppState>,
    sms_id: i64,
    done: bool,
) -> CmdResult<()> {
    state.run(move |c| c.set_reminder_done(sms_id, done)).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    state.run(|c| c.get_settings()).await
}

/// Platform receive path (Android receiver via the plugin; dev simulator).
#[tauri::command]
pub async fn ingest_incoming(
    state: State<'_, AppState>,
    message: IncomingSms,
    provider_id: Option<i64>,
) -> CmdResult<IncomingResult> {
    state
        .run(move |c| c.ingest_incoming(&message, provider_id))
        .await
}

/// Queue in the store, then hand the message to the radio (Android
/// `SmsManager` via the plugin). The send state advances through
/// `sending -> sent/failed -> delivered` from the native status receiver; a
/// rejected hand-off marks it `failed` at once.
#[tauri::command]
pub async fn queue_outgoing<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    message: OutgoingSms,
) -> CmdResult<OutgoingResult> {
    let sim_slot = message.sim_slot;
    let out = state.run(move |c| c.queue_outgoing(&message)).await?;
    let req = SendRequest {
        id: out.message.sms.id,
        address: out.message.sms.sender.clone(),
        body: out.message.sms.body.clone(),
        sim_slot,
    };
    let id = req.id;
    let sent = native(&app, move |a| a.telephony().send(req)).await;
    if !matches!(sent, Some(Ok(_))) {
        state
            .run(move |c| c.set_send_status(id, SendStatus::Failed, None))
            .await?;
    }
    Ok(out)
}

#[tauri::command]
pub async fn set_send_status(
    state: State<'_, AppState>,
    sms_id: i64,
    status: SendStatus,
    provider_id: Option<i64>,
) -> CmdResult<bool> {
    state
        .run(move |c| c.set_send_status(sms_id, status, provider_id))
        .await
}

#[tauri::command]
pub async fn provider_refs(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<Vec<ProviderRef>> {
    state.run(move |c| c.provider_refs(&ids)).await
}

#[tauri::command]
pub async fn provider_mark_read(
    state: State<'_, AppState>,
    provider_ids: Vec<i64>,
) -> CmdResult<usize> {
    state
        .run(move |c| c.provider_mark_read(&provider_ids))
        .await
}

#[tauri::command]
pub async fn provider_delete(
    state: State<'_, AppState>,
    provider_ids: Vec<i64>,
) -> CmdResult<usize> {
    state.run(move |c| c.provider_delete(&provider_ids)).await
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
pub async fn purge_expired_otps(state: State<'_, AppState>) -> CmdResult<usize> {
    state.run(|c| c.purge_expired_otps()).await
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

/// The passphrase `String` is moved into `Zeroizing` at once, so its buffer
/// is wiped when the closure drops it (on success, error or panic). Copies in
/// the IPC JSON payload owned by Tauri are outside this crate's control.
#[tauri::command]
pub async fn backup_create(
    state: State<'_, AppState>,
    passphrase: String,
) -> CmdResult<BackupFile> {
    let passphrase = Zeroizing::new(passphrase);
    state.run(move |c| c.backup_create(&passphrase)).await
}

#[tauri::command]
pub async fn backup_restore(
    state: State<'_, AppState>,
    content: String,
    passphrase: String,
) -> CmdResult<RestoreReport> {
    let passphrase = Zeroizing::new(passphrase);
    state
        .run(move |c| c.backup_restore(&content, &passphrase))
        .await
}

/// Outcome of the native "save backup" flow.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSaved {
    /// The user dismissed the picker; nothing was written.
    pub cancelled: bool,
    pub filename: String,
    pub message_count: usize,
}

/// Outcome of the native "restore backup" flow.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestored {
    pub cancelled: bool,
    pub name: String,
    pub report: Option<RestoreReport>,
}

/// Encrypt, then let the user pick the destination (SAF on Android, a save
/// dialog on Linux). The ciphertext goes core -> native writer and never
/// crosses the webview; the passphrase buffer is wiped when this returns.
#[tauri::command]
pub async fn backup_save<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    passphrase: String,
) -> CmdResult<BackupSaved> {
    let passphrase = Zeroizing::new(passphrase);
    let file = state.run(move |c| c.backup_create(&passphrase)).await?;
    let (name, count) = (file.filename.clone(), file.message_count);
    let content = Zeroizing::new(file.content);
    let written = native(&app, move |a| picker::save(a, &file.filename, &content))
        .await
        .ok_or_else(|| CmdError("backup picker failed".into()))??;
    Ok(BackupSaved {
        cancelled: !written,
        filename: name,
        message_count: count,
    })
}

/// Let the user pick a backup file, then restore it with `passphrase`.
/// A wrong passphrase surfaces as the core's decrypt error.
#[tauri::command]
pub async fn backup_restore_picked<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    passphrase: String,
) -> CmdResult<BackupRestored> {
    let passphrase = Zeroizing::new(passphrase);
    let picked = native(&app, picker::open)
        .await
        .ok_or_else(|| CmdError("backup picker failed".into()))??;
    let Some((name, content)) = picked else {
        return Ok(BackupRestored {
            cancelled: true,
            name: String::new(),
            report: None,
        });
    };
    let report = state
        .run(move |c| c.backup_restore(&content, &passphrase))
        .await?;
    Ok(BackupRestored {
        cancelled: false,
        name,
        report: Some(report),
    })
}

/// Build capabilities, updated on Android with the shell's live status
/// (default-SMS role, radio, SIM count). Desktop reports the build values.
#[tauri::command]
pub async fn capabilities<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> CmdResult<Capabilities> {
    if cfg!(target_os = "android")
        && let Some(Ok(s)) = native(&app, |a| a.telephony().status()).await
    {
        return state
            .run(move |c| {
                Ok(c.apply_platform_status(s.default_sms_role, s.can_send, s.sim_slots, s.mms))
            })
            .await;
    }
    state.run(|c| Ok(c.capabilities())).await
}

/// Predictive back: the web layer consumes the back gesture only while it
/// has an overlay or open thread (`back` event); otherwise the system
/// back-to-home animation runs. No-op on desktop.
#[tauri::command]
pub async fn set_back_handler<R: Runtime>(app: AppHandle<R>, enabled: bool) -> CmdResult<()> {
    native(&app, move |a| {
        a.telephony().set_back_handler(BackRequest { enabled })
    })
    .await;
    Ok(())
}

/// Android power-save and thermal status (UI switches to Crisp Solid).
#[tauri::command]
pub async fn power_state<R: Runtime>(app: AppHandle<R>) -> CmdResult<PowerState> {
    match native(&app, |a| a.telephony().power_state()).await {
        Some(Ok(p)) => Ok(p),
        _ => Ok(PowerState::default()),
    }
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
        get_thread,
        inbox_counts,
        reminders,
        set_reminder_done,
        get_settings,
        ingest_incoming,
        queue_outgoing,
        set_send_status,
        provider_refs,
        provider_mark_read,
        provider_delete,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        weekly_scan,
        apply_cleanup,
        purge_expired_otps,
        otp_items,
        vault_reveal,
        translate,
        backup_create,
        backup_restore,
        backup_save,
        backup_restore_picked,
        capabilities,
        set_back_handler,
        power_state,
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
        get_thread,
        inbox_counts,
        reminders,
        set_reminder_done,
        get_settings,
        ingest_incoming,
        queue_outgoing,
        set_send_status,
        provider_refs,
        provider_mark_read,
        provider_delete,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        weekly_scan,
        apply_cleanup,
        purge_expired_otps,
        otp_items,
        vault_reveal,
        translate,
        backup_create,
        backup_restore,
        backup_save,
        backup_restore_picked,
        capabilities,
        set_back_handler,
        power_state,
    ]);
    b
}
