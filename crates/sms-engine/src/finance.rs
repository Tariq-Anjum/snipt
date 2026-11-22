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
    r"(?:avl\.?\s*bal|avail(?:able)?\s*bal(?:ance)?|bal(?:ance)?|avl\.?\s*lmt|avl\.?\s*limit|avail(?:able)?\s*limit)\s*(?:is|:)?\s*(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)",
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
    r"\b(?:pre-approved|eligible\s+for|get\s+loan|apply\s+now|win\s+upto|flat\s+[0-9]+%\s+off|offer\s+valid|power\s+cash|super\s*coins?|neu\s*coins?|reward\s+points|loyalty\s+points|bonus\s+cash)\b",
    i
);
js_regex!(
    credit_card_re,
    r"\b(?:credit\s*card|cc\s*(?:xx|ending|\d{4})|regalia|millennia|infinia|coral\s*card|amex|sbi\s*card|icici\s*bank\s*card|axis\s*bank\s*card|card\s*(?:xx+|\*+|ending)\s*[0-9]{3,4}|avl\.?\s*lmt|available\s*limit)\b",
    i
);
js_regex!(upi_sent_re, r"\bsent\s+(?:rs\.?|inr|₹)", i);
// Phone round 1 (mirrors finance.ts): non-transactions and balance amounts.
js_regex!(
    due_notice_re,
    r"\b(?:is\s+due|are\s+due|due\s+(?:on|by|date)|amount\s+due|min(?:imum)?\.?\s+(?:amt\.?|amount)\s+due|ignore\s+if\s+(?:already\s+)?paid|bill\s+(?:is\s+)?generated)\b",
    i
);
js_regex!(
    completed_re,
    r"\b(?:debited|credited|spent|withdrawn|deducted|charged)\b",
    i
);
js_regex!(
    future_debit_re,
    r"\bwill\s+be\s+(?:auto-?\s*)?(?:debited|deducted|charged)\b",
    i
);
js_regex!(
    biller_receipt_re,
    r"\b(?:payment\b[^\n]{0,100}?\b(?:has\s+been\s+|is\s+)?received|received\s+(?:your\s+)?payment)\b",
    i
);
js_regex!(credited_re, r"\bcredited\b", i);
// Own-account transfer (card bill from savings, "InfoBIL*INFT*"): not spending.
js_regex!(
    own_transfer_re,
    r"\b(?:INFT|self\s+transfer|own\s+account\s+transfer)\b",
    i
);
js_regex!(
    foreign_amount_re,
    r"\b(USD|EUR|GBP|AED|SGD)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)",
    i
);
js_regex!(
    upi_payee_re,
    r"\bUPI[-:/][0-9]{6,}[-/]([A-Za-z][A-Za-z0-9 &._-]{1,32}?)(?:\.(?:\s|$)|\s+to\s+dispute\b|$)",
    i
);
js_regex!(
    name_credited_re,
    r";\s*([A-Za-z][A-Za-z0-9 &._-]{1,40}?)\s+credited\b",
    i
);
js_regex!(phone_like_re, r"^\+?[0-9][0-9 -]{6,}$");
js_regex!(explicit_credit_card_re, r"\bcredit\s*card\b", i);
js_regex!(twelve_digits_re, r"^\d{12}$");

/// `Number.parseFloat(raw.replace(/,/g, ''))` + `Number.isFinite` for the
/// digit/comma/decimal strings captured by AMOUNT_RE / BALANCE_RE.
fn parse_inr(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| *c != ',').collect();
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

pub fn detect_payment_method(body: &str, ref_id: Option<&str>) -> PaymentMethod {
    // Credit card charged through UPI is a card spend (card billing cycle).
    if explicit_credit_card_re().is_match(body) {
        return PaymentMethod::CreditCard;
    }
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

/// First INR amount outside a balance / available-limit phrase, or an
/// earlier foreign-currency amount (`currency` = its code).
fn pick_amount(body: &str) -> Option<(f64, String)> {
    let spans: Vec<(usize, usize)> = balance_re()
        .find_iter(body)
        .map(|m| (m.start(), m.end()))
        .collect();
    let mut inr: Option<(f64, usize)> = None;
    for c in amount_re().captures_iter(body) {
        let i = c.get(0).map_or(0, |m| m.start());
        if spans.iter().any(|&(s, e)| i >= s && i < e) {
            continue;
        }
        if let Some(n) = parse_inr(&c[1]).filter(|n| *n > 0.0) {
            inr = Some((n, i));
            break;
        }
    }
    if let Some(f) = foreign_amount_re().captures(body) {
        let fi = f.get(0).map_or(0, |m| m.start());
        if inr.is_none_or(|(_, ii)| fi < ii)
            && let Some(n) = parse_inr(&f[2]).filter(|n| *n > 0.0)
        {
            return Some((n, f[1].to_uppercase()));
        }
    }
    inr.map(|(n, _)| (n, "INR".to_string()))
}

fn pick_merchant(body: &str) -> Option<String> {
    if let Some(c) = upi_payee_re()
        .captures(body)
        .or_else(|| name_credited_re().captures(body))
    {
        let p = js::trim(&c[1]).to_string();
        if js::utf16_len(&p) >= 2 {
            return Some(p);
        }
    }
    merchant_re()
        .captures_iter(body)
        .map(|c| js::trim(&c[1]).to_string())
        .find(|m| js::utf16_len(m) >= 2 && !phone_like_re().is_match(m))
}

pub fn extract_transaction(body: &str) -> Option<ExtractedTransaction> {
    if promo_blocker_re().is_match(body) {
        return None;
    }
    if future_debit_re().is_match(body) {
        return None;
    }
    if due_notice_re().is_match(body) && !completed_re().is_match(body) {
        return None;
    }
    if biller_receipt_re().is_match(body) && !credited_re().is_match(body) {
        return None;
    }
    if own_transfer_re().is_match(body) {
        return None;
    }
    let debit = debit_re().find(body);
    let credit = credit_re().find(body);
    if debit.is_none() && credit.is_none() {
        return None;
    }
    let (amount, currency) = pick_amount(body)?;

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
    let merchant_or_vpa = pick_merchant(body);

    let payment_method = detect_payment_method(body, reference_id.as_deref());

    Some(ExtractedTransaction {
        amount,
        currency,
        direction,
        payment_method,
        account_last4: account,
        merchant_or_vpa,
        reference_id,
        available_balance: balance,
        is_upi: payment_method == PaymentMethod::Upi,
    })
}
