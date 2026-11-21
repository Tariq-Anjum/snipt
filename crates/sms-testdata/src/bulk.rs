//! Port of indexedDb.ts `generateBulkHistoryMessages`.

use sms_types::{MessageProtocol, RawSms};

const DAY_MS: i64 = 86_400_000;

struct Template {
    thread_id: i64,
    sender: &'static str,
    contact_name: Option<&'static str>,
    is_known_contact: bool,
    sim_slot: u8,
    protocol: MessageProtocol,
    make_body: fn(i64) -> String,
}

const TEMPLATES: [Template; 7] = [
    Template {
        thread_id: 201,
        sender: "JM-MYNTRA-P",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 1,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "Flat {}% OFF flash sale #{i}! Use code SAVE{} on styles. Shop https://myntra.com/deal",
                30 + i % 40,
                10 + i % 80
            )
        },
    },
    Template {
        thread_id: 202,
        sender: "AD-SWIGGY-P",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 0,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "Weekend cravings? Flat 50% OFF upto Rs.120 on order #{i}. Offer valid tonight!"
            )
        },
    },
    Template {
        thread_id: 203,
        sender: "VM-OTPBOT-T",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 0,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "{} is your OTP for secure login verification. Valid for 5 mins. Do not share.",
                100_000 + (i * 7919) % 899_999
            )
        },
    },
    Template {
        thread_id: 204,
        sender: "VM-BLUEDR-S",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 1,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "Shipment AWB-{} is out for delivery today by 6 PM.",
                400_000 + i
            )
        },
    },
    Template {
        thread_id: 205,
        sender: "+918800291844",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 1,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "URGENT KYC #{i}: Your account is suspended! Claim lottery winner prize at http://bit.ly/verify-{i}"
            )
        },
    },
    Template {
        thread_id: 104,
        sender: "+919845012345",
        contact_name: Some("Aarav Mehta"),
        is_known_contact: true,
        sim_slot: 0,
        protocol: MessageProtocol::Rcs,
        make_body: |i| format!("Archived sprint check-in note #{i} from earlier this quarter."),
    },
    Template {
        thread_id: 101,
        sender: "AX-HDFCBK-S",
        contact_name: None,
        is_known_contact: false,
        sim_slot: 0,
        protocol: MessageProtocol::Sms,
        make_body: |i| {
            format!(
                "Sent Rs.{}.00 From HDFC Bank A/C *4821 To SWIGGY INSTAMART On 04/09/26 Ref {}. Avl Bal Rs.45,100.00",
                120 + i % 480,
                631_000_000_000_i64 + i
            )
        },
    },
];

/// Returns `existing` followed by generated messages up to `target_total`.
pub fn generate_bulk_history_messages(
    target_total: usize,
    existing: Vec<RawSms>,
    now_ms: i64,
) -> Vec<RawSms> {
    let needed = target_total.saturating_sub(existing.len());
    if needed == 0 {
        return existing;
    }
    let first_id = existing.iter().map(|s| s.id).fold(10_000, i64::max) + 1;
    let mut out = existing;
    out.reserve(needed);
    for i in 0..needed as i64 {
        let tpl = &TEMPLATES[(i % TEMPLATES.len() as i64) as usize];
        let age_days = 10 + i % 170;
        out.push(RawSms {
            id: first_id + i,
            thread_id: tpl.thread_id,
            sender: tpl.sender.to_string(),
            contact_name: tpl.contact_name.map(str::to_string),
            body: (tpl.make_body)(i),
            timestamp_ms: now_ms - age_days * DAY_MS - (i % 3600) * 1000,
            is_known_contact: tpl.is_known_contact,
            is_outgoing: false,
            sim_slot: tpl.sim_slot,
            is_read: Some(true),
            is_starred: Some(false),
            protocol: Some(tpl.protocol),
            rcs_status: None,
            rcs_rich_card: None,
        });
    }
    out
}
