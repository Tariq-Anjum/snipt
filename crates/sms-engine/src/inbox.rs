//! In-memory port of the remaining store.ts selectors (filterThreads,
//! computeFinanceSummary, collectOtpItems, collectReminders,
//! purgeExpiredOtpMessages). sms-store mirrors these rules in SQL and uses
//! them as its reference.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sms_types::{
    AccountBalance, Category, ExtractedOtp, ExtractedReminder, ExtractedTransaction,
    FinanceSummary, FinanceTransactionItem, PaymentCycleConfig, PaymentMethod, RawSms,
    ThreadSummary, TopMerchant, TxnDirection, js,
};

use crate::budget::{categorize_merchant_expense, is_timestamp_in_method_cycle};
use crate::engine::SmsEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxFilter {
    All,
    Category(Category),
    Rcs,
    Starred,
    Archived,
}

impl InboxFilter {
    pub fn parse(s: &str) -> Option<InboxFilter> {
        Some(match s {
            "All" => InboxFilter::All,
            "RCS" => InboxFilter::Rcs,
            "Starred" => InboxFilter::Starred,
            "Archived" => InboxFilter::Archived,
            other => InboxFilter::Category(Category::parse(other)?),
        })
    }
}

/// Search tokens exactly as store.ts: JS trim, lowercase, split on JS `\s+`.
pub fn search_tokens(query: &str) -> Vec<String> {
    js::trim(query)
        .to_lowercase()
        .split(js::is_js_whitespace)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Folder/category/SIM part of filterThreads (no search).
pub fn thread_passes_filter(
    is_archived: bool,
    has_starred: bool,
    is_rcs: bool,
    category: Category,
    sim_slot: u8,
    filter: InboxFilter,
    sim: Option<u8>,
) -> bool {
    let folder_ok = match filter {
        InboxFilter::Archived => is_archived,
        _ if is_archived => false,
        InboxFilter::Starred => has_starred,
        InboxFilter::Rcs => is_rcs,
        InboxFilter::All => category != Category::Spam,
        InboxFilter::Category(c) => category == c,
    };
    folder_ok && sim.is_none_or(|s| s == sim_slot)
}

pub fn filter_threads<'a>(
    threads: &'a [ThreadSummary],
    filter: InboxFilter,
    sim: Option<u8>,
    query: &str,
) -> Vec<&'a ThreadSummary> {
    let tokens = search_tokens(query);
    threads
        .iter()
        .filter(|t| {
            if !thread_passes_filter(
                t.is_archived,
                t.has_starred,
                t.is_rcs_capable,
                t.classification.category,
                t.sim_slot,
                filter,
                sim,
            ) {
                return false;
            }
            if tokens.is_empty() {
                return true;
            }
            let recent = if t.messages.len() > 60 {
                &t.messages[t.messages.len() - 60..]
            } else {
                &t.messages[..]
            };
            let mut parts: Vec<String> = vec![
                t.display_name.clone(),
                t.sender.clone(),
                if t.is_rcs_capable { "rcs" } else { "sms" }.to_string(),
            ];
            for m in recent {
                let txn = m.transaction.as_ref();
                parts.push(
                    [
                        m.sms.body.as_str(),
                        m.otp.as_ref().map_or("", |o| o.code.as_str()),
                        txn.and_then(|x| x.merchant_or_vpa.as_deref()).unwrap_or(""),
                        txn.map_or("", |x| x.payment_method.as_str()),
                        txn.and_then(|x| x.reference_id.as_deref()).unwrap_or(""),
                        m.reminder
                            .as_ref()
                            .and_then(|r| r.reference.as_deref())
                            .unwrap_or(""),
                    ]
                    .join(" "),
                );
            }
            let haystack = parts.join(" ").to_lowercase();
            tokens.iter().all(|tok| haystack.contains(tok.as_str()))
        })
        .collect()
}

/// One message carrying a transaction, as consumed by [`compute_finance_summary_from`].
#[derive(Clone, Debug)]
pub struct FinanceMessage<'a> {
    pub sms_id: i64,
    pub timestamp_ms: i64,
    pub sim_slot: u8,
    pub body: &'a str,
    pub txn: &'a ExtractedTransaction,
}

/// Thread view for finance aggregation (threads in list order, messages oldest first).
#[derive(Clone, Debug)]
pub struct FinanceThread<'a> {
    pub thread_id: i64,
    pub display_name: &'a str,
    pub is_blocked: bool,
    pub messages: Vec<FinanceMessage<'a>>,
}

pub fn compute_finance_summary(
    threads: &[ThreadSummary],
    cycle_config: &PaymentCycleConfig,
    now_ms: i64,
    only_active_cycle: bool,
    tz_offset_min: i32,
) -> FinanceSummary {
    let inputs: Vec<FinanceThread<'_>> = threads
        .iter()
        .map(|t| FinanceThread {
            thread_id: t.thread_id,
            display_name: &t.display_name,
            is_blocked: t.is_blocked,
            messages: t
                .messages
                .iter()
                .filter_map(|m| {
                    m.transaction.as_ref().map(|txn| FinanceMessage {
                        sms_id: m.sms.id,
                        timestamp_ms: m.sms.timestamp_ms,
                        sim_slot: m.sms.sim_slot,
                        body: &m.sms.body,
                        txn,
                    })
                })
                .collect(),
        })
        .collect();
    compute_finance_summary_from(
        &inputs,
        cycle_config,
        now_ms,
        only_active_cycle,
        tz_offset_min,
    )
}

/// Port of store.ts `computeFinanceSummary` over an abstract thread view
/// (shared by the in-memory path and sms-store).
pub fn compute_finance_summary_from(
    threads: &[FinanceThread<'_>],
    cycle_config: &PaymentCycleConfig,
    now_ms: i64,
    only_active_cycle: bool,
    tz_offset_min: i32,
) -> FinanceSummary {
    let mut transactions: Vec<FinanceTransactionItem> = Vec::new();
    let mut bal_order: Vec<String> = Vec::new();
    let mut bal: HashMap<String, AccountBalance> = HashMap::new();
    let mut merch_order: Vec<String> = Vec::new();
    let mut merch: HashMap<String, (f64, i64, PaymentMethod)> = HashMap::new();
    let (mut total_debits, mut total_credits) = (0f64, 0f64);
    let (mut upi, mut cc, mut sav) = (0f64, 0f64, 0f64);

    for thread in threads {
        if thread.is_blocked {
            continue;
        }
        for msg in &thread.messages {
            let txn = msg.txn;
            let in_active = is_timestamp_in_method_cycle(
                msg.timestamp_ms,
                txn.payment_method,
                cycle_config,
                now_ms,
                tz_offset_min,
            );
            let budget_category =
                categorize_merchant_expense(txn.merchant_or_vpa.as_deref(), msg.body);
            transactions.push(FinanceTransactionItem {
                sms_id: msg.sms_id,
                thread_id: thread.thread_id,
                sender: thread.display_name.to_string(),
                timestamp_ms: msg.timestamp_ms,
                sim_slot: msg.sim_slot,
                body: msg.body.to_string(),
                txn: txn.clone(),
                budget_category,
                in_active_cycle: in_active,
            });

            if !only_active_cycle || in_active {
                match txn.direction {
                    TxnDirection::Debit => {
                        total_debits += txn.amount;
                        match txn.payment_method {
                            PaymentMethod::Upi => upi += txn.amount,
                            PaymentMethod::CreditCard => cc += txn.amount,
                            PaymentMethod::SavingsAccount => sav += txn.amount,
                        }
                        let key = txn
                            .merchant_or_vpa
                            .clone()
                            .filter(|m| !m.is_empty())
                            .unwrap_or_else(|| thread.display_name.to_string());
                        let e = merch.entry(key.clone()).or_insert_with(|| {
                            merch_order.push(key);
                            (0.0, 0, txn.payment_method)
                        });
                        e.0 += txn.amount;
                        e.1 += 1;
                        e.2 = txn.payment_method;
                    }
                    TxnDirection::Credit => total_credits += txn.amount,
                    TxnDirection::Unknown => {}
                }
            }

            if let Some(balance) = txn.available_balance {
                let prefix = if txn.payment_method == PaymentMethod::CreditCard {
                    "CC"
                } else {
                    "AC"
                };
                let key = format!(
                    "{prefix}-{}",
                    txn.account_last4.as_deref().unwrap_or(thread.display_name)
                );
                let replace = bal
                    .get(&key)
                    .is_none_or(|e| msg.timestamp_ms > e.updated_at_ms);
                if replace {
                    if !bal.contains_key(&key) {
                        bal_order.push(key.clone());
                    }
                    bal.insert(
                        key,
                        AccountBalance {
                            account_last4: txn
                                .account_last4
                                .clone()
                                .unwrap_or_else(|| "Main".into()),
                            bank_label: thread.display_name.to_string(),
                            payment_method: txn.payment_method,
                            balance,
                            updated_at_ms: msg.timestamp_ms,
                        },
                    );
                }
            }
        }
    }

    transactions.sort_by_key(|t| std::cmp::Reverse(t.timestamp_ms));
    let transaction_count = if only_active_cycle {
        transactions.iter().filter(|t| t.in_active_cycle).count()
    } else {
        transactions.len()
    } as i64;

    let mut top_merchants: Vec<TopMerchant> = merch_order
        .into_iter()
        .map(|k| {
            let (total, count, pm) = merch[&k];
            TopMerchant {
                merchant: k,
                total,
                count,
                payment_method: pm,
            }
        })
        .collect();
    top_merchants.sort_by(|a, b| b.total.total_cmp(&a.total));
    top_merchants.truncate(6);

    let mut account_balances: Vec<AccountBalance> = bal_order
        .into_iter()
        .map(|k| bal.remove(&k).expect("balance key"))
        .collect();
    account_balances.sort_by(|a, b| b.balance.total_cmp(&a.balance));

    FinanceSummary {
        total_debits,
        total_credits,
        net_cashflow: total_credits - total_debits,
        upi_debit_total: upi,
        credit_card_debit_total: cc,
        savings_debit_total: sav,
        card_or_bank_debit_total: cc + sav,
        transaction_count,
        account_balances,
        top_merchants,
        transactions,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveOtpItem {
    pub sms_id: i64,
    pub thread_id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp_ms: i64,
    pub sim_slot: u8,
    pub otp: ExtractedOtp,
    pub expires_at_ms: i64,
    pub is_expired: bool,
}

pub fn collect_otp_items(threads: &[ThreadSummary], now_ms: i64) -> Vec<ActiveOtpItem> {
    let mut items: Vec<ActiveOtpItem> = threads
        .iter()
        .filter(|t| !t.is_blocked && !t.is_archived)
        .flat_map(|t| {
            t.messages.iter().filter_map(move |m| {
                let otp = m.otp.as_ref()?;
                let expires_at_ms = sms_types::time::expiry_ms(m.sms.timestamp_ms, otp.ttl_seconds);
                Some(ActiveOtpItem {
                    sms_id: m.sms.id,
                    thread_id: t.thread_id,
                    sender: t.display_name.clone(),
                    body: m.sms.body.clone(),
                    timestamp_ms: m.sms.timestamp_ms,
                    sim_slot: m.sms.sim_slot,
                    otp: otp.clone(),
                    expires_at_ms,
                    is_expired: now_ms >= expires_at_ms,
                })
            })
        })
        .collect();
    items.sort_by_key(|o| std::cmp::Reverse(o.timestamp_ms));
    items
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReminderItem {
    pub sms_id: i64,
    pub thread_id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp_ms: i64,
    pub reminder: ExtractedReminder,
    pub is_done: bool,
}

pub fn collect_reminders(threads: &[ThreadSummary], completed_ids: &[i64]) -> Vec<ReminderItem> {
    let done: HashSet<i64> = completed_ids.iter().copied().collect();
    let mut items: Vec<ReminderItem> = threads
        .iter()
        .filter(|t| !t.is_blocked)
        .flat_map(|t| {
            let done = &done;
            t.messages.iter().filter_map(move |m| {
                let r = m.reminder.as_ref()?;
                Some(ReminderItem {
                    sms_id: m.sms.id,
                    thread_id: t.thread_id,
                    sender: t.display_name.clone(),
                    body: m.sms.body.clone(),
                    timestamp_ms: m.sms.timestamp_ms,
                    reminder: r.clone(),
                    is_done: done.contains(&m.sms.id),
                })
            })
        })
        .collect();
    items.sort_by(|a, b| {
        a.is_done
            .cmp(&b.is_done)
            .then(b.timestamp_ms.cmp(&a.timestamp_ms))
    });
    items
}

/// True when store.ts `purgeExpiredOtpMessages` would drop this message.
pub fn is_purgeable_expired_otp(raw: &RawSms, engine: &SmsEngine, now_ms: i64) -> bool {
    let p = engine.analyze(raw);
    match &p.otp {
        Some(otp) if p.transaction.is_none() && p.reminder.is_none() && !raw.starred() => {
            now_ms >= sms_types::time::expiry_ms(raw.timestamp_ms, otp.ttl_seconds)
        }
        _ => false,
    }
}

/// Returns (kept messages, purged count).
pub fn purge_expired_otp_messages(
    messages: &[RawSms],
    now_ms: i64,
    engine: &SmsEngine,
) -> (Vec<RawSms>, usize) {
    let kept: Vec<RawSms> = messages
        .iter()
        .filter(|m| !is_purgeable_expired_otp(m, engine, now_ms))
        .cloned()
        .collect();
    let purged = messages.len() - kept.len();
    (kept, purged)
}
