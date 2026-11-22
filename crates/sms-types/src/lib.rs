//! Shared serde types for mySMS.
//!
//! JSON shape is identical to `src/engine/types.ts` (camelCase keys, optional
//! fields omitted when absent, nullable fields serialised as `null`). Field
//! declaration order follows types.ts so that a canonical TS `JSON.stringify`
//! of a `RawSms` re-serialises byte-for-byte.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

pub mod js;
pub mod time;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Personal,
    Transactions,
    Promotions,
    Priority,
    Spam,
}

pub const CATEGORY_COUNT: usize = 5;

pub const CATEGORY_LIST: [Category; CATEGORY_COUNT] = [
    Category::Personal,
    Category::Transactions,
    Category::Promotions,
    Category::Priority,
    Category::Spam,
];

impl Category {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Personal => "Personal",
            Category::Transactions => "Transactions",
            Category::Promotions => "Promotions",
            Category::Priority => "Priority",
            Category::Spam => "Spam",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        CATEGORY_LIST.into_iter().find(|c| c.as_str() == s)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageProtocol {
    #[serde(rename = "SMS")]
    Sms,
    #[serde(rename = "RCS")]
    Rcs,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RcsDeliveryStatus {
    Sent,
    Delivered,
    Read,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RcsRichCard {
    pub title: String,
    pub subtitle: String,
    pub action_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_meta: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DltSuffix {
    Service,
    Transactional,
    Promotional,
    Government,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum SenderKind {
    Contact,
    PhoneNumber,
    Shortcode,
    DltHeader {
        prefix: String,
        entity: String,
        suffix: Option<DltSuffix>,
    },
    Alphanumeric {
        value: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawSms {
    pub id: i64,
    pub thread_id: i64,
    pub sender: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_name: Option<String>,
    pub body: String,
    pub timestamp_ms: i64,
    pub is_known_contact: bool,
    pub is_outgoing: bool,
    pub sim_slot: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_read: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_starred: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<MessageProtocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rcs_status: Option<RcsDeliveryStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rcs_rich_card: Option<RcsRichCard>,
}

impl RawSms {
    /// JS truthiness of `sms.isRead`.
    pub fn read(&self) -> bool {
        self.is_read.unwrap_or(false)
    }
    /// JS truthiness of `sms.isStarred`.
    pub fn starred(&self) -> bool {
        self.is_starred.unwrap_or(false)
    }
    /// JS truthiness of `sms.contactName` (empty string is falsy).
    pub fn contact_name_truthy(&self) -> Option<&str> {
        self.contact_name.as_deref().filter(|s| !s.is_empty())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedOtp {
    pub code: String,
    pub service_hint: Option<String>,
    pub ttl_seconds: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TxnDirection {
    Debit,
    Credit,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PaymentMethod {
    #[serde(rename = "UPI")]
    Upi,
    #[serde(rename = "Credit Card")]
    CreditCard,
    #[serde(rename = "Savings Account")]
    SavingsAccount,
}

pub const PAYMENT_METHODS: [PaymentMethod; 3] = [
    PaymentMethod::Upi,
    PaymentMethod::CreditCard,
    PaymentMethod::SavingsAccount,
];

impl PaymentMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            PaymentMethod::Upi => "UPI",
            PaymentMethod::CreditCard => "Credit Card",
            PaymentMethod::SavingsAccount => "Savings Account",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedTransaction {
    pub amount: f64,
    pub currency: String,
    pub direction: TxnDirection,
    pub payment_method: PaymentMethod,
    pub account_last4: Option<String>,
    pub merchant_or_vpa: Option<String>,
    pub reference_id: Option<String>,
    pub available_balance: Option<f64>,
    pub is_upi: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReminderKind {
    BillDue,
    TravelPnr,
    Delivery,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedReminder {
    pub kind: ReminderKind,
    pub reference: Option<String>,
    pub date_text: Option<String>,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationReason {
    ContactOrOutgoing,
    TxnExtractor,
    OtpOrReminder,
    DltPromoSuffix,
    MlLinear,
    ServiceRule,
    ThreadConsensus,
    EmptyThread,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: Category,
    pub confidence: f64,
    pub scores: [f64; CATEGORY_COUNT],
    pub reason: ClassificationReason,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedSms {
    pub sms: RawSms,
    pub sender_kind: SenderKind,
    pub classification: Classification,
    pub otp: Option<ExtractedOtp>,
    pub transaction: Option<ExtractedTransaction>,
    pub reminder: Option<ExtractedReminder>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub thread_id: i64,
    pub sender: String,
    pub display_name: String,
    pub sender_kind: SenderKind,
    pub messages: Vec<ParsedSms>,
    pub latest_message: ParsedSms,
    pub classification: Classification,
    pub unread_count: i64,
    pub has_starred: bool,
    pub is_archived: bool,
    pub is_blocked: bool,
    pub sim_slot: u8,
    pub is_rcs_capable: bool,
}

/// `Record<PaymentMethod, number>`: billing-cycle start day per method.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PaymentCycleConfig {
    #[serde(rename = "UPI")]
    pub upi: f64,
    #[serde(rename = "Credit Card")]
    pub credit_card: f64,
    #[serde(rename = "Savings Account")]
    pub savings_account: f64,
}

impl PaymentCycleConfig {
    pub fn get(&self, m: PaymentMethod) -> f64 {
        match m {
            PaymentMethod::Upi => self.upi,
            PaymentMethod::CreditCard => self.credit_card,
            PaymentMethod::SavingsAccount => self.savings_account,
        }
    }
}

impl Default for PaymentCycleConfig {
    /// `DEFAULT_CYCLE_CONFIG` from budget.ts.
    fn default() -> Self {
        PaymentCycleConfig {
            upi: 1.0,
            credit_card: 15.0,
            savings_account: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BudgetCategoryId {
    #[serde(rename = "Food & Dining")]
    FoodDining,
    #[serde(rename = "Transport & Travel")]
    TransportTravel,
    #[serde(rename = "Bills & Utilities")]
    BillsUtilities,
    #[serde(rename = "Shopping & Retail")]
    ShoppingRetail,
    #[serde(rename = "General & Other")]
    GeneralOther,
}

pub const BUDGET_CATEGORIES: [BudgetCategoryId; 5] = [
    BudgetCategoryId::FoodDining,
    BudgetCategoryId::TransportTravel,
    BudgetCategoryId::BillsUtilities,
    BudgetCategoryId::ShoppingRetail,
    BudgetCategoryId::GeneralOther,
];

impl BudgetCategoryId {
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetCategoryId::FoodDining => "Food & Dining",
            BudgetCategoryId::TransportTravel => "Transport & Travel",
            BudgetCategoryId::BillsUtilities => "Bills & Utilities",
            BudgetCategoryId::ShoppingRetail => "Shopping & Retail",
            BudgetCategoryId::GeneralOther => "General & Other",
        }
    }

    pub fn parse(s: &str) -> Option<BudgetCategoryId> {
        BUDGET_CATEGORIES.into_iter().find(|c| c.as_str() == s)
    }
}

/// `Record<BudgetCategoryId, number>`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BudgetLimits {
    #[serde(rename = "Food & Dining")]
    pub food_dining: f64,
    #[serde(rename = "Transport & Travel")]
    pub transport_travel: f64,
    #[serde(rename = "Bills & Utilities")]
    pub bills_utilities: f64,
    #[serde(rename = "Shopping & Retail")]
    pub shopping_retail: f64,
    #[serde(rename = "General & Other")]
    pub general_other: f64,
}

impl BudgetLimits {
    pub fn get(&self, c: BudgetCategoryId) -> f64 {
        match c {
            BudgetCategoryId::FoodDining => self.food_dining,
            BudgetCategoryId::TransportTravel => self.transport_travel,
            BudgetCategoryId::BillsUtilities => self.bills_utilities,
            BudgetCategoryId::ShoppingRetail => self.shopping_retail,
            BudgetCategoryId::GeneralOther => self.general_other,
        }
    }
}

impl Default for BudgetLimits {
    /// `DEFAULT_BUDGET_LIMITS` from budget.ts.
    fn default() -> Self {
        BudgetLimits {
            food_dining: 8000.0,
            transport_travel: 5000.0,
            bills_utilities: 6000.0,
            shopping_retail: 7000.0,
            general_other: 4000.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinanceTransactionItem {
    pub sms_id: i64,
    pub thread_id: i64,
    pub sender: String,
    pub timestamp_ms: i64,
    pub sim_slot: u8,
    pub body: String,
    pub txn: ExtractedTransaction,
    pub budget_category: BudgetCategoryId,
    pub in_active_cycle: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    pub account_last4: String,
    pub bank_label: String,
    pub payment_method: PaymentMethod,
    pub balance: f64,
    pub updated_at_ms: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopMerchant {
    pub merchant: String,
    pub total: f64,
    pub count: i64,
    pub payment_method: PaymentMethod,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSummary {
    pub total_debits: f64,
    pub total_credits: f64,
    pub net_cashflow: f64,
    pub upi_debit_total: f64,
    pub credit_card_debit_total: f64,
    pub savings_debit_total: f64,
    pub card_or_bank_debit_total: f64,
    pub transaction_count: i64,
    pub account_balances: Vec<AccountBalance>,
    pub top_merchants: Vec<TopMerchant>,
    pub transactions: Vec<FinanceTransactionItem>,
}
