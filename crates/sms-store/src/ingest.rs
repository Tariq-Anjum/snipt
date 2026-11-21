//! Ingest: classify (sms-engine) + insert + incremental thread recompute.

use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sms_engine::{Analysis, budget::categorize_merchant_expense};
use sms_types::{MessageProtocol, RawSms, js, time::is_valid_timestamp_ms};

use crate::{Result, Store, StoreError, threads::recompute_thread};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct IngestReport {
    pub inserted: usize,
    pub skipped_existing: usize,
    pub threads_touched: usize,
}

/// Input of store.ts `ingestIncomingSms`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IncomingSms {
    pub sender: String,
    #[serde(default)]
    pub contact_name: Option<String>,
    pub body: String,
    pub is_known_contact: bool,
    pub sim_slot: u8,
    #[serde(default)]
    pub protocol: Option<MessageProtocol>,
    #[serde(default)]
    pub timestamp_ms: Option<i64>,
}

pub(crate) fn enum_str<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => unreachable!("enum serialises to a string"),
    }
}

/// Non-body parts of the TS search haystack for one message.
pub(crate) fn search_extra(a: &Analysis) -> String {
    let txn = a.transaction.as_ref();
    [
        a.otp.as_ref().map_or("", |o| o.code.as_str()),
        txn.and_then(|t| t.merchant_or_vpa.as_deref()).unwrap_or(""),
        txn.map_or("", |t| t.payment_method.as_str()),
        txn.and_then(|t| t.reference_id.as_deref()).unwrap_or(""),
        a.reminder
            .as_ref()
            .and_then(|r| r.reference.as_deref())
            .unwrap_or(""),
    ]
    .join(" ")
}

fn opt_json<T: Serialize>(v: &Option<T>) -> Result<Option<String>> {
    Ok(match v {
        Some(x) => Some(serde_json::to_string(x)?),
        None => None,
    })
}

/// Insert one analysed message. Returns false when the id already exists.
pub(crate) fn insert_message(tx: &Transaction<'_>, sms: &RawSms, a: &Analysis) -> Result<bool> {
    if sms.sim_slot > 1 {
        return Err(StoreError::Invalid("simSlot must be 0 or 1"));
    }
    if !is_valid_timestamp_ms(sms.timestamp_ms) {
        return Err(StoreError::Invalid("timestampMs out of range"));
    }
    let exists: Option<i64> = tx
        .prepare_cached("SELECT 1 FROM messages WHERE id = ?1")?
        .query_row([sms.id], |r| r.get(0))
        .optional()?;
    if exists.is_some() {
        return Ok(false);
    }
    let seq: i64 = tx
        .prepare_cached("SELECT COALESCE(MAX(ins_seq), 0) + 1 FROM messages")?
        .query_row([], |r| r.get(0))?;
    let c = &a.classification;
    tx.prepare_cached(
        "INSERT INTO messages (id, ins_seq, thread_id, sender, sender_lc, contact_name, body, ts_ms,
            is_known_contact, is_outgoing, sim_slot, is_read, is_starred, protocol, rcs_status,
            rcs_card_json, category, confidence, reason, scores_json, sender_kind_json, otp_json,
            txn_json, reminder_json, search_extra)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
    )?
    .execute(params![
        sms.id,
        seq,
        sms.thread_id,
        sms.sender,
        sms.sender.to_lowercase(),
        sms.contact_name,
        sms.body,
        sms.timestamp_ms,
        sms.is_known_contact,
        sms.is_outgoing,
        sms.sim_slot,
        sms.is_read,
        sms.is_starred,
        sms.protocol.as_ref().map(enum_str),
        sms.rcs_status.as_ref().map(enum_str),
        opt_json(&sms.rcs_rich_card)?,
        c.category.as_str(),
        c.confidence,
        enum_str(&c.reason),
        serde_json::to_string(&c.scores)?,
        serde_json::to_string(&a.sender_kind)?,
        opt_json(&a.otp)?,
        opt_json(&a.transaction)?,
        opt_json(&a.reminder)?,
        search_extra(a),
    ])?;
    if let Some(t) = &a.transaction {
        tx.prepare_cached(
            "INSERT INTO transactions (msg_id, ts_ms, amount, direction, method, last4, merchant,
                ref_id, balance, budget_category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?
        .execute(params![
            sms.id,
            sms.timestamp_ms,
            t.amount,
            enum_str(&t.direction),
            t.payment_method.as_str(),
            t.account_last4,
            t.merchant_or_vpa,
            t.reference_id,
            t.available_balance,
            categorize_merchant_expense(t.merchant_or_vpa.as_deref(), &sms.body).as_str(),
        ])?;
    }
    Ok(true)
}

impl Store {
    /// Classify and insert a batch in one transaction; threads touched are
    /// recomputed once each. Existing ids are skipped (idempotent re-import).
    pub fn ingest_batch(&mut self, messages: &[RawSms]) -> Result<IngestReport> {
        let tx = crate::write_tx(&mut self.conn)?;
        let mut report = IngestReport::default();
        let mut touched = BTreeSet::new();
        for sms in messages {
            let a = self.engine.analyze(sms);
            if insert_message(&tx, sms, &a)? {
                report.inserted += 1;
                touched.insert(sms.thread_id);
            } else {
                report.skipped_existing += 1;
            }
        }
        for t in &touched {
            recompute_thread(&tx, *t)?;
        }
        report.threads_touched = touched.len();
        tx.commit()?;
        Ok(report)
    }

    /// Port of store.ts `ingestIncomingSms`: assigns id / thread id, un-archives
    /// the thread, classifies, stores. `now_ms` is used when no timestamp given.
    pub fn ingest_incoming(&mut self, input: &IncomingSms, now_ms: i64) -> Result<RawSms> {
        Ok(self.ingest_from_provider(input, None, now_ms)?.0)
    }

    /// [`Store::ingest_incoming`] for a message that also exists in the
    /// platform provider (Android `content://sms` `_id`). A provider id that is
    /// already stored is not ingested again: the stored message is returned
    /// with `false`, so receiver retries and re-syncs are idempotent.
    pub fn ingest_from_provider(
        &mut self,
        input: &IncomingSms,
        provider_id: Option<i64>,
        now_ms: i64,
    ) -> Result<(RawSms, bool)> {
        if input.sim_slot > 1 {
            return Err(StoreError::Invalid("simSlot must be 0 or 1"));
        }
        if !is_valid_timestamp_ms(input.timestamp_ms.unwrap_or(now_ms)) {
            return Err(StoreError::Invalid("timestampMs out of range"));
        }
        let tx = crate::write_tx(&mut self.conn)?;
        if let Some(pid) = provider_id {
            let sql = format!(
                "SELECT {} FROM messages WHERE provider_id = ?1",
                crate::query::RAW_COLS
            );
            let existing = tx
                .prepare_cached(&sql)?
                .query_row([pid], crate::query::raw_from_row)
                .optional()?;
            if let Some(raw) = existing {
                return Ok((raw, false));
            }
        }
        let normalized = js::trim(&input.sender).to_string();
        let existing: Option<(i64, Option<String>)> = tx
            .prepare_cached(
                "SELECT thread_id, contact_name FROM messages WHERE sender_lc = ?1
                 ORDER BY ins_seq LIMIT 1",
            )?
            .query_row([normalized.to_lowercase()], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let next_id: i64 = tx.query_row(
            "SELECT MAX(COALESCE(MAX(id), 1000), 1000) + 1 FROM messages",
            [],
            |r| r.get(0),
        )?;
        let thread_id = match &existing {
            Some((t, _)) => *t,
            None => tx.query_row(
                "SELECT MAX(COALESCE(MAX(thread_id), 100), 100) + 1 FROM messages",
                [],
                |r| r.get(0),
            )?,
        };
        let contact_name = input
            .contact_name
            .as_deref()
            .map(js::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| existing.and_then(|(_, c)| c));
        let sms = RawSms {
            id: next_id,
            thread_id,
            sender: normalized,
            contact_name,
            body: js::trim(&input.body).to_string(),
            timestamp_ms: input.timestamp_ms.unwrap_or(now_ms),
            is_known_contact: input.is_known_contact,
            is_outgoing: false,
            sim_slot: input.sim_slot,
            is_read: Some(false),
            is_starred: Some(false),
            protocol: Some(input.protocol.unwrap_or(MessageProtocol::Sms)),
            rcs_status: None,
            rcs_rich_card: None,
        };
        let a = self.engine.analyze(&sms);
        insert_message(&tx, &sms, &a)?;
        if let Some(pid) = provider_id {
            tx.execute(
                "UPDATE messages SET provider_id = ?2, provider_kind = 'sms' WHERE id = ?1",
                params![sms.id, pid],
            )?;
        }
        tx.execute(
            "DELETE FROM archived_threads WHERE thread_id = ?1",
            [thread_id],
        )?;
        recompute_thread(&tx, thread_id)?;
        tx.commit()?;
        Ok((sms, true))
    }
}
