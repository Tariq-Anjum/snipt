//! Port of thread.ts (`classifyThread`) and store.ts `buildThreads`.

use std::collections::{HashMap, HashSet};

use sms_types::{
    CATEGORY_COUNT, CATEGORY_LIST, Category, Classification, ClassificationReason, ParsedSms,
    RawSms, SenderKind, ThreadSummary,
};

use crate::engine::SmsEngine;

/// Exponentially recency-weighted consensus over the last 12 messages.
pub fn classify_thread(messages: &[ParsedSms]) -> Classification {
    let classes: Vec<&Classification> = messages.iter().map(|m| &m.classification).collect();
    classify_thread_classes(&classes)
}

/// Same as [`classify_thread`] over bare classifications (oldest first).
pub fn classify_thread_classes(messages: &[&Classification]) -> Classification {
    if messages.is_empty() {
        return Classification {
            category: Category::Personal,
            confidence: 1.0,
            scores: [1.0, 0.0, 0.0, 0.0, 0.0],
            reason: ClassificationReason::EmptyThread,
        };
    }
    let slice = if messages.len() > 12 {
        &messages[messages.len() - 12..]
    } else {
        messages
    };
    let mut acc = [0f64; CATEGORY_COUNT];
    let mut weight_sum = 0f64;
    let n = slice.len();
    for (i, cls) in slice.iter().enumerate() {
        let recency = 0.85f64.powf((n - 1 - i) as f64);
        let w = recency * (0.5 + cls.confidence);
        for (c, a) in acc.iter_mut().enumerate() {
            *a += cls.scores[c] * w;
        }
        weight_sum += w;
    }
    let denom = weight_sum.max(1e-9);
    for a in &mut acc {
        *a /= denom;
    }
    let (best_idx, best_score) = argmax(&acc);
    Classification {
        category: CATEGORY_LIST[best_idx],
        confidence: best_score,
        scores: acc,
        reason: ClassificationReason::ThreadConsensus,
    }
}

/// First index of the strict maximum (JS `>` scan).
pub fn argmax(scores: &[f64; CATEGORY_COUNT]) -> (usize, f64) {
    let mut best_idx = 0;
    let mut best = scores[0];
    for (i, s) in scores.iter().enumerate().skip(1) {
        if *s > best {
            best = *s;
            best_idx = i;
        }
    }
    (best_idx, best)
}

/// Inputs of store.ts `StoreState` that affect thread summaries.
#[derive(Clone, Debug, Default)]
pub struct ThreadState<'a> {
    pub archived_thread_ids: &'a [i64],
    pub blocked_senders: &'a [String],
}

/// Display name rule from store.ts `buildThreads`.
pub fn display_name(contact_name: Option<&str>, latest_kind: &SenderKind, sender: &str) -> String {
    match (contact_name, latest_kind) {
        (Some(n), _) => n.to_string(),
        (None, SenderKind::DltHeader { entity, .. }) => entity.clone(),
        (None, _) => sender.to_string(),
    }
}

/// Port of store.ts `buildThreads` (in-memory; sms-store keeps the same rules in SQL).
pub fn build_threads(
    messages: &[RawSms],
    state: &ThreadState<'_>,
    engine: &SmsEngine,
) -> Vec<ThreadSummary> {
    let archived: HashSet<i64> = state.archived_thread_ids.iter().copied().collect();
    let blocked: HashSet<String> = state
        .blocked_senders
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut order: Vec<(i64, Vec<ParsedSms>)> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    for raw in messages {
        let parsed = engine.analyze_message(raw);
        match index.get(&raw.thread_id) {
            Some(&i) => order[i].1.push(parsed),
            None => {
                index.insert(raw.thread_id, order.len());
                order.push((raw.thread_id, vec![parsed]));
            }
        }
    }

    let mut summaries: Vec<ThreadSummary> = Vec::with_capacity(order.len());
    for (thread_id, mut msgs) in order {
        msgs.sort_by_key(|m| m.sms.timestamp_ms);
        let latest = msgs.last().expect("non-empty thread").clone();
        let sender = latest.sms.sender.clone();
        let is_blocked = blocked.contains(&sender.to_lowercase());
        let mut classification = classify_thread(&msgs);
        if is_blocked {
            classification.category = Category::Spam;
            classification.confidence = 1.0;
        }
        let contact = msgs.iter().find_map(|m| m.sms.contact_name_truthy());
        let display = display_name(contact, &latest.sender_kind, &sender);
        let unread_count = msgs
            .iter()
            .filter(|m| !m.sms.is_outgoing && !m.sms.read())
            .count() as i64;
        let has_starred = msgs.iter().any(|m| m.sms.starred());
        let is_rcs_capable = msgs
            .iter()
            .any(|m| m.sms.protocol == Some(sms_types::MessageProtocol::Rcs));
        summaries.push(ThreadSummary {
            thread_id,
            sender,
            display_name: display,
            sender_kind: latest.sender_kind.clone(),
            sim_slot: latest.sms.sim_slot,
            latest_message: latest,
            messages: msgs,
            classification,
            unread_count,
            has_starred,
            is_archived: archived.contains(&thread_id),
            is_blocked,
            is_rcs_capable,
        });
    }
    summaries.sort_by(|a, b| {
        b.latest_message
            .sms
            .timestamp_ms
            .cmp(&a.latest_message.sms.timestamp_ms)
    });
    summaries
}
