import { defaultEngine, type SmsEngine } from './index';
import type { RawSms } from './types';

export type CleanupBucketId =
  | 'expired_otps'
  | 'old_promotions'
  | 'spam_phishing'
  | 'old_deliveries';

export interface CleanupBucket {
  id: CleanupBucketId;
  title: string;
  description: string;
  messageIds: number[];
  estimatedBytes: number;
  samples: Array<{
    smsId: number;
    sender: string;
    body: string;
    timestampMs: number;
  }>;
}

export interface WeeklyScanReport {
  scannedAtMs: number;
  totalScanned: number;
  totalUselessCount: number;
  totalEstimatedBytes: number;
  buckets: CleanupBucket[];
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Performs a weekly hygiene scan across the entire SMS history (including 10,000+ SMS)
 * and groups safe-to-delete messages into reviewable categories.
 * Never flags Personal contacts, Transactions, Bill Due / Travel PNRs, or Starred SMS.
 */
export function runWeeklyUselessScan(
  messages: RawSms[],
  blockedSenders: string[],
  nowMs: number,
  engine: SmsEngine = defaultEngine
): WeeklyScanReport {
  const blockedSet = new Set(blockedSenders.map((s) => s.toLowerCase()));

  const expiredOtps: CleanupBucket = {
    id: 'expired_otps',
    title: 'Expired OTPs',
    description: 'One-time codes older than 24 hours with no transaction or reminder attached.',
    messageIds: [],
    estimatedBytes: 0,
    samples: [],
  };

  const oldPromotions: CleanupBucket = {
    id: 'old_promotions',
    title: 'Old Promotions & Offers',
    description: 'Promotional DLT (-P) ads, flash sales, and coupon alerts older than 7 days.',
    messageIds: [],
    estimatedBytes: 0,
    samples: [],
  };

  const spamPhishing: CleanupBucket = {
    id: 'spam_phishing',
    title: 'Spam & Phishing Alerts',
    description: 'Blocked senders, KYC scams, lottery bait, and suspicious loan links.',
    messageIds: [],
    estimatedBytes: 0,
    samples: [],
  };

  const oldDeliveries: CleanupBucket = {
    id: 'old_deliveries',
    title: 'Expired Delivery Alerts',
    description: 'Past package out-for-delivery updates older than 7 days.',
    messageIds: [],
    estimatedBytes: 0,
    samples: [],
  };

  const addToBucket = (bucket: CleanupBucket, sms: RawSms) => {
    bucket.messageIds.push(sms.id);
    // Approximate SQLite row + FTS5 index footprint per SMS (~240 bytes overhead + body)
    bucket.estimatedBytes += 240 + sms.body.length * 2 + sms.sender.length * 2;
    if (bucket.samples.length < 3) {
      bucket.samples.push({
        smsId: sms.id,
        sender: sms.sender,
        body: sms.body,
        timestampMs: sms.timestampMs,
      });
    }
  };

  for (const sms of messages) {
    if (sms.isStarred || sms.isOutgoing || sms.isKnownContact) continue;

    const ageMs = nowMs - sms.timestampMs;
    const isBlocked = blockedSet.has(sms.sender.toLowerCase());
    const parsed = engine.analyzeMessage(sms);

    // Safety exclusions come first and beat every bucket, including blocked senders
    // and Spam: never auto-delete financial transactions or active reminders
    // (bill due / travel PNR at any age, deliveries younger than 7 days).
    if (parsed.transaction) continue;
    if (parsed.reminder && (parsed.reminder.kind !== 'Delivery' || ageMs < 7 * DAY_MS)) continue;

    if (isBlocked || parsed.classification.category === 'Spam') {
      addToBucket(spamPhishing, sms);
      continue;
    }

    if (parsed.otp && !parsed.reminder && ageMs >= parsed.otp.ttlSeconds * 1000) {
      addToBucket(expiredOtps, sms);
      continue;
    }

    if (parsed.reminder?.kind === 'Delivery' && ageMs >= 7 * DAY_MS) {
      addToBucket(oldDeliveries, sms);
      continue;
    }

    if (parsed.classification.category === 'Promotions' && ageMs >= 7 * DAY_MS) {
      addToBucket(oldPromotions, sms);
    }
  }

  const buckets = [expiredOtps, oldPromotions, spamPhishing, oldDeliveries];
  const totalUselessCount = buckets.reduce((sum, b) => sum + b.messageIds.length, 0);
  const totalEstimatedBytes = buckets.reduce((sum, b) => sum + b.estimatedBytes, 0);

  return {
    scannedAtMs: nowMs,
    totalScanned: messages.length,
    totalUselessCount,
    totalEstimatedBytes,
    buckets,
  };
}
