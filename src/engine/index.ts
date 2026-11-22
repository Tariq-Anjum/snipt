import { parseSender } from './dlt';
import { extractTransaction } from './finance';
import { LinearClassifier } from './ml';
import { extractOtp } from './otp';
import { extractReminder } from './reminder';
import { classifyThread } from './thread';
import {
  CATEGORY_LIST,
  type Classification,
  type ParsedSms,
  type RawSms,
} from './types';

// Phone round 1: corrections applied after the ML stage (never to Spam).
// A service notice (lab report ready, ready for pickup, appointment
// confirmed) is Priority; a loyalty-points notice with no transaction is
// Promotions.
const SERVICE_NOTICE_RE =
  /\b(?:(?:report|results?)\s+(?:is|are)\s+(?:now\s+)?ready|ready\s+for\s+(?:pickup|collection)|appointment\s+(?:is\s+)?(?:confirmed|scheduled|booked))\b/i;
const LOYALTY_RE = /\bpoints?\b[^\n]{0,80}\b(?:redeem|redemption|adjusted|expir|earned|balance)/i;

export class SmsEngine {
  private readonly classifier: LinearClassifier;

  constructor(classifier: LinearClassifier = LinearClassifier.bootstrapHeuristic()) {
    this.classifier = classifier;
  }

  analyzeMessage(sms: RawSms): ParsedSms {
    const senderKind = parseSender(sms.sender, sms.isKnownContact);
    const otp = extractOtp(sms.body, senderKind);
    const transaction = extractTransaction(sms.body);
    const reminder = extractReminder(sms.body);

    let classification: Classification;

    if (sms.isOutgoing || senderKind.type === 'Contact') {
      classification = {
        category: 'Personal',
        confidence: 0.99,
        scores: [0.99, 0.0025, 0.0025, 0.0025, 0.0025],
        reason: 'contact_or_outgoing',
      };
    } else if (transaction !== null) {
      classification = {
        category: 'Transactions',
        confidence: 0.98,
        scores: [0.005, 0.98, 0.005, 0.005, 0.005],
        reason: 'txn_extractor',
      };
    } else if (otp !== null || reminder !== null) {
      classification = {
        category: 'Priority',
        confidence: 0.97,
        scores: [0.005, 0.015, 0.005, 0.97, 0.005],
        reason: 'otp_or_reminder',
      };
    } else if (
      senderKind.type === 'DltHeader' &&
      senderKind.suffix === 'Promotional'
    ) {
      classification = {
        category: 'Promotions',
        confidence: 0.98,
        scores: [0.005, 0.005, 0.98, 0.005, 0.005],
        reason: 'dlt_promo_suffix',
      };
    } else {
      const scores = this.classifier.predictProba(sms, senderKind);
      let bestIdx = 0;
      let bestScore = scores[0];
      for (let i = 1; i < scores.length; i += 1) {
        if (scores[i] > bestScore) {
          bestScore = scores[i];
          bestIdx = i;
        }
      }
      classification = {
        category: CATEGORY_LIST[bestIdx],
        confidence: bestScore,
        scores,
        reason: 'ml_linear',
      };
      const ml = classification.category;
      if ((ml === 'Promotions' || ml === 'Transactions') && SERVICE_NOTICE_RE.test(sms.body)) {
        classification = {
          category: 'Priority',
          confidence: 0.95,
          scores: [0.01, 0.01, 0.01, 0.95, 0.02],
          reason: 'service_rule',
        };
      } else if (ml === 'Transactions' && LOYALTY_RE.test(sms.body)) {
        classification = {
          category: 'Promotions',
          confidence: 0.95,
          scores: [0.01, 0.02, 0.95, 0.01, 0.01],
          reason: 'service_rule',
        };
      }
    }

    return {
      sms,
      senderKind,
      classification,
      otp,
      transaction,
      reminder,
    };
  }

  classifyThread(messages: ParsedSms[]): Classification {
    return classifyThread(messages);
  }
}

export const defaultEngine = new SmsEngine();
export * from './types';
export { parseSender, formatSenderKindLabel } from './dlt';
export { extractOtp } from './otp';
export { extractTransaction, detectPaymentMethod } from './finance';
export { extractReminder } from './reminder';
export { fnv1a32, extractFeatures, LinearClassifier } from './ml';
export { classifyThread } from './thread';
export * from './budget';
export * from './cleanup';
export * from './cryptoVault';
export * from './indexedDb';
export * from './translate';
export * from './backup';
