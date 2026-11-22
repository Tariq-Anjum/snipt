//! One-time re-analysis of stored messages after an engine rule change
//! (migration step). Recomputes classification, transaction and reminder for
//! every message and rewrites only rows whose result changed; OTP data and
//! user state (read, starred, archived, blocked) are untouched.

use std::collections::BTreeSet;

use rusqlite::{Transaction, params};
use sms_engine::SmsEngine;
use sms_engine::budget::categorize_merchant_expense;
use sms_types::RawSms;

use crate::StoreError;
use crate::ingest::{enum_str, search_extra};
use crate::query::{RAW_COLS, raw_from_row};
use crate::threads::recompute_thread;

fn opt_json<T: serde::Serialize>(v: &Option<T>) -> Result<Option<String>, StoreError> {
    Ok(match v {
        Some(x) => Some(serde_json::to_string(x)?),
        None => None,
    })
}

/// Returns the number of messages whose analysis changed.
pub(crate) fn reanalyze_all(tx: &Transaction<'_>) -> Result<usize, StoreError> {
    let engine = SmsEngine::new();
    let rows: Vec<(RawSms, Option<String>, Option<String>, String)> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT {RAW_COLS}, txn_json, reminder_json, category FROM messages"
        ))?;
        stmt.query_map([], |r| {
            Ok((raw_from_row(r)?, r.get(14)?, r.get(15)?, r.get(16)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    let mut touched = BTreeSet::new();
    let mut changed = 0usize;
    for (sms, old_txn, old_rem, old_cat) in rows {
        let a = engine.analyze(&sms);
        let new_txn = opt_json(&a.transaction)?;
        let new_rem = opt_json(&a.reminder)?;
        let c = &a.classification;
        if new_txn == old_txn && new_rem == old_rem && c.category.as_str() == old_cat {
            continue;
        }
        tx.prepare_cached(
            "UPDATE messages SET category = ?2, confidence = ?3, reason = ?4, scores_json = ?5,
                txn_json = ?6, reminder_json = ?7, search_extra = ?8
             WHERE id = ?1",
        )?
        .execute(params![
            sms.id,
            c.category.as_str(),
            c.confidence,
            enum_str(&c.reason),
            serde_json::to_string(&c.scores)?,
            new_txn,
            new_rem,
            search_extra(&a),
        ])?;
        tx.prepare_cached("DELETE FROM transactions WHERE msg_id = ?1")?
            .execute([sms.id])?;
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
        touched.insert(sms.thread_id);
        changed += 1;
    }
    for t in &touched {
        recompute_thread(tx, *t)?;
    }
    Ok(changed)
}
