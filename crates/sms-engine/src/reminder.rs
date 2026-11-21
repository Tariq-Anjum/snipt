//! Port of reminder.ts — PNR / bill due / delivery reminders.

use crate::jsre::js_regex;
use sms_types::{ExtractedReminder, ReminderKind, js};

js_regex!(
    pnr_re,
    r"\bpnr\s*(?:no\.?|number|#|:|-)?\s*([A-Z0-9]{6,10})\b",
    i
);
js_regex!(
    due_date_re,
    r"\b(?:due\s*(?:date|by|on)|pay\s*(?:by|before)|last\s*date)\s*(?:is|:)?\s*([0-9]{1,2}[-/ ][A-Za-z0-9]{2,9}(?:[-/ ][0-9]{2,4})?)",
    i
);
js_regex!(
    delivery_re,
    r"\b(?:out\s+for\s+delivery|arriving\s+today|will\s+be\s+delivered\s+today)\b",
    i
);

pub fn extract_reminder(body: &str) -> Option<ExtractedReminder> {
    if let Some(c) = pnr_re().captures(body) {
        let pnr = c[1].to_ascii_uppercase();
        return Some(ExtractedReminder {
            kind: ReminderKind::TravelPnr,
            summary: format!("Travel Booking (PNR: {pnr})"),
            reference: Some(pnr),
            date_text: None,
        });
    }
    if let Some(c) = due_date_re().captures(body) {
        let date_text = js::trim(&c[1]).to_string();
        return Some(ExtractedReminder {
            kind: ReminderKind::BillDue,
            reference: None,
            summary: format!("Bill payment due on {date_text}"),
            date_text: Some(date_text),
        });
    }
    if delivery_re().is_match(body) {
        return Some(ExtractedReminder {
            kind: ReminderKind::Delivery,
            reference: None,
            date_text: Some("Today".into()),
            summary: "Package out for delivery today".into(),
        });
    }
    None
}
