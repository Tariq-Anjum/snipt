//! Finance summary and budget stats over stored transactions, using the shared
//! sms-engine aggregation (identical rules to store.ts / budget.ts).

use sms_engine::{
    budget::{BudgetPlannerSummary, compute_budget_stats},
    inbox::{FinanceMessage, FinanceThread, compute_finance_summary_from},
};
use sms_types::{BudgetLimits, ExtractedTransaction, FinanceSummary, PaymentCycleConfig};

use crate::{Result, Store};

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

        let mut threads: Vec<FinanceThread<'_>> = Vec::new();
        for r in &rows {
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
        Ok(compute_finance_summary_from(
            &threads,
            cycle_config,
            now_ms,
            only_active_cycle,
            self.tz_offset_min,
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
}
