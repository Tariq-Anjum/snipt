//! Port of budget.ts — billing-cycle windows (local time via explicit
//! `tz_offset_min`), merchant categorisation and budget planner stats.

use serde::{Deserialize, Serialize};
use sms_types::{
    BUDGET_CATEGORIES, BudgetCategoryId, BudgetLimits, FinanceTransactionItem, PaymentCycleConfig,
    PaymentMethod, TxnDirection,
};

use sms_types::time::DAY_MS;
/// `toLocaleDateString('en-IN', {month: 'short'})` (ICU: "Sept").
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec",
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CycleWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub label: String,
}

pub use sms_types::time::{civil_from_days, days_from_civil, local_date};

/// JS `new Date(y, month0, day, h, mi, s, ms).getTime()` with overflow
/// normalisation, for a fixed UTC offset.
#[allow(clippy::too_many_arguments)]
pub fn make_local(
    y: i64,
    month0: i64,
    day: i64,
    h: i64,
    mi: i64,
    s: i64,
    ms: i64,
    tz_offset_min: i32,
) -> i64 {
    let y = y + month0.div_euclid(12);
    let m = month0.rem_euclid(12) + 1;
    let days = days_from_civil(y, m, 1) + (day - 1);
    days * DAY_MS + h * 3_600_000 + mi * 60_000 + s * 1000 + ms - i64::from(tz_offset_min) * 60_000
}

fn fmt_day_month(ms: i64, tz: i32) -> String {
    let (_, m, d) = local_date(ms, tz);
    format!("{d} {}", MONTHS[(m - 1) as usize])
}

pub fn get_cycle_window(cycle_start_day: f64, now_ms: i64, tz_offset_min: i32) -> CycleWindow {
    let safe_day = cycle_start_day.floor().clamp(1.0, 28.0) as i64;
    let (year, month, current_day) = local_date(now_ms, tz_offset_min);
    let month0 = month - 1;
    let tz = tz_offset_min;
    let (start, end) = if current_day >= safe_day {
        let start = make_local(year, month0, safe_day, 0, 0, 0, 0, tz);
        let end = if safe_day == 1 {
            make_local(year, month0 + 1, 0, 23, 59, 59, 999, tz)
        } else {
            make_local(year, month0 + 1, safe_day - 1, 23, 59, 59, 999, tz)
        };
        (start, end)
    } else {
        (
            make_local(year, month0 - 1, safe_day, 0, 0, 0, 0, tz),
            make_local(year, month0, safe_day - 1, 23, 59, 59, 999, tz),
        )
    };
    CycleWindow {
        start_ms: start,
        end_ms: end,
        label: format!("{} – {}", fmt_day_month(start, tz), fmt_day_month(end, tz)),
    }
}

pub fn is_timestamp_in_method_cycle(
    timestamp_ms: i64,
    method: PaymentMethod,
    cycle_config: &PaymentCycleConfig,
    now_ms: i64,
    tz_offset_min: i32,
) -> bool {
    let w = get_cycle_window(cycle_config.get(method), now_ms, tz_offset_min);
    timestamp_ms >= w.start_ms && timestamp_ms <= w.end_ms
}

const CATEGORY_RULES: &[(BudgetCategoryId, &[&str])] = &[
    (
        BudgetCategoryId::FoodDining,
        &[
            "swiggy",
            "zomato",
            "zepto",
            "blinkit",
            "bigbasket",
            "blue tokai",
            "starbucks",
            "cafe",
            "coffee",
            "restaurant",
            "dominos",
            "bakery",
            "instamart",
            "eat",
            "food",
        ],
    ),
    (
        BudgetCategoryId::TransportTravel,
        &[
            "uber",
            "ola",
            "rapido",
            "irctc",
            "indigo",
            "air india",
            "vistara",
            "makemytrip",
            "metro",
            "fuel",
            "petrol",
            "hpcl",
            "bpcl",
            "fastag",
        ],
    ),
    (
        BudgetCategoryId::BillsUtilities,
        &[
            "act fibernet",
            "fibernet",
            "airtel",
            "jio",
            "vi ",
            "electricity",
            "bescom",
            "tata power",
            "broadband",
            "gas",
            "water",
            "recharge",
            "insurance",
            "lic",
            "rent",
        ],
    ),
    (
        BudgetCategoryId::ShoppingRetail,
        &[
            "amazon",
            "flipkart",
            "myntra",
            "ajio",
            "nykaa",
            "croma",
            "reliance",
            "decathlon",
            "ikea",
            "apple",
            "store",
            "mall",
        ],
    ),
];

pub fn categorize_merchant_expense(merchant_or_vpa: Option<&str>, body: &str) -> BudgetCategoryId {
    let text = format!("{} {body}", merchant_or_vpa.unwrap_or("")).to_lowercase();
    for (cat, kws) in CATEGORY_RULES {
        if kws.iter().any(|k| text.contains(k)) {
            return *cat;
        }
    }
    BudgetCategoryId::GeneralOther
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetStatus {
    #[serde(rename = "On track")]
    OnTrack,
    #[serde(rename = "Near limit")]
    NearLimit,
    #[serde(rename = "Over budget")]
    OverBudget,
}

/// `Record<PaymentMethod, number>`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct ByMethod {
    #[serde(rename = "UPI")]
    pub upi: f64,
    #[serde(rename = "Credit Card")]
    pub credit_card: f64,
    #[serde(rename = "Savings Account")]
    pub savings_account: f64,
}

impl ByMethod {
    fn add(&mut self, m: PaymentMethod, v: f64) {
        match m {
            PaymentMethod::Upi => self.upi += v,
            PaymentMethod::CreditCard => self.credit_card += v,
            PaymentMethod::SavingsAccount => self.savings_account += v,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetCategoryStat {
    pub category: BudgetCategoryId,
    pub limit: f64,
    pub spent: f64,
    pub remaining: f64,
    pub percent_used: f64,
    pub transaction_count: i64,
    pub status: BudgetStatus,
    pub by_method: ByMethod,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPlannerSummary {
    pub total_budget: f64,
    pub total_spent: f64,
    pub total_remaining: f64,
    pub overall_percent_used: f64,
    pub overall_status: BudgetStatus,
    pub categories: Vec<BudgetCategoryStat>,
}

pub fn compute_budget_stats(
    transactions: &[FinanceTransactionItem],
    limits: &BudgetLimits,
    only_active_cycle: bool,
) -> BudgetPlannerSummary {
    let mut spent = [0f64; 5];
    let mut count = [0i64; 5];
    let mut by_method = [ByMethod::default(); 5];
    for item in transactions {
        if item.txn.direction != TxnDirection::Debit {
            continue;
        }
        if only_active_cycle && !item.in_active_cycle {
            continue;
        }
        let i = item.budget_category as usize;
        spent[i] += item.txn.amount;
        count[i] += 1;
        by_method[i].add(item.txn.payment_method, item.txn.amount);
    }

    let (mut total_budget, mut total_spent) = (0f64, 0f64);
    let categories = BUDGET_CATEGORIES
        .iter()
        .map(|&category| {
            let i = category as usize;
            let limit = limits.get(category).max(0.0);
            let s = spent[i];
            let percent_used = if limit > 0.0 {
                (s / limit) * 100.0
            } else if s > 0.0 {
                100.0
            } else {
                0.0
            };
            let status = if s > limit {
                BudgetStatus::OverBudget
            } else if percent_used >= 80.0 {
                BudgetStatus::NearLimit
            } else {
                BudgetStatus::OnTrack
            };
            total_budget += limit;
            total_spent += s;
            BudgetCategoryStat {
                category,
                limit,
                spent: s,
                remaining: limit - s,
                percent_used,
                transaction_count: count[i],
                status,
                by_method: by_method[i],
            }
        })
        .collect();

    let overall_percent_used = if total_budget > 0.0 {
        (total_spent / total_budget) * 100.0
    } else {
        0.0
    };
    let overall_status = if total_spent > total_budget {
        BudgetStatus::OverBudget
    } else if overall_percent_used >= 80.0 {
        BudgetStatus::NearLimit
    } else {
        BudgetStatus::OnTrack
    };
    BudgetPlannerSummary {
        total_budget,
        total_spent,
        total_remaining: total_budget - total_spent,
        overall_percent_used,
        overall_status,
        categories,
    }
}
