//! Port of index.ts `SmsEngine.analyzeMessage` — identical cascade order.

use serde::{Deserialize, Serialize};
use sms_types::{
    CATEGORY_LIST, Category, Classification, ClassificationReason, DltSuffix, ExtractedOtp,
    ExtractedReminder, ExtractedTransaction, ParsedSms, RawSms, SenderKind,
};

use crate::{dlt, finance, ml::LinearClassifier, otp, reminder, thread};

/// Result of analysing one message (ParsedSms without the echoed `sms`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub sender_kind: SenderKind,
    pub classification: Classification,
    pub otp: Option<ExtractedOtp>,
    pub transaction: Option<ExtractedTransaction>,
    pub reminder: Option<ExtractedReminder>,
}

pub struct SmsEngine {
    classifier: &'static LinearClassifier,
}

impl Default for SmsEngine {
    fn default() -> Self {
        SmsEngine {
            classifier: LinearClassifier::shared(),
        }
    }
}

fn fixed(
    category: Category,
    confidence: f64,
    scores: [f64; 5],
    reason: ClassificationReason,
) -> Classification {
    Classification {
        category,
        confidence,
        scores,
        reason,
    }
}

impl SmsEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn analyze(&self, sms: &RawSms) -> Analysis {
        let sender_kind = dlt::parse_sender(&sms.sender, sms.is_known_contact);
        let otp = otp::extract_otp(&sms.body, &sender_kind);
        let transaction = finance::extract_transaction(&sms.body);
        let reminder = reminder::extract_reminder(&sms.body);

        let classification = if sms.is_outgoing || sender_kind == SenderKind::Contact {
            fixed(
                Category::Personal,
                0.99,
                [0.99, 0.0025, 0.0025, 0.0025, 0.0025],
                ClassificationReason::ContactOrOutgoing,
            )
        } else if transaction.is_some() {
            fixed(
                Category::Transactions,
                0.98,
                [0.005, 0.98, 0.005, 0.005, 0.005],
                ClassificationReason::TxnExtractor,
            )
        } else if otp.is_some() || reminder.is_some() {
            fixed(
                Category::Priority,
                0.97,
                [0.005, 0.015, 0.005, 0.97, 0.005],
                ClassificationReason::OtpOrReminder,
            )
        } else if matches!(
            sender_kind,
            SenderKind::DltHeader {
                suffix: Some(DltSuffix::Promotional),
                ..
            }
        ) {
            fixed(
                Category::Promotions,
                0.98,
                [0.005, 0.005, 0.98, 0.005, 0.005],
                ClassificationReason::DltPromoSuffix,
            )
        } else {
            let scores = self.classifier.predict_proba(sms, &sender_kind);
            let (best_idx, best) = thread::argmax(&scores);
            Classification {
                category: CATEGORY_LIST[best_idx],
                confidence: best,
                scores,
                reason: ClassificationReason::MlLinear,
            }
        };

        Analysis {
            sender_kind,
            classification,
            otp,
            transaction,
            reminder,
        }
    }

    pub fn analyze_message(&self, sms: &RawSms) -> ParsedSms {
        let a = self.analyze(sms);
        ParsedSms {
            sms: sms.clone(),
            sender_kind: a.sender_kind,
            classification: a.classification,
            otp: a.otp,
            transaction: a.transaction,
            reminder: a.reminder,
        }
    }

    pub fn classify_thread(&self, messages: &[ParsedSms]) -> Classification {
        thread::classify_thread(messages)
    }
}
