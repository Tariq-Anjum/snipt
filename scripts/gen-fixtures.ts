/**
 * Parity fixture generator: runs the TypeScript engine (the oracle) and writes
 * JSON fixtures consumed by the Rust crates. Inputs listed here may be chosen by
 * hand; every expected OUTPUT comes from the TS engine. Never edit fixtures by hand.
 *
 * Run: TZ=Asia/Kolkata npx tsx scripts/gen-fixtures.ts
 * Deterministic: fixed nowMs, and crypto.getRandomValues is replaced by a seeded
 * generator only while the backup fixture is created (test passphrase constant).
 */
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  SmsEngine,
  fnv1a32,
  getCycleWindow,
  computeBudgetStats,
  categorizeMerchantExpense,
  DEFAULT_BUDGET_LIMITS,
  DEFAULT_CYCLE_CONFIG,
  runWeeklyUselessScan,
  generateBulkHistoryMessages,
  detectMessageLanguage,
  translateSmsToEnglish,
  createEncryptedBackup,
  buildBackupFilename,
  type RawSms,
  type ParsedSms,
  type ThreadSummary,
  type WeeklyScanReport,
} from '../src/engine/index';
import {
  buildThreads,
  filterThreads,
  computeFinanceSummary,
  collectOtpItems,
  collectReminders,
  purgeExpiredOtpMessages,
  ingestIncomingSms,
  type InboxFilter,
  type StoreState,
} from '../src/engine/store';
import { createSeedMessages, SIMULATION_PRESETS } from '../src/engine/seedData';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const ENGINE_FX = join(ROOT, 'crates/sms-engine/tests/fixtures');
const CRYPTO_FX = join(ROOT, 'crates/sms-crypto/tests/fixtures');

// --- environment guard -----------------------------------------------------
const NOW_MS = new Date(2026, 10, 21, 12, 0, 0).getTime(); // 21 Nov 2026 12:00 local
if (new Date(NOW_MS).getTimezoneOffset() !== -330) {
  console.error('gen-fixtures: run with TZ=Asia/Kolkata (UTC+05:30)');
  process.exit(2);
}
const TZ_OFFSET_MIN = 330;

// --- helpers ---------------------------------------------------------------
/** RawSms with keys in types.ts declaration order, undefined keys dropped. */
function canon(s: RawSms): RawSms {
  const o: Record<string, unknown> = {};
  const keys = [
    'id', 'threadId', 'sender', 'contactName', 'body', 'timestampMs',
    'isKnownContact', 'isOutgoing', 'simSlot', 'isRead', 'isStarred',
    'protocol', 'rcsStatus', 'rcsRichCard',
  ] as const;
  for (const k of keys) {
    const v = (s as unknown as Record<string, unknown>)[k];
    if (v !== undefined) o[k] = v;
  }
  return o as unknown as RawSms;
}

function analysisOf(p: ParsedSms) {
  return {
    senderKind: p.senderKind,
    classification: p.classification,
    otp: p.otp,
    transaction: p.transaction,
    reminder: p.reminder,
  };
}

function threadRow(t: ThreadSummary) {
  return {
    threadId: t.threadId,
    sender: t.sender,
    displayName: t.displayName,
    senderKind: t.senderKind,
    latestMessageId: t.latestMessage.sms.id,
    messageIds: t.messages.map((m) => m.sms.id),
    classification: t.classification,
    unreadCount: t.unreadCount,
    hasStarred: t.hasStarred,
    isArchived: t.isArchived,
    isBlocked: t.isBlocked,
    simSlot: t.simSlot,
    isRcsCapable: t.isRcsCapable,
  };
}

function write(dir: string, name: string, data: unknown) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, name), JSON.stringify(data) + '\n');
}

function makeSms(sender: string, body: string, opts: Partial<RawSms> = {}): RawSms {
  return {
    id: 1,
    threadId: 10,
    sender,
    body,
    timestampMs: 1_763_600_000_000,
    isKnownContact: false,
    isOutgoing: false,
    simSlot: 0,
    ...opts,
  };
}

const engine = new SmsEngine();
const seed = createSeedMessages(NOW_MS).map(canon);
const state: StoreState = {
  messages: seed,
  archivedThreadIds: [],
  blockedSenders: [],
  completedReminderSmsIds: [],
};
const state2: StoreState = {
  messages: seed,
  archivedThreadIds: [110, 104],
  blockedSenders: ['+918800291844', 'jm-myntra-p'],
  completedReminderSmsIds: [1011],
};

// --- seed_raw.json (one canonical message per line: byte-stable serde check) ---
mkdirSync(ENGINE_FX, { recursive: true });
writeFileSync(
  join(ENGINE_FX, 'seed_raw.json'),
  `{"nowMs":${NOW_MS},"tzOffsetMin":${TZ_OFFSET_MIN},"messages":[\n` +
    seed.map((m) => JSON.stringify(m)).join(',\n') +
    '\n]}\n'
);

// --- bulk archive ------------------------------------------------------------
const archive = generateBulkHistoryMessages(10_000, seed, NOW_MS).map(canon);
const bulkOnly = archive.slice(seed.length);
const archiveJson = JSON.stringify(archive);
write(ENGINE_FX, 'bulk_sample.json', {
  nowMs: NOW_MS,
  targetTotal: 10_000,
  count: archive.length,
  seedCount: seed.length,
  firstGenerated: bulkOnly.slice(0, 40),
  lastGenerated: bulkOnly.slice(-10),
  archiveSha256: createHash('sha256').update(archiveJson, 'utf8').digest('hex'),
});

// --- analyze.json ------------------------------------------------------------
const hazardInputs: RawSms[] = [
  // \b next to non-ASCII letters (JS \b is ASCII-only)
  makeSms('VM-SBIBNK-T', 'éOTP 482913é is here'),
  makeSms('VM-SBIBNK-T', 'Code: ñ123456ñ use it'),
  makeSms('VM-SBIBNK-T', 'पासवर्ड OTP है 739104 कृपया'),
  makeSms('AD-GOOGLE-T', 'G-392814é verification'),
  // currency false-positive prefixes
  makeSms('VM-SBIBNK-T', 'Your OTP for ₹ 4821 payment'),
  makeSms('VM-SBIBNK-T', 'Pay INR 2026 now, OTP 9921'),
  makeSms('VM-SBIBNK-T', 'card ending 2031 is your code'),
  makeSms('VM-SBIBNK-T', '2031 is your login code'),
  // emoji (UTF-16 surrogate pairs) and U+FEFF
  makeSms('﻿AX-HDFCBK-S﻿', '😀 Sent Rs.120.00 to 😀 CAFE 😀 via UPI Ref 631000000001'),
  makeSms('VM-AMAZON\u0085', 'Out for delivery 📦 arriving today'),
  makeSms('　JM-ZOMATO-P', 'Flat 40% off 🍕🍕🍕 on pizza!'),
  makeSms('+91 98450 12345', 'ok see you'),
  makeSms('56767', 'Reply STOP'),
  makeSms('ab-xyz12-g', 'Govt notice: due date is 5/Dec/2026'),
  makeSms('AB XYZ12-T', 'Payment due by 12 Jan'),
  makeSms('INDIGO-RCS', 'PNR no. ab12cd34 confirmed'),
  // case-folding and Unicode lowercase
  makeSms('VM-HDFCBK-T', 'İSTANBUL ŞALE FREE OFFER Click NOW!!!'),
  makeSms('VM-HDFCBK', 'ΣΊΣΥΦΟΣ lottery WINNER claim prize'),
  makeSms('JD-ICICIB-S', 'INR 1,23,456.78 CREDITED to A/C no. XX1234 Avl Bal INR 9,99,999.00 UTR 123456789012'),
  makeSms('JD-ICICIB-S', 'Rs 50 refunded and Rs 20 debited at STARBUCKS on 3 Nov'),
  makeSms('JD-ICICIB-S', 'Rs.0.00 debited'),
  makeSms('AX-AMEXIN-S', 'Charged INR 999 on your Amex card xx4002 at NETFLIX. Avl lmt INR 50,000'),
  makeSms('AX-PAYTM-S', 'Paid Rs.75 to chai.wala@okaxis via paytm'),
  makeSms('AX-SBIUPI-S', 'Sent Rs.300 to Ravi Ref 123456789012'),
  makeSms('+919999999999', 'urgent kyc suspended loan claim http://bit.ly/x', { timestampMs: 0 }),
  makeSms('+919999999999', 'hello', { timestampMs: 1_763_651_400_000 }),
  makeSms('Friend', 'Outgoing hi', { isOutgoing: true }),
  makeSms('', ''),
  makeSms('   ', '   ﻿  '),
  makeSms('TX-ZZ-P', 'two-letter entity (not DLT)'),
  makeSms('TXZOMATO-P', 'no dash DLT'),
  // Phone round 1: real-world shapes (names/numbers changed)
  makeSms('BP-ICICIT-S', 'USD 5.54 spent using ICICI Bank Card XX1111 on 18-Nov-26 on OPENTOOL. Avl Limit: INR 95,221.00. If not you, call 1800 2662/SMS BLOCK 1111 to 9200000000'),
  makeSms('BP-ICICIT-S', 'ICICI Bank Credit Card XX2222 debited for INR 76.00 on 22-Nov-26 for UPI-151641006879-RAVI KUM. To dispute call 18001080/SMS BLOCK 2222 to 9200000000'),
  makeSms('BP-ICICIT-S', 'ICICI Bank Acct XX333 debited for Rs 25.00 on 21-Nov-26; ASHA DEVI credited. UPI:134225588727. Call 18002662 for dispute. SMS BLOCK 333 to 9200000000'),
  makeSms('VM-BSESRP-S', 'CA No. 000151813123 , Rs. 680 is due on 26-NOV-26. To view & pay your bill click https://example.in/x .Ignore if paid . Team BRPL'),
  makeSms('VM-BSESRP-S', 'Dear Customer, payment against CA NO. 150214035 of Rs. 890.0 / has been received, subject to realization & updation. Team BRPL'),
  makeSms('AX-HDFCBK-S', 'Rs 199 will be debited from your A/c XX4444 on 25-Nov for mandate NETFLIX'),
  makeSms('AX-HDFCBK-S', 'EMI of Rs 5,000 debited from A/c XX4444. Next EMI is due on 5-Dec'),
  makeSms('AX-HDFCBK-S', 'Payment of Rs 12,000 received towards your Credit Card XX5555. Thank you'),
  makeSms('BP-ICICIT-S', 'ICICI Bank Acc XX333 debited Rs. 20,086.06 on 04-Nov-26 InfoBIL*INFT*FK47.Avl Bal Rs. 36,303.68.To dispute call 18002662'),
  makeSms('VM-TALUXE-S', 'Hi, Your wallet has been credited with Rs. 1,000* Seiko Power Cash. View your balance now: https://example.in/x Tata CLiQ Luxury'),
  makeSms('VM-RETAIL-S', 'Dear Customer, from 1 Dec 2026, 1 point = Rs 1. Your existing points will be adjusted accordingly, while their total redemption value stays'),
  makeSms('VM-LABXYZ-S', 'Dear Customer, Lab No 559000, your report is ready. You can collect from counter / download from https://example.org/'),
  makeSms('BP-ICICIT-S', 'MICROSOFT INDIA refund of Rs 2.00 credited to ICICI Bank Credit Card XX1111 on 29-OCT-26. Revised total due Rs 20,086.06, minimum due Rs 12,348.55'),
];
const analyzeInputs = [...seed, ...bulkOnly.slice(0, 2000), ...hazardInputs.map(canon)];
const fnvInputs = [
  '', 'foobar', 'a', 'kyc', 'lottery', '₹', 'हिन्दी', 'اردو', '😀', 'aĀb', 'é', 'İ',
  'ok_bye', 'flat_50', '﻿', 'x'.repeat(100),
];
write(ENGINE_FX, 'analyze.json', {
  nowMs: NOW_MS,
  fnv: fnvInputs.map((s) => ({ s, h: fnv1a32(s) })),
  cases: analyzeInputs.map((sms) => ({ input: sms, output: analysisOf(engine.analyzeMessage(sms)) })),
});

// --- threads.json --------------------------------------------------------------
const threads = buildThreads(state);
const threads2 = buildThreads(state2);
const filters: InboxFilter[] = [
  'All', 'Personal', 'Transactions', 'Promotions', 'Priority', 'Spam', 'RCS', 'Starred', 'Archived',
];
const sims: Array<'all' | 0 | 1> = ['all', 0, 1];
const queries = ['', 'hdfc', 'rcs', 'sms', 'otp 849201', 'swiggy instamart', 'UPI', '  Aarav  ', 'xk94m2', 'zz', 'credit card', '₹', 'شکریہ'];
const filterCases: unknown[] = [];
for (const [stateName, ts] of [['base', threads], ['blocked', threads2]] as const) {
  for (const f of filters) {
    for (const sim of sims) {
      for (const q of queries) {
        filterCases.push({
          state: stateName,
          filter: f,
          sim,
          query: q,
          threadIds: filterThreads(ts, f, sim, q).map((t) => t.threadId),
        });
      }
    }
  }
}
const purged = purgeExpiredOtpMessages(state, NOW_MS);
const keptIds = new Set(purged.nextState.messages.map((m) => m.id));
const ingestA = ingestIncomingSms(purged.nextState, {
  sender: ' VM-AIRTEL-T ',
  body: ' 492011 is your Airtel OTP. ',
  isKnownContact: false,
  simSlot: 0,
  timestampMs: NOW_MS + 1000,
});
const ingestB = ingestIncomingSms(state, {
  sender: 'ax-hdfcbk-s',
  body: 'Sent Rs.10.00 via UPI',
  isKnownContact: false,
  simSlot: 1,
  timestampMs: NOW_MS + 2000,
});
write(ENGINE_FX, 'threads.json', {
  nowMs: NOW_MS,
  states: {
    base: { archivedThreadIds: state.archivedThreadIds, blockedSenders: state.blockedSenders, completedReminderSmsIds: state.completedReminderSmsIds },
    blocked: { archivedThreadIds: state2.archivedThreadIds, blockedSenders: state2.blockedSenders, completedReminderSmsIds: state2.completedReminderSmsIds },
  },
  threads: { base: threads.map(threadRow), blocked: threads2.map(threadRow) },
  filters: filterCases,
  otpItems: {
    base: collectOtpItems(threads, NOW_MS).map((o) => ({ smsId: o.smsId, threadId: o.threadId, sender: o.sender, expiresAtMs: o.expiresAtMs, isExpired: o.isExpired, code: o.otp.code })),
    blocked: collectOtpItems(threads2, NOW_MS).map((o) => ({ smsId: o.smsId, threadId: o.threadId, sender: o.sender, expiresAtMs: o.expiresAtMs, isExpired: o.isExpired, code: o.otp.code })),
  },
  reminders: {
    base: collectReminders(threads, state.completedReminderSmsIds).map((r) => ({ smsId: r.smsId, threadId: r.threadId, sender: r.sender, isDone: r.isDone, kind: r.reminder.kind })),
    blocked: collectReminders(threads2, state2.completedReminderSmsIds).map((r) => ({ smsId: r.smsId, threadId: r.threadId, sender: r.sender, isDone: r.isDone, kind: r.reminder.kind })),
  },
  purge: { purgedCount: purged.purgedCount, purgedIds: seed.filter((m) => !keptIds.has(m.id)).map((m) => m.id) },
  ingest: [canon(ingestA.createdSms), canon(ingestB.createdSms)],
});

// --- cycles.json -----------------------------------------------------------------
const cycleDays = [1, 15, 28, 0, 31, 14.6, -3];
const cycleCases: unknown[] = [];
const filenames: unknown[] = [];
for (const [y, months] of [[2026, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]], [2027, [0]], [2028, [1, 2]]] as const) {
  for (const m of months) {
    const last = new Date(y, m + 1, 0).getDate();
    for (const d of [1, 14, 15, 27, 28, last]) {
      for (const [hh, mm, ss, ms] of [[0, 0, 0, 0], [12, 0, 0, 0], [23, 59, 59, 999]]) {
        const now = new Date(y, m, d, hh, mm, ss, ms).getTime();
        for (const day of cycleDays) {
          const w = getCycleWindow(day, now);
          cycleCases.push({ cycleStartDay: day, nowMs: now, ...w });
        }
        if (hh === 0 || hh === 23) filenames.push({ nowMs: now, name: buildBackupFilename(now) });
      }
    }
  }
}
write(ENGINE_FX, 'cycles.json', { tzOffsetMin: TZ_OFFSET_MIN, cases: cycleCases, backupFilenames: filenames });

// --- budget.json -------------------------------------------------------------------
const ccCycle12 = { ...DEFAULT_CYCLE_CONFIG, 'Credit Card': 12 };
const finances = [
  { state: 'base', cycleConfig: DEFAULT_CYCLE_CONFIG, onlyActiveCycle: true, summary: computeFinanceSummary(threads, DEFAULT_CYCLE_CONFIG, NOW_MS, true) },
  { state: 'base', cycleConfig: DEFAULT_CYCLE_CONFIG, onlyActiveCycle: false, summary: computeFinanceSummary(threads, DEFAULT_CYCLE_CONFIG, NOW_MS, false) },
  { state: 'base', cycleConfig: ccCycle12, onlyActiveCycle: true, summary: computeFinanceSummary(threads, ccCycle12, NOW_MS, true) },
  { state: 'blocked', cycleConfig: DEFAULT_CYCLE_CONFIG, onlyActiveCycle: true, summary: computeFinanceSummary(threads2, DEFAULT_CYCLE_CONFIG, NOW_MS, true) },
];
const limitSets = [
  DEFAULT_BUDGET_LIMITS,
  { ...DEFAULT_BUDGET_LIMITS, 'Food & Dining': 5000 },
  { ...DEFAULT_BUDGET_LIMITS, 'Shopping & Retail': 0, 'Transport & Travel': 1000 },
];
const budgetCases: unknown[] = [];
finances.forEach((f, fi) => {
  for (const limits of limitSets) {
    for (const only of [true, false]) {
      budgetCases.push({ financeIndex: fi, limits, onlyActiveCycle: only, stats: computeBudgetStats(f.summary.transactions, limits, only) });
    }
  }
});
const merchantInputs: Array<[string | null, string]> = [
  ['SWIGGY INSTAMART', ''], [null, 'Paid to UBER'], ['VI Store', ''], ['x', 'vi recharge done'],
  [null, 'nothing matches'], ['Apple Music', 'monthly'], [null, 'GAS bill'], ['İkea', ''],
];
write(ENGINE_FX, 'budget.json', {
  nowMs: NOW_MS,
  finances,
  budgets: budgetCases,
  merchants: merchantInputs.map(([m, b]) => ({ merchant: m, body: b, category: categorizeMerchantExpense(m, b) })),
});

// --- scan.json ----------------------------------------------------------------------
function scanSummary(r: WeeklyScanReport) {
  return {
    scannedAtMs: r.scannedAtMs,
    totalScanned: r.totalScanned,
    totalUselessCount: r.totalUselessCount,
    totalEstimatedBytes: r.totalEstimatedBytes,
    buckets: r.buckets.map((b) => ({
      id: b.id, title: b.title, description: b.description, count: b.messageIds.length,
      idsSha256: createHash('sha256').update(JSON.stringify(b.messageIds)).digest('hex'),
      estimatedBytes: b.estimatedBytes, samples: b.samples,
    })),
  };
}
// Safety regression: transactions and active reminders from blocked senders are never flagged.
const SCAN_DAY = 24 * 60 * 60 * 1000;
const scanSafetyExtra: RawSms[] = [
  ['Rs.2,500.00 debited from HDFC Bank A/C XX4821 via UPI to SWIGGY on 20-Nov-26. Ref 632418902311. Avl Bal Rs.9,840.25', 10],
  ['Your electricity bill of Rs.1,240 is due. Pay by 28 Nov to avoid a late fee.', 10],
  ['PNR: 4521890341 Train: 12952 RAJDHANI EXP Confirmed B3/42 Dep 22-Nov 16:55.', 30],
  ['Out for delivery: your order will be delivered today by 5:00 PM.', 2],
  ['Out for delivery: your order will be delivered today by 7:00 PM.', 12],
  ['URGENT: your KYC is pending, account will be blocked. Click bit.ly/kyc-upd8 to verify.', 10],
].map(([body, days], i) => canon({
  id: 9001 + i, threadId: 900, sender: 'XX-SCAMMR', body: body as string,
  timestampMs: NOW_MS - (days as number) * SCAN_DAY, isKnownContact: false, isOutgoing: false, simSlot: 0,
}));
const scanSafetyMessages = [...seed, ...scanSafetyExtra];
const scanSafetyBlocked = [...new Set(scanSafetyMessages.map((m) => m.sender))];
write(ENGINE_FX, 'scan.json', {
  nowMs: NOW_MS,
  seed: runWeeklyUselessScan(seed, [], NOW_MS),
  seedBlocked: { blockedSenders: ['JM-myntra-p', 'VM-AMAZON-S'], report: runWeeklyUselessScan(seed, ['JM-myntra-p', 'VM-AMAZON-S'], NOW_MS) },
  blockedSafety: { extra: scanSafetyExtra, blockedSenders: scanSafetyBlocked, report: runWeeklyUselessScan(scanSafetyMessages, scanSafetyBlocked, NOW_MS) },
  bulk10k: scanSummary(runWeeklyUselessScan(archive, [], NOW_MS)),
});

// --- translate.json -------------------------------------------------------------------
const translateInputs = [
  ...seed.map((m) => m.body),
  ...SIMULATION_PRESETS.map((p) => p.body),
  'Sent Rs.450.50 via UPI to Swiggy.',
  'आपका OTP 482910 है। किसी के साथ साझा न करें। धन्यवाद।',
  'السلام علیکم، میں گھر پہنچ گیا ہوں۔ کل شام 7 بجے ملتے ہیں۔ شکریہ۔',
  'Bhai kal shaam 8 baje milte hain khana khane, paise bhej diye hain.',
  'तुमचे पार्सल आज संध्याकाळी 6 पर्यंत पोहोचेल. धन्यवाद.',
  'உங்கள் ஆர்டர் இன்று மாலை டெலிவரி செய்யப்படும். நன்றி.',
  'మీ ఆర్డర్ ఈరోజు సాయంత్రం డెలివరీ అవుతుంది. ధన్యవాదాలు.',
  'আপনার অর্ডার আজ সন্ধ্যায় ডেলিভারি করা হবে। ধন্যবাদ।',
  'आपका ओटीपी 1234 है',
  'आपके खाते से 500 रुपये कट गए हैं। उपलब्ध बैलेंस 9,000 रु. है',
  'आपका ऑर्डर आज शाम 7 बजे तक डिलीवर हो जाएगा',
  'आप का OTP ٹیسٹ',
  'آپ کا OTP 4410 ہے۔ کسی کو نہ بتائیں',
  'آپ کا او ٹی پی 9090 ہے',
  'kal shaam 7 baje milte hain bhai',
  'AAPKA OTP 7781 hai, kisi ko mat batayein',
  'Yaar jaldi aana, ghar pahunch raha hoon',
  'aapke khate se 200 rupaye kat gaye hain',
  'kya haal hai bhai? theek hai',
  'kal milte',
  'Kal',
  'तुमच्या खात्यातून 300 रुपये डेबिट झाले आहेत. उपलब्ध शिल्लक 1,000 रुपये',
  'உங்கள் OTP 5521. யாருடனும் பகிர வேண்டாம். தயவுசெய்து ரூ. 50',
  'మీ ఖాతా నుండి రూ. 100. ఎవరితోనూ పంచుకోవద్దు',
  'আপনার অ্যাকাউন্ট থেকে 200 টাকা. কারো সাথে শেয়ার করবেন না',
  'ok',
  '',
  '   ',
  '﻿Bhai kal﻿',
  'ß bhai kal aaj',
  'a　bhai　kal',
  'x ,  . !  ? Rs.   5',
  'ح',
  'ஆ ஆ',
];
write(ENGINE_FX, 'translate.json', {
  cases: translateInputs.map((text) => ({ text, detected: detectMessageLanguage(text), result: translateSmsToEnglish(text) })),
});

// --- sms-crypto/tests/fixtures/ts_backup.mysms.enc ---------------------------------------
// Test passphrase constant (same literal as engine.test.ts); not a secret.
const TEST_PASSPHRASE = 'my-secret-pass-2026';
let rngState = 0x6d79534d; // "mySM"
const origGetRandomValues = crypto.getRandomValues.bind(crypto);
(crypto as { getRandomValues: unknown }).getRandomValues = <T extends ArrayBufferView | null>(arr: T): T => {
  const view = arr as unknown as Uint8Array;
  for (let i = 0; i < view.length; i += 1) {
    rngState = (Math.imul(rngState, 1664525) + 1013904223) >>> 0;
    view[i] = rngState >>> 24;
  }
  return arr;
};
try {
  const { serialized } = await createEncryptedBackup(
    {
      schemaVersion: 1,
      app: 'mySMS',
      createdAtMs: NOW_MS,
      provider: 'onedrive',
      messages: seed,
      archivedThreadIds: [110],
      blockedSenders: ['+918800291844'],
      completedReminderSmsIds: [1011],
      cycleConfig: { ...DEFAULT_CYCLE_CONFIG, 'Credit Card': 12 },
      budgetLimits: { ...DEFAULT_BUDGET_LIMITS, 'Food & Dining': 6500 },
      scanScheduleDay: 'Every Sunday · 9:00 AM',
      autoTranslateToEnglish: true,
    },
    TEST_PASSPHRASE
  );
  mkdirSync(CRYPTO_FX, { recursive: true });
  writeFileSync(join(CRYPTO_FX, 'ts_backup.mysms.enc'), serialized);
} finally {
  (crypto as { getRandomValues: unknown }).getRandomValues = origGetRandomValues;
}

console.log(`gen-fixtures: wrote fixtures (seed=${seed.length}, analyze=${analyzeInputs.length}, cycles=${cycleCases.length})`);
