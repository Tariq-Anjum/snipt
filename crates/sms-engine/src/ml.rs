//! Port of ml.ts — hashed linear classifier with heuristic bootstrap weights.
//!
//! Parity details:
//! * `fnv1a32` hashes `charCodeAt(i) & 0xff`: the LOW BYTE OF EACH UTF-16 CODE
//!   UNIT, not UTF-8 bytes.
//! * Weights live in a `Float32Array` in TS, so they are stored as `f32` here and
//!   widened to `f64` for every multiply, exactly like JS.
//! * `body.length` is UTF-16 length; hour features use UTC.

use std::sync::OnceLock;

use sms_types::{CATEGORY_COUNT, Category, RawSms, SenderKind, js};

pub const DEFAULT_DIM: usize = 8192;
const STRUCT_SLOTS: usize = 16;
const MAX_JS_TIME_MS: i64 = 8_640_000_000_000_000;

pub fn fnv1a32(input: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for unit in input.encode_utf16() {
        hash ^= u32::from(unit & 0xff);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[allow(clippy::too_many_lines)]
pub fn extract_features(sms: &RawSms, sender_kind: &SenderKind, dim: usize) -> Vec<(usize, f64)> {
    let mut feats: Vec<(usize, f64)> = Vec::with_capacity(160);
    let body = sms.body.as_str();
    let lower = body.to_lowercase();
    let len = js::utf16_len(body).max(1) as f64;

    let has_url = lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("bit.ly")
        || lower.contains(".in/");
    let has_currency = lower.contains("rs.") || lower.contains("inr") || body.contains('₹');
    let has_otp_kw =
        lower.contains("otp") || lower.contains("verification code") || lower.contains("passcode");
    let has_promo_kw = [
        "off",
        "sale",
        "offer",
        "discount",
        "cashback",
        "free",
        "click",
        "unsubscribe",
        "flat ",
        "apply now",
    ]
    .iter()
    .any(|k| lower.contains(k));
    let has_txn_kw = [
        "debited", "credited", "upi", "a/c", "acct", "balance", "imps", "neft", "spent",
    ]
    .iter()
    .any(|k| lower.contains(k));
    let has_rem_kw = [
        "pnr",
        "due date",
        "pay by",
        "flight",
        "train",
        "out for delivery",
    ]
    .iter()
    .any(|k| lower.contains(k));

    let mut digits = 0u32;
    let mut uppers = 0u32;
    let mut alphas = 0u32;
    for b in body.bytes() {
        match b {
            b'0'..=b'9' => digits += 1,
            b'A'..=b'Z' => {
                uppers += 1;
                alphas += 1;
            }
            b'a'..=b'z' => alphas += 1,
            _ => {}
        }
    }
    let alphas = alphas.max(1);

    let hour_frac = if sms.timestamp_ms.abs() <= MAX_JS_TIME_MS {
        let hour = sms.timestamp_ms.rem_euclid(86_400_000) / 3_600_000;
        hour as f64 / 24.0
    } else {
        0.5
    };
    let angle = std::f64::consts::PI * 2.0 * hour_frac;
    let hour_sin = angle.sin();
    let hour_cos = angle.cos();

    let (mut is_alpha_sender, mut is_phone, mut dlt_svc_txn, mut dlt_promo) = (0.0, 0.0, 0.0, 0.0);
    match sender_kind {
        SenderKind::DltHeader { suffix, .. } => {
            is_alpha_sender = 1.0;
            match suffix {
                Some(sms_types::DltSuffix::Promotional) => dlt_promo = 1.0,
                Some(_) => dlt_svc_txn = 1.0,
                None => {}
            }
        }
        SenderKind::Alphanumeric { .. } => is_alpha_sender = 1.0,
        SenderKind::PhoneNumber | SenderKind::Contact => is_phone = 1.0,
        SenderKind::Shortcode => {}
    }

    let b = |v: bool| if v { 1.0 } else { 0.0 };
    let dense: [f64; STRUCT_SLOTS] = [
        b(has_url),
        b(has_currency),
        b(has_otp_kw),
        f64::from(digits) / len,
        f64::from(uppers) / f64::from(alphas),
        (len.ln() / 6.0).min(1.5),
        b(has_promo_kw),
        b(has_txn_kw),
        b(has_rem_kw),
        is_alpha_sender,
        is_phone,
        hour_sin,
        hour_cos,
        dlt_svc_txn,
        dlt_promo,
        b(body.contains('!')),
    ];
    for (i, v) in dense.iter().enumerate() {
        if *v != 0.0 {
            feats.push((i, *v));
        }
    }

    let hash_space = (dim - STRUCT_SLOTS) as u32;
    let tokens: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|t| t.len() >= 2 && t.len() <= 24)
        .take(64)
        .collect();

    let norm = (tokens.len().max(1) as f64).sqrt();
    let slot_sign = |h: u32| {
        let slot = STRUCT_SLOTS + (h % hash_space) as usize;
        let sign = if h >> 31 == 0 { 1.0 } else { -1.0 };
        (slot, sign)
    };
    let mut bigram = String::with_capacity(50);
    for (idx, tok) in tokens.iter().enumerate() {
        let (slot, sign) = slot_sign(fnv1a32(tok));
        feats.push((slot, sign / norm));
        if let Some(next) = tokens.get(idx + 1) {
            bigram.clear();
            bigram.push_str(tok);
            bigram.push('_');
            bigram.push_str(next);
            let (bslot, bsign) = slot_sign(fnv1a32(&bigram));
            feats.push((bslot, (0.7 * bsign) / norm));
        }
    }
    feats
}

fn softmax(logits: [f64; CATEGORY_COUNT]) -> [f64; CATEGORY_COUNT] {
    let max_l = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exps = logits.map(|l| (l - max_l).exp());
    let sum = exps.iter().fold(0.0, |a, b| a + b).max(1e-9);
    exps.map(|e| e / sum)
}

pub struct LinearClassifier {
    dim: usize,
    biases: [f64; CATEGORY_COUNT],
    weights: Vec<f32>,
}

impl LinearClassifier {
    pub fn bootstrap_heuristic() -> LinearClassifier {
        let dim = DEFAULT_DIM;
        let mut weights = vec![0f32; CATEGORY_COUNT * dim];
        let biases = [0.2, -0.3, -0.2, -0.4, -0.6];

        let mut set = |cat: Category, slot: usize, val: f64| {
            weights[cat.index() * dim + slot] = val as f32;
        };
        use Category::*;
        set(Personal, 10, 2.2);
        set(Personal, 9, -2.0);
        set(Personal, 0, -1.2);
        set(Personal, 6, -1.5);
        set(Transactions, 1, 1.8);
        set(Transactions, 7, 2.5);
        set(Transactions, 13, 1.2);
        set(Promotions, 6, 2.4);
        set(Promotions, 0, 1.4);
        set(Promotions, 14, 3.0);
        set(Promotions, 15, 0.6);
        set(Priority, 2, 3.0);
        set(Priority, 8, 2.2);
        set(Priority, 13, 1.0);
        set(Spam, 0, 1.5);
        set(Spam, 6, 1.2);
        set(Spam, 10, 1.3);

        let hash_space = (dim - STRUCT_SLOTS) as u32;
        for kw in [
            "kyc",
            "lottery",
            "winner",
            "prize",
            "suspended",
            "blocked",
            "claim",
            "urgent",
            "loan",
        ] {
            let h = fnv1a32(kw);
            let slot = STRUCT_SLOTS + (h % hash_space) as usize;
            let sign = if h >> 31 == 0 { 1.0 } else { -1.0 };
            let w = &mut weights[Spam.index() * dim + slot];
            *w = (f64::from(*w) + 2.8 * sign) as f32;
        }

        LinearClassifier {
            dim,
            biases,
            weights,
        }
    }

    /// Process-wide heuristic classifier (built once, ~160 KB).
    pub fn shared() -> &'static LinearClassifier {
        static CLS: OnceLock<LinearClassifier> = OnceLock::new();
        CLS.get_or_init(LinearClassifier::bootstrap_heuristic)
    }

    pub fn predict_proba(&self, sms: &RawSms, sender_kind: &SenderKind) -> [f64; CATEGORY_COUNT] {
        let feats = extract_features(sms, sender_kind, self.dim);
        let mut logits = self.biases;
        for (c, logit) in logits.iter_mut().enumerate() {
            let base = c * self.dim;
            let mut sum = *logit;
            for &(idx, val) in &feats {
                sum += f64::from(self.weights[base + idx]) * val;
            }
            *logit = sum;
        }
        softmax(logits)
    }
}
