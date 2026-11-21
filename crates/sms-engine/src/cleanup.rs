//! Port of cleanup.ts — weekly useless-SMS scan. Never flags starred, outgoing,
//! known-contact, transaction or active-reminder messages, even from blocked
//! senders or when classified as Spam.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sms_types::{Category, RawSms, ReminderKind, js};

use crate::engine::SmsEngine;

const DAY_MS: i64 = 86_400_000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CleanupBucketId {
    ExpiredOtps,
    OldPromotions,
    SpamPhishing,
    OldDeliveries,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSample {
    pub sms_id: i64,
    pub sender: String,
    pub body: String,
    pub timestamp_ms: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupBucket {
    pub id: CleanupBucketId,
    pub title: String,
    pub description: String,
    pub message_ids: Vec<i64>,
    pub estimated_bytes: i64,
    pub samples: Vec<CleanupSample>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyScanReport {
    pub scanned_at_ms: i64,
    pub total_scanned: i64,
    pub total_useless_count: i64,
    pub total_estimated_bytes: i64,
    pub buckets: Vec<CleanupBucket>,
}

fn bucket(id: CleanupBucketId, title: &str, description: &str) -> CleanupBucket {
    CleanupBucket {
        id,
        title: title.into(),
        description: description.into(),
        message_ids: Vec::new(),
        estimated_bytes: 0,
        samples: Vec::new(),
    }
}

fn add_to_bucket(b: &mut CleanupBucket, sms: &RawSms) {
    b.message_ids.push(sms.id);
    // ~240 B SQLite row + FTS5 overhead + UTF-16 body/sender (as in cleanup.ts)
    b.estimated_bytes +=
        240 + js::utf16_len(&sms.body) as i64 * 2 + js::utf16_len(&sms.sender) as i64 * 2;
    if b.samples.len() < 3 {
        b.samples.push(CleanupSample {
            sms_id: sms.id,
            sender: sms.sender.clone(),
            body: sms.body.clone(),
            timestamp_ms: sms.timestamp_ms,
        });
    }
}

pub fn run_weekly_useless_scan<'a, I>(
    messages: I,
    blocked_senders: &[String],
    now_ms: i64,
    engine: &SmsEngine,
) -> WeeklyScanReport
where
    I: IntoIterator<Item = &'a RawSms>,
{
    let blocked: HashSet<String> = blocked_senders.iter().map(|s| s.to_lowercase()).collect();
    let mut expired = bucket(
        CleanupBucketId::ExpiredOtps,
        "Expired OTPs",
        "One-time codes older than 24 hours with no transaction or reminder attached.",
    );
    let mut promos = bucket(
        CleanupBucketId::OldPromotions,
        "Old Promotions & Offers",
        "Promotional DLT (-P) ads, flash sales, and coupon alerts older than 7 days.",
    );
    let mut spam = bucket(
        CleanupBucketId::SpamPhishing,
        "Spam & Phishing Alerts",
        "Blocked senders, KYC scams, lottery bait, and suspicious loan links.",
    );
    let mut deliveries = bucket(
        CleanupBucketId::OldDeliveries,
        "Expired Delivery Alerts",
        "Past package out-for-delivery updates older than 7 days.",
    );

    let mut total = 0i64;
    for sms in messages {
        total += 1;
        if sms.starred() || sms.is_outgoing || sms.is_known_contact {
            continue;
        }
        let age_ms = now_ms - sms.timestamp_ms;
        let is_blocked = blocked.contains(&sms.sender.to_lowercase());
        let parsed = engine.analyze(sms);

        // Safety exclusions beat every bucket, including blocked senders and
        // Spam: transactions and active reminders (bill due / travel PNR at any
        // age, deliveries younger than 7 days) are never flagged.
        if parsed.transaction.is_some() {
            continue;
        }
        if let Some(r) = &parsed.reminder
            && (r.kind != ReminderKind::Delivery || age_ms < 7 * DAY_MS)
        {
            continue;
        }
        if is_blocked || parsed.classification.category == Category::Spam {
            add_to_bucket(&mut spam, sms);
            continue;
        }
        if let Some(otp) = &parsed.otp
            && parsed.reminder.is_none()
            && age_ms >= otp.ttl_seconds * 1000
        {
            add_to_bucket(&mut expired, sms);
            continue;
        }
        if parsed.reminder.as_ref().map(|r| r.kind) == Some(ReminderKind::Delivery)
            && age_ms >= 7 * DAY_MS
        {
            add_to_bucket(&mut deliveries, sms);
            continue;
        }
        if parsed.classification.category == Category::Promotions && age_ms >= 7 * DAY_MS {
            add_to_bucket(&mut promos, sms);
        }
    }

    let buckets = vec![expired, promos, spam, deliveries];
    WeeklyScanReport {
        scanned_at_ms: now_ms,
        total_scanned: total,
        total_useless_count: buckets.iter().map(|b| b.message_ids.len() as i64).sum(),
        total_estimated_bytes: buckets.iter().map(|b| b.estimated_bytes).sum(),
        buckets,
    }
}
