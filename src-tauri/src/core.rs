//! Window-free application core: the store, the OTP vault and settings.
//! Every command is a thin async wrapper over a method of [`Core`], so the
//! command layer is testable without a Tauri window.

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sms_crypto::{EncryptedOtpPayload, OtpVault};
use sms_engine::budget::BudgetPlannerSummary;
use sms_engine::cleanup::{CleanupBucketId, WeeklyScanReport};
use sms_engine::inbox::{ActiveOtpItem, InboxFilter};
use sms_engine::translate::{TranslationResult, translate_sms_to_english};
use sms_store::{IngestReport, MessageCursor, Store, ThreadCursor, ThreadPage, ThreadRow};
use sms_types::{BudgetLimits, FinanceSummary, ParsedSms, PaymentCycleConfig};

use crate::error::{CmdError, CmdResult};

/// Upper bounds on untrusted frontend input.
pub const MAX_QUERY_CHARS: usize = 256;
pub const MAX_IDS: usize = 20_000;
pub const MAX_TRANSLATE_CHARS: usize = 20_000;
pub const MAX_BACKUP_BYTES: usize = 256 * 1024 * 1024;
const DEFAULT_PAGE: usize = 50;

const KEY_CYCLE: &str = "cycle_config";
const KEY_BUDGET: &str = "budget_limits";

pub struct Core {
    store: Store,
    vault: OtpVault,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MessagesPage {
    pub messages: Vec<ParsedSms>,
    pub next: Option<MessageCursor>,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub ingest: IngestReport,
    pub archived_threads: usize,
    pub blocked_senders: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub platform: &'static str,
}

pub fn platform_capabilities() -> Capabilities {
    Capabilities {
        platform: if cfg!(target_os = "android") {
            "android"
        } else {
            "linux"
        },
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

impl Core {
    pub fn new(store: Store) -> Self {
        Core {
            store,
            vault: OtpVault::new_random(),
        }
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
        Ok(MessagesPage { messages, next })
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

    pub fn weekly_scan(&self) -> CmdResult<WeeklyScanReport> {
        Ok(self.store.weekly_scan(now_ms())?)
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
        let items = self.store.otp_items(now_ms())?;
        Ok(items
            .into_iter()
            .map(|mut item| {
                let vault = self.vault.encrypt_otp(&item.otp.code);
                item.otp.code = "\u{2022}".repeat(item.otp.code.chars().count());
                OtpItemView { item, vault }
            })
            .collect())
    }

    pub fn vault_reveal(&self, payload: &EncryptedOtpPayload) -> CmdResult<String> {
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
