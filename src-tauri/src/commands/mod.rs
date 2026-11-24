//! Tauri command surface (PLAN S8). Each command is a thin async wrapper over
//! a [`Core`](crate::core::Core) method run on a blocking thread.

use sms_crypto::EncryptedOtpPayload;
use sms_engine::budget::BudgetPlannerSummary;
use sms_engine::cleanup::{CleanupBucketId, WeeklyScanReport};
use sms_engine::inbox::ReminderItem;
use sms_engine::translate::TranslationResult;
use sms_store::{
    InboxCounts, IngestReport, MessageCursor, OutgoingSms, SendStatus, ThreadCursor, ThreadPage,
    ThreadRow,
};
use sms_types::{BudgetLimits, FinanceSummary, PaymentCycleConfig};
use std::collections::HashSet;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_telephony::TelephonyExt;

use tauri_plugin_telephony::models::{BackRequest, PowerState, ProviderIds, SendRequest};
use zeroize::Zeroizing;

use crate::core::{
    AppSettings, AppState, BackupFile, Capabilities, MessagesPage, OtpItemView, OutgoingResult,
    RestoreReport,
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

/// Provider rows (Android default-SMS app) changed successfully by a native
/// mark-read / delete call. Any failure (role lost, provider write refused,
/// IPC error) leaves the id out, so its local row stays as it was.
async fn provider_ok<R: Runtime>(
    app: &AppHandle<R>,
    ids: Vec<i64>,
    mms: bool,
    delete: bool,
) -> HashSet<i64> {
    if ids.is_empty() {
        return HashSet::new();
    }
    let res = native(app, move |a| {
        let req = ProviderIds {
            provider_ids: ids,
            mms,
        };
        if delete {
            a.telephony().provider_delete(req)
        } else {
            a.telephony().provider_mark_read(req)
        }
    })
    .await;
    match res {
        Some(Ok(r)) => r
            .results
            .into_iter()
            .filter(|x| x.ok)
            .map(|x| x.provider_id)
            .collect(),
        _ => HashSet::new(),
    }
}

/// Provider first (Android), then the store: a message whose provider row
/// could not be marked read stays unread locally and the call fails, so the
/// UI keeps it retryable instead of reporting a success the next default
/// SMS app would not see. Desktop has no provider.
#[tauri::command]
pub async fn mark_read<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    sms_id: i64,
) -> CmdResult<bool> {
    let (sms, mms) = state
        .run(move |c| Ok((c.provider_refs(&[sms_id])?, c.mms_refs(&[sms_id])?)))
        .await?;
    if cfg!(target_os = "android") {
        for (refs, is_mms) in [(sms, false), (mms, true)] {
            let want: Vec<i64> = refs.iter().map(|r| r.provider_id).collect();
            let ok = provider_ok(&app, want.clone(), is_mms, false).await;
            if !want.iter().all(|p| ok.contains(p)) {
                return Err(CmdError(
                    "Could not mark the message read in the system SMS store; try again.".into(),
                ));
            }
        }
    }
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

/// Per-row outcome of `delete_messages`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOutcome {
    pub deleted: usize,
    /// Requested messages that are still stored: their provider row could
    /// not be deleted, or a send hand-off was in progress (R4 #4). Visible
    /// and retryable.
    pub failed_ids: Vec<i64>,
}

/// Deletes the Telephony provider rows first (Android), then only those
/// store rows whose provider row is gone (or that have none). Failed rows
/// are reported per id and kept.
#[tauri::command]
pub async fn delete_messages<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<DeleteOutcome> {
    // Reserve first (R5 #3): a message mid-send is refused before its
    // provider row is touched, and a reserved one cannot start a send.
    let (ids, busy) = state.run(move |c| c.reserve_delete(&ids)).await?;
    let reserved = ids.clone();
    let out = delete_reserved(&app, &state, ids).await;
    let released = state
        .run(move |c| {
            c.release_delete(&reserved);
            Ok(())
        })
        .await;
    let mut out = out?;
    released?;
    out.failed_ids.extend(busy);
    Ok(out)
}

async fn delete_reserved<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<DeleteOutcome> {
    let lookup = ids.clone();
    let (sms, mms) = state
        .run(move |c| Ok((c.provider_refs(&lookup)?, c.mms_refs(&lookup)?)))
        .await?;
    let mut failed_ids = Vec::new();
    if cfg!(target_os = "android") {
        // SMS and MMS provider ids are separate namespaces: one call each.
        for (refs, is_mms) in [(sms, false), (mms, true)] {
            let ok = provider_ok(
                app,
                refs.iter().map(|r| r.provider_id).collect(),
                is_mms,
                true,
            )
            .await;
            failed_ids.extend(
                refs.iter()
                    .filter(|r| !ok.contains(&r.provider_id))
                    .map(|r| r.id),
            );
        }
    }
    let keep: HashSet<i64> = failed_ids.iter().copied().collect();
    let delete: Vec<i64> = ids.into_iter().filter(|i| !keep.contains(i)).collect();
    let (deleted, kept) = state
        .run(move |c| {
            let n = c.delete_messages(&delete)?;
            Ok((n, c.existing(&delete)?))
        })
        .await?;
    failed_ids.extend(kept);
    Ok(DeleteOutcome {
        deleted,
        failed_ids,
    })
}

/// Image bytes of an MMS attachment (thumbnail or full size). Only
/// allow-listed image types stored by the core are returned, as base64 for a
/// `blob:` URL: no file path or content URI ever reaches the webview.
#[tauri::command]
pub async fn mms_image(
    state: State<'_, AppState>,
    part_id: i64,
    thumb: bool,
) -> CmdResult<crate::core::MmsImage> {
    state.run(move |c| c.mms_image(part_id, thumb)).await
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

/// Dev simulator only (registered in debug / `dev-seed` builds): on
/// Android the receiver ingests natively over JNI, never through the webview.
#[cfg(any(debug_assertions, feature = "dev-seed"))]
#[tauri::command]
pub async fn ingest_incoming(
    state: State<'_, AppState>,
    message: sms_store::IncomingSms,
    provider_id: Option<i64>,
) -> CmdResult<crate::core::IncomingResult> {
    state
        .run(move |c| c.ingest_incoming(&message, provider_id))
        .await
}

/// Queue in the store, then hand the message to the radio (Android
/// `SmsManager` via the plugin, which opens a send attempt and binds its
/// provider row through the JNI core). Callbacks advance the state
/// monotonically; a rejected hand-off marks it `failed`. The result carries
/// the state persisted after the hand-off, not the initial `queued`.
#[tauri::command]
pub async fn queue_outgoing<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    message: OutgoingSms,
) -> CmdResult<OutgoingResult> {
    let sim_slot = message.sim_slot;
    let mut out = state.run(move |c| c.queue_outgoing(&message)).await?;
    let req = SendRequest {
        id: out.message.sms.id,
        address: out.message.sms.sender.clone(),
        body: out.message.sms.body.clone(),
        sim_slot,
    };
    let id = req.id;
    let sent = native(&app, move |a| a.telephony().send(req)).await;
    let persisted = state
        .run(move |c| {
            if !matches!(sent, Some(Ok(_))) {
                c.set_send_status(id, SendStatus::Failed)?;
            }
            c.send_status(id)
        })
        .await?;
    out.message.send_status = persisted;
    Ok(out)
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
pub async fn set_budget_period(
    state: State<'_, AppState>,
    period: crate::core::BudgetPeriod,
) -> CmdResult<crate::core::BudgetPeriod> {
    state.run(move |c| c.set_budget_period(period)).await
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
        mms_image,
        inbox_counts,
        reminders,
        set_reminder_done,
        get_settings,
        queue_outgoing,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        set_budget_period,
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
        ingest_incoming,
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
        mms_image,
        inbox_counts,
        reminders,
        set_reminder_done,
        get_settings,
        queue_outgoing,
        finance_summary,
        budget_stats,
        set_cycle,
        set_budget,
        set_budget_period,
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
