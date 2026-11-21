//! Port of otp.ts — OTP extraction with currency/card false-positive guard.

use crate::jsre::js_regex;
use sms_types::{ExtractedOtp, SenderKind};

js_regex!(
    otp_after_kw,
    r"\b(?:otp|one[- ]?time[- ]?(?:password|passcode|pin|code)|verification[- ]?code|security[- ]?code|auth(?:entication)?[- ]?code|login[- ]?code|passcode|mpin)\b[^0-9]{0,24}\b([0-9]{4,8})\b",
    i
);
js_regex!(
    otp_before_kw,
    r"\b([0-9]{4,8})\b\s+(?:is|as)\s+(?:your|the)\s+(?:[a-z0-9]+\s+){0,4}(?:otp|one[- ]?time|verification|security|login|secret|passcode|code)\b",
    i
);
js_regex!(otp_prefixed, r"\bG-([0-9]{6})\b", i);
js_regex!(
    false_positive_prefix,
    r"(?:rs\.?|inr|₹|a\/c|acct|xx+|ending)\s*$",
    i
);

/// `body.slice(max(0, idx - 12), idx)` where `idx` and `12` are UTF-16 units.
/// A surrogate pair straddling the window start is dropped; JS would keep a
/// lone low surrogate there, which can never take part in the `\s*$`-anchored
/// prefix test, so the outcome is identical.
fn utf16_prefix(body: &str, end: usize) -> &str {
    let mut units = 0;
    let mut start = end;
    for (i, ch) in body[..end].char_indices().rev() {
        units += ch.len_utf16();
        if units > 12 {
            break;
        }
        start = i;
    }
    &body[start..end]
}

pub fn extract_otp(body: &str, sender_kind: &SenderKind) -> Option<ExtractedOtp> {
    let regexes = [otp_prefixed(), otp_before_kw(), otp_after_kw()];
    let mut lower: Option<String> = None;

    for re in regexes {
        let Some(caps) = re.captures(body) else {
            continue;
        };
        let Some(code_m) = caps.get(1).filter(|m| !m.as_str().is_empty()) else {
            continue;
        };
        let code = code_m.as_str();
        let whole = caps.get(0).expect("group 0");
        let match_index = whole.start() + whole.as_str().find(code).unwrap_or(0);
        let prefix = utf16_prefix(body, match_index);

        if false_positive_prefix().is_match(prefix) {
            continue;
        }

        if code.len() == 4 {
            let num: i64 = code.parse().unwrap_or(0);
            let lower = lower.get_or_insert_with(|| body.to_lowercase());
            if (2020..=2035).contains(&num) && !lower.contains("otp") {
                continue;
            }
        }

        let service_hint = match sender_kind {
            SenderKind::DltHeader { entity, .. } => Some(entity.clone()),
            SenderKind::Alphanumeric { value } => Some(value.clone()),
            _ => None,
        };

        return Some(ExtractedOtp {
            code: code.to_string(),
            service_hint,
            ttl_seconds: 86_400,
        });
    }
    None
}
