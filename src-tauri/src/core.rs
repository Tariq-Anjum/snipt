//! Window-free application core: the store, the OTP vault and settings.
//! Every command is a thin async wrapper over a method of [`Core`], so the
//! command layer is testable without a Tauri window.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sms_crypto::{EncryptedOtpPayload, OtpVault};
use sms_engine::budget::BudgetPlannerSummary;
use sms_engine::cleanup::{CleanupBucketId, WeeklyScanReport};
use sms_engine::inbox::{ActiveOtpItem, InboxFilter, ReminderItem};
use sms_engine::otp::redact_otp_code;
use sms_engine::translate::{TranslationResult, translate_sms_to_english};
use sms_store::{
    InboxCounts, IncomingSms, IngestReport, MessageCursor, OutgoingSms, PartKind, PartOutcome,
    ProviderRef, SendStatus, SendUpdate, Store, ThreadCursor, ThreadPage, ThreadRow,
};
use sms_types::{BudgetLimits, FinanceSummary, ParsedSms, PaymentCycleConfig};

use crate::error::{CmdError, CmdResult};

/// Upper bounds on untrusted frontend input.
pub const MAX_QUERY_CHARS: usize = 256;
pub const MAX_IDS: usize = 20_000;
pub const MAX_TRANSLATE_CHARS: usize = 20_000;
pub const MAX_BACKUP_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_BODY_CHARS: usize = 20_000;
pub const MAX_SENDER_CHARS: usize = 128;
const DEFAULT_PAGE: usize = 50;

const KEY_CYCLE: &str = "cycle_config";
const KEY_BUDGET: &str = "budget_limits";

pub struct Core {
    store: Store,
    vault: OtpVault,
    /// Android: false until the Keystore-unwrapped key is installed
    /// ([`Core::set_vault_key`]); vault payloads are neither issued nor
    /// decrypted before that (they would be bound to a throwaway key). Linux
    /// uses an ephemeral per-session key by design and is ready at once.
    vault_ready: bool,
    caps: Capabilities,
}

/// Shared handle stored in Tauri state.
#[derive(Clone)]
pub struct AppState(Arc<Mutex<Core>>);

impl AppState {
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, CmdError> {
        let tz = local_tz_offset_min();
        let store = Store::open(db_path, tz)?;
        Ok(AppState(Arc::new(Mutex::new(Core::new(store)))))
    }

    pub fn from_core(core: Core) -> Self {
        AppState(Arc::new(Mutex::new(core)))
    }

    /// One process-wide core for the Tauri window and the JNI receiver path
    /// (Android): both open the same DB through one connection and one OTP
    /// vault. The first caller opens it; later callers get the same handle
    /// (the path of later calls is ignored, both sides pass the same file).
    pub fn shared(db_path: impl AsRef<Path>) -> Result<Self, CmdError> {
        static SHARED: OnceLock<AppState> = OnceLock::new();
        if let Some(s) = SHARED.get() {
            return Ok(s.clone());
        }
        let opened = Self::open(db_path)?;
        Ok(SHARED.get_or_init(|| opened).clone())
    }

    /// Synchronous exclusive access (JNI threads; never the async runtime).
    pub fn with_core<T>(&self, f: impl FnOnce(&mut Core) -> CmdResult<T>) -> CmdResult<T> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| CmdError("internal state poisoned".into()))?;
        f(&mut guard)
    }

    /// Run `f` on a blocking thread with exclusive access to the core.
    pub async fn run<T, F>(&self, f: F) -> CmdResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Core) -> CmdResult<T> + Send + 'static,
    {
        let me = self.0.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut guard = me
                .lock()
                .map_err(|_| CmdError("internal state poisoned".into()))?;
            f(&mut guard)
        })
        .await
        .map_err(|_| CmdError("background task failed".into()))?
    }
}

pub fn local_tz_offset_min() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A stored message plus its send state (outgoing messages queued through
/// `queue_outgoing`). Serialises as the flat `ParsedSms` JSON with an optional
/// `sendStatus` key, so the frontend type is `ParsedSms & { sendStatus? }`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    #[serde(flatten)]
    pub parsed: ParsedSms,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_status: Option<SendStatus>,
    /// Thread view: the OTP is masked in `sms.body` and `otp.code`; the code
    /// is only here, encrypted (decrypt with `vault_reveal`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otp_vault: Option<EncryptedOtpPayload>,
}

impl std::ops::Deref for MessageView {
    type Target = ParsedSms;
    fn deref(&self) -> &ParsedSms {
        &self.parsed
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessagesPage {
    pub messages: Vec<MessageView>,
    pub next: Option<MessageCursor>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub cycle_config: PaymentCycleConfig,
    pub budget_limits: BudgetLimits,
}

/// What the platform shows for a newly received message. `message` is
/// redacted: the body has the OTP masked and `otp.code` is bullets; the code
/// is only in `otpVault` (decrypt with `vault_reveal`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPayload {
    /// False for a duplicate delivery, a blocked sender or a Spam thread.
    pub notify: bool,
    pub title: String,
    pub message: ParsedSms,
    pub otp_vault: Option<EncryptedOtpPayload>,
    pub otp_expires_at_ms: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomingResult {
    /// False when the provider id was already stored (nothing ingested).
    pub inserted: bool,
    pub thread: ThreadRow,
    pub notification: NotificationPayload,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutgoingResult {
    pub message: MessageView,
    pub thread: ThreadRow,
}

/// Mask the OTP in a message that leaves the core without an explicit reveal.
pub fn redact_parsed(p: &mut ParsedSms) {
    if let Some(otp) = p.otp.as_mut() {
        p.sms.body = redact_otp_code(&p.sms.body, &otp.code);
        otp.code = "\u{2022}".repeat(otp.code.chars().count());
    }
}

/// OTP list entry: the plaintext code never crosses IPC in the list; the UI
/// asks `vault_reveal` with the encrypted payload for one item at a time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OtpItemView {
    pub item: ActiveOtpItem,
    pub vault: EncryptedOtpPayload,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub filename: String,
    pub content: String,
    pub message_count: usize,
}

/// A notification action's validated source message.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionTarget {
    pub msg_id: i64,
    pub thread_id: i64,
    pub provider_id: Option<i64>,
    pub is_read: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReport {
    pub deleted: usize,
    pub marked_read: usize,
    /// Provider inbox ids never ingested (and not deleted locally): replay.
    pub missing: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub ingest: IngestReport,
    pub archived_threads: usize,
    pub blocked_senders: usize,
}

/// Platform capabilities for the UI. The Linux build is a read-only backup
/// viewer (no radio): `canSend` is false and `queue_outgoing` is rejected.
/// On Android, `canSend` means the core accepts outgoing messages into the
/// send queue; the radio send, the default-SMS role, RCS/MMS and the SIM count
/// are reported by the telephony plugin (S12), so until it updates them they
/// stay at the conservative values below.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub platform: &'static str,
    pub can_send: bool,
    pub read_only: bool,
    pub default_sms_role: bool,
    pub rcs: bool,
    pub mms: bool,
    /// SIM slots the store accepts (0 and 1).
    pub sim_slots: u8,
    /// Native save/open picker for backups (SAF on Android, file dialog on
    /// Linux); false only in the dev browser, which uses download/file input.
    pub native_picker: bool,
}

pub fn platform_capabilities() -> Capabilities {
    let android = cfg!(target_os = "android");
    Capabilities {
        platform: if android { "android" } else { "linux" },
        can_send: android,
        read_only: !android,
        default_sms_role: false,
        rcs: false,
        mms: false,
        sim_slots: 2,
        native_picker: true,
    }
}

fn parse_filter(s: &str) -> CmdResult<InboxFilter> {
    InboxFilter::parse(s).ok_or_else(|| CmdError::invalid("unknown filter"))
}

fn check_sim(sim: Option<u8>) -> CmdResult<()> {
    match sim {
        Some(s) if s > 1 => Err(CmdError::invalid("sim must be 0 or 1")),
        _ => Ok(()),
    }
}

fn check_ids(ids: &[i64]) -> CmdResult<()> {
    if ids.len() > MAX_IDS {
        return Err(CmdError::invalid("too many ids"));
    }
    Ok(())
}

fn page_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_PAGE).clamp(1, 500)
}

fn valid_day(d: f64) -> bool {
    d.is_finite() && (1.0..=31.0).contains(&d)
}

fn check_text(s: &str, max: usize, what: &str) -> CmdResult<()> {
    if s.chars().count() > max {
        return Err(CmdError::invalid(&format!("{what} too long")));
    }
    Ok(())
}

impl Core {
    pub fn new(store: Store) -> Self {
        Self::with_capabilities(store, platform_capabilities())
    }

    /// Test seam: a core that behaves as another platform would.
    pub fn with_capabilities(store: Store, caps: Capabilities) -> Self {
        Core {
            store,
            vault: OtpVault::new_random(),
            vault_ready: caps.platform != "android",
            caps,
        }
    }

    pub fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }

    /// Android shell report (telephony plugin `status`): role, radio, SIMs.
    /// `canSend` can only be lowered from the build default, never raised
    /// (Linux stays read-only whatever a caller claims).
    pub fn apply_platform_status(
        &mut self,
        default_sms_role: bool,
        can_send: bool,
        sim_slots: u8,
        mms: bool,
    ) -> Capabilities {
        let base = platform_capabilities();
        self.caps.default_sms_role = default_sms_role;
        self.caps.can_send = base.can_send && can_send;
        self.caps.read_only = !self.caps.can_send;
        self.caps.sim_slots = sim_slots.clamp(1, 2);
        self.caps.mms = mms;
        self.caps.clone()
    }

    /// Install the persistent OTP-vault data key (Android: unwrapped by the
    /// Keystore in Kotlin). Payloads issued under the previous key stop
    /// decrypting, so this runs before the UI lists vault items.
    pub fn set_vault_key(&mut self, dek: [u8; 32]) {
        self.vault = OtpVault::from_key(dek);
        self.vault_ready = true;
    }

    pub fn vault_ready(&self) -> bool {
        self.vault_ready
    }

    fn require_vault(&self) -> CmdResult<()> {
        if self.vault_ready {
            Ok(())
        } else {
            Err(CmdError(
                "The code vault is still unlocking; try again.".into(),
            ))
        }
    }

    /// Plaintext OTP of one stored message, for the native "Copy OTP"
    /// notification action only (never sent over IPC).
    pub fn otp_code(&self, sms_id: i64) -> CmdResult<Option<String>> {
        Ok(self
            .store
            .message(sms_id)?
            .and_then(|m| m.otp.map(|o| o.code)))
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    // ---- settings ------------------------------------------------------

    fn load_setting<T: for<'de> Deserialize<'de> + Default>(&self, key: &str) -> CmdResult<T> {
        use rusqlite::OptionalExtension;
        let json: Option<String> = self
            .store
            .connection()
            .query_row("SELECT json FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(sms_store::StoreError::from)?;
        Ok(match json {
            // A corrupt row falls back to defaults rather than bricking the app.
            Some(j) => serde_json::from_str(&j).unwrap_or_default(),
            None => T::default(),
        })
    }

    fn save_setting<T: Serialize>(&self, key: &str, v: &T) -> CmdResult<()> {
        let json = serde_json::to_string(v).map_err(|_| CmdError("serialise failed".into()))?;
        self.store
            .connection()
            .execute(
                "INSERT INTO settings(key, json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET json = excluded.json",
                (key, json),
            )
            .map_err(sms_store::StoreError::from)?;
        Ok(())
    }

    pub fn cycle_config(&self) -> CmdResult<PaymentCycleConfig> {
        self.load_setting(KEY_CYCLE)
    }

    pub fn budget_limits(&self) -> CmdResult<BudgetLimits> {
        self.load_setting(KEY_BUDGET)
    }

    pub fn get_settings(&self) -> CmdResult<AppSettings> {
        Ok(AppSettings {
            cycle_config: self.cycle_config()?,
            budget_limits: self.budget_limits()?,
        })
    }

    pub fn set_cycle(&mut self, cfg: PaymentCycleConfig) -> CmdResult<PaymentCycleConfig> {
        if !(valid_day(cfg.upi) && valid_day(cfg.credit_card) && valid_day(cfg.savings_account)) {
            return Err(CmdError::invalid("cycle day must be 1..=31"));
        }
        self.save_setting(KEY_CYCLE, &cfg)?;
        Ok(cfg)
    }

    pub fn set_budget(&mut self, l: BudgetLimits) -> CmdResult<BudgetLimits> {
        let all = [
            l.food_dining,
            l.transport_travel,
            l.bills_utilities,
            l.shopping_retail,
            l.general_other,
        ];
        if all.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > 1e12) {
            return Err(CmdError::invalid("budget limit out of range"));
        }
        self.save_setting(KEY_BUDGET, &l)?;
        Ok(l)
    }

    // ---- inbox ---------------------------------------------------------

    pub fn threads_page(
        &self,
        filter: &str,
        sim: Option<u8>,
        after: Option<ThreadCursor>,
        limit: Option<usize>,
    ) -> CmdResult<ThreadPage> {
        check_sim(sim)?;
        Ok(self
            .store
            .threads_page(parse_filter(filter)?, sim, after, page_limit(limit))?)
    }

    pub fn messages_page(
        &self,
        thread_id: i64,
        before: Option<MessageCursor>,
        limit: Option<usize>,
    ) -> CmdResult<MessagesPage> {
        let (messages, next) = self
            .store
            .messages_page(thread_id, before, page_limit(limit))?;
        Ok(MessagesPage {
            messages: self.with_send_status(messages)?,
            next,
        })
    }

    fn with_send_status(&self, messages: Vec<ParsedSms>) -> CmdResult<Vec<MessageView>> {
        let outgoing: Vec<i64> = messages
            .iter()
            .filter(|m| m.sms.is_outgoing)
            .map(|m| m.sms.id)
            .collect();
        let status: std::collections::HashMap<i64, SendStatus> =
            self.store.send_statuses(&outgoing)?.into_iter().collect();
        Ok(messages
            .into_iter()
            .map(|mut parsed| {
                let otp_vault = parsed
                    .otp
                    .as_ref()
                    .filter(|_| self.vault_ready)
                    .map(|o| self.vault.encrypt_otp(&o.code));
                redact_parsed(&mut parsed);
                MessageView {
                    send_status: status.get(&parsed.sms.id).copied(),
                    otp_vault,
                    parsed,
                }
            })
            .collect())
    }

    pub fn get_thread(&self, thread_id: i64) -> CmdResult<Option<ThreadRow>> {
        Ok(self.store.thread(thread_id)?)
    }

    pub fn inbox_counts(&self, sim: Option<u8>) -> CmdResult<InboxCounts> {
        check_sim(sim)?;
        Ok(self.store.inbox_counts(sim)?)
    }

    pub fn search(
        &self,
        filter: &str,
        sim: Option<u8>,
        query: &str,
        limit: Option<usize>,
    ) -> CmdResult<Vec<ThreadRow>> {
        check_sim(sim)?;
        if query.chars().count() > MAX_QUERY_CHARS {
            return Err(CmdError::invalid("query too long"));
        }
        Ok(self
            .store
            .search(parse_filter(filter)?, sim, query, page_limit(limit))?)
    }

    pub fn mark_read(&mut self, sms_id: i64) -> CmdResult<bool> {
        Ok(self.store.mark_read(sms_id)?)
    }

    pub fn star(&mut self, sms_id: i64, starred: bool) -> CmdResult<bool> {
        Ok(self.store.set_starred(sms_id, starred)?)
    }

    pub fn archive(&mut self, thread_id: i64, archived: bool) -> CmdResult<()> {
        Ok(self.store.set_archived(thread_id, archived)?)
    }

    pub fn block(&mut self, sender: &str, blocked: bool) -> CmdResult<()> {
        if sender.trim().is_empty() || sender.chars().count() > 128 {
            return Err(CmdError::invalid("sender"));
        }
        Ok(self.store.set_blocked(sender, blocked)?)
    }

    pub fn delete_messages(&mut self, ids: &[i64]) -> CmdResult<usize> {
        check_ids(ids)?;
        Ok(self.store.delete_messages(ids)?)
    }

    // ---- reminders -----------------------------------------------------

    pub fn reminders(&self) -> CmdResult<Vec<ReminderItem>> {
        Ok(self.store.reminders()?)
    }

    pub fn set_reminder_done(&mut self, sms_id: i64, done: bool) -> CmdResult<()> {
        Ok(self.store.set_reminder_done(sms_id, done)?)
    }

    // ---- platform: receive, send queue, provider sync -------------------

    /// One message delivered by the platform (Android `SMS_DELIVER`, or the
    /// dev simulator): classify, store, and build the notification payload.
    pub fn ingest_incoming(
        &mut self,
        input: &IncomingSms,
        provider_id: Option<i64>,
    ) -> CmdResult<IncomingResult> {
        if input.sender.trim().is_empty() {
            return Err(CmdError::invalid("sender"));
        }
        check_text(&input.sender, MAX_SENDER_CHARS, "sender")?;
        check_text(&input.body, MAX_BODY_CHARS, "body")?;
        if let Some(c) = &input.contact_name {
            check_text(c, MAX_SENDER_CHARS, "contactName")?;
        }
        let (raw, inserted) = self
            .store
            .ingest_from_provider(input, provider_id, now_ms())?;
        let thread = self
            .store
            .thread(raw.thread_id)?
            .ok_or_else(|| CmdError("thread missing after ingest".into()))?;
        let mut message = self
            .store
            .message(raw.id)?
            .ok_or_else(|| CmdError("message missing after ingest".into()))?;
        let otp_vault = message
            .otp
            .as_ref()
            .filter(|_| self.vault_ready)
            .map(|o| self.vault.encrypt_otp(&o.code));
        let otp_expires_at_ms = message
            .otp
            .as_ref()
            .map(|o| sms_types::time::expiry_ms(message.sms.timestamp_ms, o.ttl_seconds));
        redact_parsed(&mut message);
        let notify = inserted
            && !thread.is_blocked
            && thread.classification.category != sms_types::Category::Spam;
        Ok(IncomingResult {
            inserted,
            notification: NotificationPayload {
                notify,
                title: thread.display_name.clone(),
                message,
                otp_vault,
                otp_expires_at_ms,
            },
            thread,
        })
    }

    /// Store an outgoing SMS as `queued`. Rejected where the platform cannot
    /// send (Linux is a read-only backup viewer).
    pub fn queue_outgoing(&mut self, out: &OutgoingSms) -> CmdResult<OutgoingResult> {
        if !self.caps.can_send {
            return Err(CmdError("sending is not available on this device".into()));
        }
        check_text(&out.body, MAX_BODY_CHARS, "body")?;
        if let Some(a) = &out.address {
            check_text(a, MAX_SENDER_CHARS, "address")?;
        }
        let raw = self.store.queue_outgoing(out, now_ms())?;
        let parsed = self
            .store
            .message(raw.id)?
            .ok_or_else(|| CmdError("message missing after queue".into()))?;
        let thread = self
            .store
            .thread(raw.thread_id)?
            .ok_or_else(|| CmdError("thread missing after queue".into()))?;
        Ok(OutgoingResult {
            message: MessageView {
                parsed,
                send_status: Some(SendStatus::Queued),
                otp_vault: None,
            },
            thread,
        })
    }

    /// Monotonic send-state move for failures before an attempt exists
    /// (rejected hand-off). Returns the persisted state.
    pub fn set_send_status(
        &mut self,
        sms_id: i64,
        status: SendStatus,
    ) -> CmdResult<Option<SendUpdate>> {
        Ok(self.store.set_send_status(sms_id, status)?)
    }

    /// Stored send state of one message (None: not outgoing).
    pub fn send_status(&self, sms_id: i64) -> CmdResult<Option<SendStatus>> {
        Ok(self
            .store
            .send_statuses(&[sms_id])?
            .into_iter()
            .next()
            .map(|(_, s)| s))
    }

    pub fn begin_send_attempt(&mut self, sms_id: i64, parts: u32) -> CmdResult<i64> {
        Ok(self.store.begin_send_attempt(sms_id, parts, now_ms())?)
    }

    pub fn bind_send_provider(&mut self, attempt_id: i64, provider_id: i64) -> CmdResult<bool> {
        Ok(self.store.bind_send_provider(attempt_id, provider_id)?)
    }

    pub fn record_send_part(
        &mut self,
        attempt_id: i64,
        part: u32,
        kind: PartKind,
        outcome: PartOutcome,
    ) -> CmdResult<Option<SendUpdate>> {
        Ok(self
            .store
            .record_send_part(attempt_id, part, kind, outcome)?)
    }

    /// Validates a notification action against the message it was posted
    /// for: the message must still exist (ids are never reused, so a deleted
    /// source never matches again) and its sender must equal `address`.
    pub fn action_target(&self, sms_id: i64, address: &str) -> CmdResult<Option<ActionTarget>> {
        let Some(m) = self.store.message(sms_id)? else {
            return Ok(None);
        };
        let want = sms_types::js::trim(address).to_lowercase();
        if want.is_empty() || m.sms.sender.to_lowercase() != want {
            return Ok(None);
        }
        let provider_id = self
            .store
            .provider_refs(&[sms_id])?
            .first()
            .map(|r| r.provider_id);
        Ok(Some(ActionTarget {
            msg_id: sms_id,
            thread_id: m.sms.thread_id,
            provider_id,
            is_read: m.sms.is_read.unwrap_or(false),
        }))
    }

    pub fn provider_refs(&self, ids: &[i64]) -> CmdResult<Vec<ProviderRef>> {
        check_ids(ids)?;
        Ok(self.store.provider_refs(ids)?)
    }

    pub fn provider_mark_read(&mut self, provider_ids: &[i64]) -> CmdResult<usize> {
        check_ids(provider_ids)?;
        Ok(self.store.provider_mark_read(provider_ids)?)
    }

    pub fn provider_delete(&mut self, provider_ids: &[i64]) -> CmdResult<usize> {
        check_ids(provider_ids)?;
        Ok(self.store.provider_delete(provider_ids)?)
    }

    /// Local `(id, providerId)` candidates; captured before the provider
    /// query of a reconcile pass (Android, on start/resume).
    pub fn provider_candidates(&self) -> CmdResult<Vec<ProviderRef>> {
        Ok(self.store.provider_candidates()?)
    }

    /// Provider -> store sync against the pre-query candidate set (see
    /// [`Store::provider_reconcile`]): a message received after the snapshot
    /// is never deleted; `missing` provider inbox rows are replayed by the
    /// caller.
    pub fn provider_reconcile(
        &mut self,
        candidates: &[ProviderRef],
        present: &[i64],
        read: &[i64],
        inbox: &[i64],
    ) -> CmdResult<ReconcileReport> {
        if candidates.len() > 10 * MAX_IDS
            || present.len() > 10 * MAX_IDS
            || read.len() > 10 * MAX_IDS
            || inbox.len() > 10 * MAX_IDS
        {
            return Err(CmdError::invalid("too many ids"));
        }
        let r = self
            .store
            .provider_reconcile(candidates, present, read, inbox)?;
        Ok(ReconcileReport {
            deleted: r.deleted,
            marked_read: r.marked_read,
            missing: r.missing,
        })
    }

    // ---- finance -------------------------------------------------------

    pub fn finance_summary(&self, only_active_cycle: bool) -> CmdResult<FinanceSummary> {
        let cfg = self.cycle_config()?;
        Ok(self
            .store
            .finance_summary(&cfg, now_ms(), only_active_cycle)?)
    }

    pub fn budget_stats(&self, only_active_cycle: bool) -> CmdResult<BudgetPlannerSummary> {
        let cfg = self.cycle_config()?;
        let limits = self.budget_limits()?;
        Ok(self
            .store
            .budget_stats(&cfg, &limits, now_ms(), only_active_cycle)?)
    }

    // ---- cleanup -------------------------------------------------------

    /// Weekly scan with OTP codes masked in the sample bodies (the scan
    /// itself is the TS-parity engine; masking happens at the IPC edge).
    pub fn weekly_scan(&self) -> CmdResult<WeeklyScanReport> {
        let mut report = self.store.weekly_scan(now_ms())?;
        for sample in report.buckets.iter_mut().flat_map(|b| b.samples.iter_mut()) {
            if let Some(code) = self
                .store
                .message(sample.sms_id)?
                .and_then(|m| m.otp)
                .map(|o| o.code)
            {
                sample.body = redact_otp_code(&sample.body, &code);
            }
        }
        Ok(report)
    }

    /// Deletes exactly the messages a fresh scan places in the chosen buckets;
    /// the frontend cannot name arbitrary message ids for cleanup.
    pub fn apply_cleanup(&mut self, buckets: &[CleanupBucketId]) -> CmdResult<usize> {
        let report = self.store.weekly_scan(now_ms())?;
        let ids: Vec<i64> = report
            .buckets
            .iter()
            .filter(|b| buckets.contains(&b.id))
            .flat_map(|b| b.message_ids.iter().copied())
            .collect();
        Ok(self.store.delete_messages(&ids)?)
    }

    // ---- OTP vault -----------------------------------------------------

    pub fn otp_items(&self) -> CmdResult<Vec<OtpItemView>> {
        self.require_vault()?;
        let items = self.store.otp_items(now_ms())?;
        Ok(items
            .into_iter()
            .map(|mut item| {
                let vault = self.vault.encrypt_otp(&item.otp.code);
                item.body = redact_otp_code(&item.body, &item.otp.code);
                item.otp.code = "\u{2022}".repeat(item.otp.code.chars().count());
                OtpItemView { item, vault }
            })
            .collect())
    }

    /// store.ts `purgeExpiredOtpMessages`: deletes OTP messages whose TTL
    /// (24 h for every extracted OTP) has passed, unless they also carry a
    /// transaction or reminder or are starred. Returns the number purged.
    pub fn purge_expired_otps(&mut self) -> CmdResult<usize> {
        Ok(self.store.purge_expired_otps(now_ms())?.len())
    }

    pub fn vault_reveal(&self, payload: &EncryptedOtpPayload) -> CmdResult<String> {
        self.require_vault()?;
        Ok(self.vault.decrypt_otp(payload)?.to_string())
    }

    // ---- translate -----------------------------------------------------

    pub fn translate(&self, text: &str) -> CmdResult<TranslationResult> {
        if text.chars().count() > MAX_TRANSLATE_CHARS {
            return Err(CmdError::invalid("text too long"));
        }
        Ok(translate_sms_to_english(text))
    }

    // ---- backup --------------------------------------------------------

    pub fn backup_create(&self, passphrase: &str) -> CmdResult<BackupFile> {
        let now = now_ms();
        let conn = self.store.connection();
        let ids = |sql: &str| -> CmdResult<Vec<i64>> {
            let mut st = conn.prepare(sql).map_err(sms_store::StoreError::from)?;
            let v = st
                .query_map([], |r| r.get(0))
                .map_err(sms_store::StoreError::from)?
                .collect::<Result<Vec<i64>, _>>()
                .map_err(sms_store::StoreError::from)?;
            Ok(v)
        };
        let payload = sms_crypto::MySmsBackupPayload {
            schema_version: 1,
            app: "mySMS".into(),
            created_at_ms: now,
            provider: sms_crypto::CloudBackupProvider::default(),
            messages: self.store.all_messages()?,
            archived_thread_ids: ids("SELECT thread_id FROM archived_threads ORDER BY thread_id")?,
            blocked_senders: self.store.blocked_senders()?,
            completed_reminder_sms_ids: ids("SELECT msg_id FROM reminders_done ORDER BY msg_id")?,
            cycle_config: self.cycle_config()?,
            budget_limits: self.budget_limits()?,
            scan_schedule_day: String::new(),
            auto_translate_to_english: false,
        };
        let message_count = payload.messages.len();
        let (_, content) = sms_crypto::create_encrypted_backup(
            &payload,
            passphrase,
            sms_crypto::backup::WRITER_ITERATIONS,
        )?;
        Ok(BackupFile {
            filename: sms_crypto::build_backup_filename(now, self.store.tz_offset_min),
            content,
            message_count,
        })
    }

    pub fn backup_restore(&mut self, content: &str, passphrase: &str) -> CmdResult<RestoreReport> {
        if content.len() > MAX_BACKUP_BYTES {
            return Err(CmdError::invalid("backup too large"));
        }
        let p = sms_crypto::restore_encrypted_backup(content, passphrase)?;
        let ingest = self.store.ingest_batch(&p.messages)?;
        let mut archived = 0;
        for t in &p.archived_thread_ids {
            self.store.set_archived(*t, true)?;
            archived += 1;
        }
        for s in &p.blocked_senders {
            if !s.trim().is_empty() {
                self.store.set_blocked(s, true)?;
            }
        }
        for id in &p.completed_reminder_sms_ids {
            self.store.set_reminder_done(*id, true)?;
        }
        if valid_day(p.cycle_config.upi)
            && valid_day(p.cycle_config.credit_card)
            && valid_day(p.cycle_config.savings_account)
        {
            self.save_setting(KEY_CYCLE, &p.cycle_config)?;
        }
        self.set_budget(p.budget_limits).ok();
        Ok(RestoreReport {
            ingest,
            archived_threads: archived,
            blocked_senders: p.blocked_senders.len(),
        })
    }

    // ---- dev -----------------------------------------------------------

    pub fn import_seed(&mut self) -> CmdResult<IngestReport> {
        if !seed_allowed() {
            return Err(CmdError("import_seed is disabled in this build".into()));
        }
        Ok(self
            .store
            .ingest_batch(&sms_testdata::seed::create_seed_messages(now_ms()))?)
    }
}

/// `import_seed` exists only in debug builds or with `--features dev-seed`.
pub const fn seed_allowed() -> bool {
    cfg!(debug_assertions) || cfg!(feature = "dev-seed")
}
