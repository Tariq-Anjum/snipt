//! Port of finance.ts — Indian UPI / credit card / savings transaction extraction.

use crate::jsre::js_regex;
use sms_types::{ExtractedTransaction, PaymentMethod, TxnDirection, js};

js_regex!(
    amount_re,
    r"(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)",
    i
);
js_regex!(
    account_re,
    r"(?:a\/c|ac|acct|account|card)\s*(?:no\.?\s*)?(?:ending\s*(?:with\s*)?|xx+|x+|\*+)?\s*([0-9]{3,4})\b",
    i
);
js_regex!(
    balance_re,
    r"(?:avl\.?\s*bal|avail(?:able)?\s*bal(?:ance)?|bal(?:ance)?|avl\.?\s*lmt|avail(?:able)?\s*limit)\s*(?:is|:)?\s*(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)",
    i
);
js_regex!(
    ref_re,
    r"(?:upi\s*(?:ref(?:\s*no)?|id|txn)?|ref(?:\s*no)?|utr)\s*[:#-]?[\s]*([0-9]{10,16}|[a-zA-Z0-9._-]+@[a-zA-Z]{2,})",
    i
);
js_regex!(
    merchant_re,
    r"\b(?:at|to|from|towards|paid\s+to|trf\s+to|vpa)\s+([A-Za-z0-9][A-Za-z0-9 &._@-]{1,32}?)(?:\s+(?:on|via|ref|upi|avl|bal|using|for|at|if)\b|[.,;]|$)",
    i
);
js_regex!(
    debit_re,
    r"\b(?:debited|spent|paid|sent|withdrawn|deducted|purchase|dr|charged)\b",
    i
);
js_regex!(
    credit_re,
    r"\b(?:credited|received|deposited|refund(?:ed)?|cashback\s+credited|cr)\b",
    i
);
js_regex!(
    promo_blocker_re,
    r"\b(?:pre-approved|eligible\s+for|get\s+loan|apply\s+now|win\s+upto|flat\s+[0-9]+%\s+off|offer\s+valid)\b",
    i
);
js_regex!(
    credit_card_re,
    r"\b(?:credit\s*card|cc\s*(?:xx|ending|\d{4})|regalia|millennia|infinia|coral\s*card|amex|sbi\s*card|icici\s*bank\s*card|axis\s*bank\s*card|card\s*(?:xx+|\*+|ending)\s*[0-9]{3,4}|avl\.?\s*lmt|available\s*limit)\b",
    i
);
js_regex!(upi_sent_re, r"\bsent\s+(?:rs\.?|inr|₹)", i);
js_regex!(twelve_digits_re, r"^\d{12}$");

/// `Number.parseFloat(raw.replace(/,/g, ''))` + `Number.isFinite` for the
/// digit/comma/decimal strings captured by AMOUNT_RE / BALANCE_RE.
fn parse_inr(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| *c != ',').collect();
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

pub fn detect_payment_method(body: &str, ref_id: Option<&str>) -> PaymentMethod {
    let lower = body.to_lowercase();
    let is_upi = lower.contains("upi")
        || lower.contains("vpa")
        || lower.contains("gpay")
        || lower.contains("phonepe")
        || lower.contains("paytm")
        || body.contains('@')
        || (upi_sent_re().is_match(body) && ref_id.is_some_and(|r| twelve_digits_re().is_match(r)));
    if is_upi {
        return PaymentMethod::Upi;
    }
    if credit_card_re().is_match(body) {
        return PaymentMethod::CreditCard;
    }
    PaymentMethod::SavingsAccount
}

pub fn extract_transaction(body: &str) -> Option<ExtractedTransaction> {
    if promo_blocker_re().is_match(body) {
        return None;
    }
    let debit = debit_re().find(body);
    let credit = credit_re().find(body);
    if debit.is_none() && credit.is_none() {
        return None;
    }
    let amount_caps = amount_re().captures(body)?;
    let amount = parse_inr(&amount_caps[1])?;
    if amount <= 0.0 {
        return None;
    }

    let direction = match (debit, credit) {
        (Some(_), None) => TxnDirection::Debit,
        (None, Some(_)) => TxnDirection::Credit,
        (Some(d), Some(c)) => {
            if d.start() <= c.start() {
                TxnDirection::Debit
            } else {
                TxnDirection::Credit
            }
        }
        (None, None) => TxnDirection::Unknown,
    };

    let account = account_re().captures(body).map(|c| c[1].to_string());
    let balance = balance_re().captures(body).and_then(|c| parse_inr(&c[1]));
    let reference_id = ref_re().captures(body).map(|c| c[1].to_string());
    let merchant_or_vpa = merchant_re()
        .captures(body)
        .map(|c| js::trim(&c[1]).to_string())
        .filter(|m| js::utf16_len(m) >= 2);

    let payment_method = detect_payment_method(body, reference_id.as_deref());

    Some(ExtractedTransaction {
        amount,
        currency: "INR".into(),
        direction,
        payment_method,
        account_last4: account,
        merchant_or_vpa,
        reference_id,
        available_balance: balance,
        is_upi: payment_method == PaymentMethod::Upi,
    })
}
