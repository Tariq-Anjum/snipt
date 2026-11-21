//! Port of dlt.ts — TRAI DLT sender header parsing.

use crate::jsre::js_regex;
use sms_types::{DltSuffix, SenderKind, js};

js_regex!(
    dlt_with_suffix,
    r"^([A-Z]{2})[- ]?([A-Z0-9]{3,9})-([STPG])$",
    i
);
js_regex!(dlt_legacy, r"^([A-Z]{2})-([A-Z0-9]{3,9})$", i);
js_regex!(phone_re, r"^\+?[0-9]{10,15}$");
js_regex!(shortcode_re, r"^[0-9]{4,8}$");

pub fn parse_sender(sender: &str, is_known_contact: bool) -> SenderKind {
    if is_known_contact {
        return SenderKind::Contact;
    }
    let cleaned = js::trim(sender);

    if let Some(c) = dlt_with_suffix().captures(cleaned) {
        let suffix = match c[3].to_ascii_uppercase().as_str() {
            "S" => Some(DltSuffix::Service),
            "T" => Some(DltSuffix::Transactional),
            "P" => Some(DltSuffix::Promotional),
            "G" => Some(DltSuffix::Government),
            _ => None,
        };
        return SenderKind::DltHeader {
            prefix: c[1].to_ascii_uppercase(),
            entity: c[2].to_ascii_uppercase(),
            suffix,
        };
    }

    if let Some(c) = dlt_legacy().captures(cleaned) {
        return SenderKind::DltHeader {
            prefix: c[1].to_ascii_uppercase(),
            entity: c[2].to_ascii_uppercase(),
            suffix: None,
        };
    }

    let compact: String = cleaned
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();
    if phone_re().is_match(&compact) {
        return SenderKind::PhoneNumber;
    }
    if shortcode_re().is_match(&compact) {
        return SenderKind::Shortcode;
    }

    SenderKind::Alphanumeric {
        value: cleaned.to_string(),
    }
}

pub fn format_sender_kind_label(kind: &SenderKind) -> String {
    match kind {
        SenderKind::Contact => "Saved Contact".into(),
        SenderKind::PhoneNumber => "Phone Number".into(),
        SenderKind::Shortcode => "Shortcode".into(),
        SenderKind::DltHeader { entity, suffix, .. } => match suffix {
            Some(s) => format!("DLT {s:?} ({entity})"),
            None => format!("DLT Entity ({entity})"),
        },
        SenderKind::Alphanumeric { value } => format!("Alphanumeric ({value})"),
    }
}
