//! Finance summary and budget stats over stored transactions, using the shared
//! sms-engine aggregation (identical rules to store.ts / budget.ts).

use sms_engine::{
    budget::{BudgetPlannerSummary, compute_budget_stats, get_cycle_window},
    inbox::{FinanceMessage, FinanceThread, compute_finance_summary_since},
};
use sms_types::{BudgetLimits, ExtractedTransaction, FinanceSummary, PaymentCycleConfig};

use crate::{Result, Store};

/// Two alerts for the same money within this window can be one transaction.
const DUP_WINDOW_MS: i64 = 20 * 60 * 1000;

fn norm_body(b: &str) -> String {
    b.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Detail a bank alert carries; the richer alert of a duplicate pair is kept.
fn richness(t: &ExtractedTransaction) -> u8 {
    u8::from(t.account_last4.is_some())
        + u8::from(t.reference_id.is_some())
        + u8::from(t.available_balance.is_some())
        + u8::from(t.merchant_or_vpa.is_some())
}

fn differ(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

fn same_money(a: &Row, b: &Row) -> bool {
    a.txn.direction == b.txn.direction
        && a.txn.currency == b.txn.currency
        && (a.txn.amount - b.txn.amount).abs() < 0.005
        && (a.ts - b.ts).abs() <= DUP_WINDOW_MS
}

/// Duplicate alerts for one transaction: the same SMS delivered twice (or by
/// two sender headers of one bank), or a bank alert plus the merchant's or
/// card issuer's own message for the same payment. Same thread: duplicate
/// only when the body or the reference number is identical (two real
/// purchases of the same amount carry different references). Across
/// threads: duplicate unless both name different accounts or different
/// references. Returns the indices of the alerts to leave out.
fn duplicate_rows(rows: &[Row]) -> std::collections::HashSet<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| (rows[i].ts, rows[i].sms_id));
    let norms: Vec<String> = rows.iter().map(|r| norm_body(&r.body)).collect();
    let mut drop = std::collections::HashSet::new();
    for (k, &i) in order.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        for &j in order[k + 1..].iter() {
            if rows[j].ts - rows[i].ts > DUP_WINDOW_MS {
                break;
            }
            if drop.contains(&j) || !same_money(&rows[i], &rows[j]) {
                continue;
            }
            let (a, b) = (&rows[i].txn, &rows[j].txn);
            let same_body = norms[i] == norms[j];
            let same_ref =
                matches!((&a.reference_id, &b.reference_id), (Some(x), Some(y)) if x == y);
            let dup = if rows[i].thread_id == rows[j].thread_id {
                same_body || same_ref
            } else {
                same_body
                    || same_ref
                    || !(differ(&a.account_last4, &b.account_last4)
                        || differ(&a.reference_id, &b.reference_id))
            };
            if dup {
                // Keep the richer alert; on a tie keep the earlier one.
                if richness(b) > richness(a) {
                    drop.insert(i);
                    break;
                }
                drop.insert(j);
            }
        }
    }
    drop
}

struct Row {
    thread_id: i64,
    display_name: String,
    is_blocked: bool,
    sms_id: i64,
    ts: i64,
    sim_slot: u8,
    body: String,
    txn: ExtractedTransaction,
}

impl Store {
    pub fn finance_summary(
        &self,
        cycle_config: &PaymentCycleConfig,
        now_ms: i64,
        only_active_cycle: bool,
    ) -> Result<FinanceSummary> {
        self.finance_summary_since(cycle_config, now_ms, only_active_cycle, None)
    }

    /// Finance summary whose active cycles start no earlier than `since_ms`
    /// (the user's manual "start fresh" reset). Nothing is deleted.
    pub fn finance_summary_since(
        &self,
        cycle_config: &PaymentCycleConfig,
        now_ms: i64,
        only_active_cycle: bool,
        since_ms: Option<i64>,
    ) -> Result<FinanceSummary> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.thread_id, t.display_name, t.is_blocked, m.id, m.ts_ms, m.sim_slot, m.body, m.txn_json
             FROM transactions x
             JOIN messages m ON m.id = x.msg_id
             JOIN threads t ON t.thread_id = m.thread_id
             ORDER BY t.latest_ts DESC, t.thread_id DESC, m.ts_ms, m.ins_seq",
        )?;
        let rows: Vec<Row> = stmt
            .query_map([], |r| {
                let txn_json: String = r.get(7)?;
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    txn_json,
                ))
            })?
            .map(|row| {
                let (thread_id, display_name, is_blocked, sms_id, ts, sim_slot, body, txn_json): (
                    i64,
                    String,
                    bool,
                    i64,
                    i64,
                    u8,
                    String,
                    String,
                ) = row?;
                Ok(Row {
                    thread_id,
                    display_name,
                    is_blocked,
                    sms_id,
                    ts,
                    sim_slot,
                    body,
                    txn: serde_json::from_str(&txn_json)?,
                })
            })
            .collect::<Result<_>>()?;

        let dups = duplicate_rows(&rows);
        let mut threads: Vec<FinanceThread<'_>> = Vec::new();
        for (idx, r) in rows.iter().enumerate() {
            if dups.contains(&idx) {
                continue;
            }
            if threads.last().is_none_or(|t| t.thread_id != r.thread_id) {
                threads.push(FinanceThread {
                    thread_id: r.thread_id,
                    display_name: &r.display_name,
                    is_blocked: r.is_blocked,
                    messages: Vec::new(),
                });
            }
            threads
                .last_mut()
                .expect("pushed")
                .messages
                .push(FinanceMessage {
                    sms_id: r.sms_id,
                    timestamp_ms: r.ts,
                    sim_slot: r.sim_slot,
                    body: &r.body,
                    txn: &r.txn,
                });
        }
        Ok(compute_finance_summary_since(
            &threads,
            cycle_config,
            now_ms,
            only_active_cycle,
            self.tz_offset_min,
            since_ms,
        ))
    }

    pub fn budget_stats(
        &self,
        cycle_config: &PaymentCycleConfig,
        limits: &BudgetLimits,
        now_ms: i64,
        only_active_cycle: bool,
    ) -> Result<BudgetPlannerSummary> {
        let fin = self.finance_summary(cycle_config, now_ms, only_active_cycle)?;
        Ok(compute_budget_stats(
            &fin.transactions,
            limits,
            only_active_cycle,
        ))
    }

    /// Budget over one monthly budget period for every payment method: the
    /// month starting on `reset_day` (1..=28, local time), and no earlier
    /// than `since_ms` (manual reset). Per-method billing cycles still drive
    /// the finance totals; the budget follows the user's budget month.
    pub fn budget_stats_period(
        &self,
        cycle_config: &PaymentCycleConfig,
        limits: &BudgetLimits,
        now_ms: i64,
        only_active_cycle: bool,
        reset_day: u8,
        since_ms: Option<i64>,
    ) -> Result<BudgetPlannerSummary> {
        let mut fin = self.finance_summary_since(cycle_config, now_ms, false, None)?;
        let w = get_cycle_window(f64::from(reset_day), now_ms, self.tz_offset_min);
        for t in &mut fin.transactions {
            t.in_active_cycle = t.timestamp_ms >= w.start_ms
                && t.timestamp_ms <= w.end_ms
                && since_ms.is_none_or(|s| t.timestamp_ms >= s);
        }
        Ok(compute_budget_stats(
            &fin.transactions,
            limits,
            only_active_cycle,
        ))
    }
}
