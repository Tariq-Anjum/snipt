import test from 'node:test';
import assert from 'node:assert/strict';
import {
  SmsEngine,
  parseSender,
  extractOtp,
  extractTransaction,
  extractReminder,
  fnv1a32,
  getCycleWindow,
  computeBudgetStats,
  DEFAULT_BUDGET_LIMITS,
  DEFAULT_CYCLE_CONFIG,
  runWeeklyUselessScan,
  generateBulkHistoryMessages,
  encryptOtpCode,
  decryptOtpCode,
  detectMessageLanguage,
  translateSmsToEnglish,
  createEncryptedBackup,
  restoreEncryptedBackup,
  type RawSms,
} from './index';
import {
  buildThreads,
  filterThreads,
  computeFinanceSummary,
  collectOtpItems,
  purgeExpiredOtpMessages,
  deleteMessagesByIds,
  ingestIncomingSms,
  type StoreState,
} from './store';
import { createSeedMessages } from './seedData';

function makeSms(
  sender: string,
  body: string,
  opts: Partial<RawSms> = {}
): RawSms {
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

test('TRAI DLT sender parsing distinguishes suffixes, legacy headers, phone numbers, and contacts', () => {
  assert.deepEqual(parseSender('AX-HDFCBK-S', false), {
    type: 'DltHeader',
    prefix: 'AX',
    entity: 'HDFCBK',
    suffix: 'Service',
  });
  assert.deepEqual(parseSender('JM-ZOMATO-P', false), {
    type: 'DltHeader',
    prefix: 'JM',
    entity: 'ZOMATO',
    suffix: 'Promotional',
  });
  assert.deepEqual(parseSender('VM-AMAZON', false), {
    type: 'DltHeader',
    prefix: 'VM',
    entity: 'AMAZON',
    suffix: null,
  });
  assert.deepEqual(parseSender('+919845012345', true), { type: 'Contact' });
  assert.deepEqual(parseSender('+919845012345', false), { type: 'PhoneNumber' });
  assert.deepEqual(parseSender('567678', false), { type: 'Shortcode' });
});

test('OTP extractor parses SBI, Google G-codes and rejects currency or card false positives', () => {
  const sbiSender = parseSender('VM-SBIBNK-T', false);
  const otp = extractOtp(
    '849201 is your OTP for transaction of Rs. 2,500 on SBI Card ending 9912. Valid for 5 mins.',
    sbiSender
  );
  assert.ok(otp);
  assert.equal(otp.code, '849201');
  assert.equal(otp.serviceHint, 'SBIBNK');

  const gOtp = extractOtp(
    'G-392814 is your Google verification code.',
    parseSender('AD-GOOGLE-T', false)
  );
  assert.ok(gOtp);
  assert.equal(gOtp.code, '392814');

  const falsePos = extractOtp(
    'Your card ending 9912 was charged Rs. 2500 in 2026.',
    sbiSender
  );
  assert.equal(falsePos, null);
});

test('Finance extractor identifies Indian UPI, Credit Card, and Savings Account cleanly', () => {
  const upi = extractTransaction(
    'Sent Rs.450.50 From HDFC Bank A/C *4821 To SWIGGY INSTAMART On 20/11/26 Ref 632418902311. Avl Bal Rs.12,340.25'
  );
  assert.ok(upi);
  assert.equal(upi.paymentMethod, 'UPI');
  assert.equal(upi.isUpi, true);

  const cc = extractTransaction(
    'Spent Rs.5,400.00 on ICICI Bank Coral Credit Card XX7104 at BLUE TOKAI ROASTERS on 19-Nov-26. Avl Lmt Rs.1,84,600.00.'
  );
  assert.ok(cc);
  assert.equal(cc.paymentMethod, 'Credit Card');
  assert.equal(cc.availableBalance, 184600);

  const savings = extractTransaction(
    'Rs.3,200.00 debited from ICICI Bank Savings A/C XX7104 towards BESCOM ELECTRICITY via NEFT on 17-Nov-26. Avl Bal Rs.1,15,270.00.'
  );
  assert.ok(savings);
  assert.equal(savings.paymentMethod, 'Savings Account');
  assert.equal(savings.isUpi, false);

  const promoLoan = extractTransaction(
    'Pre-approved Rs. 5,00,000 instant loan! Credited in 5 mins. Apply now!'
  );
  assert.equal(promoLoan, null);
});

test('Monthly billing cycle calculation and Budget Planner auto-categorization work accurately', () => {
  const now = new Date(2026, 10, 21, 12, 0, 0).getTime(); // 21 Nov 2026
  const ccCycle = getCycleWindow(15, now);
  assert.equal(new Date(ccCycle.startMs).getDate(), 15);
  assert.equal(new Date(ccCycle.startMs).getMonth(), 10);

  const state: StoreState = {
    messages: createSeedMessages(now),
    archivedThreadIds: [],
    blockedSenders: [],
    completedReminderSmsIds: [],
  };
  const threads = buildThreads(state);
  const finance = computeFinanceSummary(threads, DEFAULT_CYCLE_CONFIG, now, true);

  assert.ok(finance.upiDebitTotal > 3000);
  assert.ok(finance.creditCardDebitTotal > 9000);
  assert.equal(finance.savingsDebitTotal, 3200);

  const budget = computeBudgetStats(finance.transactions, {
    ...DEFAULT_BUDGET_LIMITS,
    'Food & Dining': 5000, // Spent ~5850.50 -> Over budget
  });
  const food = budget.categories.find((c) => c.category === 'Food & Dining');
  assert.ok(food);
  assert.equal(food.status, 'Over budget');
  assert.ok(food.spent > 5000);
});

test('Weekly useless SMS scan categorizes old promos, expired OTPs, spam, and old deliveries safely', () => {
  const now = new Date(2026, 10, 21, 12, 0, 0).getTime();
  const state: StoreState = {
    messages: createSeedMessages(now),
    archivedThreadIds: [],
    blockedSenders: [],
    completedReminderSmsIds: [],
  };

  const report = runWeeklyUselessScan(state.messages, state.blockedSenders, now);
  assert.ok(report.totalUselessCount >= 6);

  const expiredOtps = report.buckets.find((b) => b.id === 'expired_otps')!;
  const oldPromos = report.buckets.find((b) => b.id === 'old_promotions')!;
  const spam = report.buckets.find((b) => b.id === 'spam_phishing')!;
  const oldDeliveries = report.buckets.find((b) => b.id === 'old_deliveries')!;

  assert.equal(expiredOtps.messageIds.length, 2);
  assert.equal(oldPromos.messageIds.length, 3);
  assert.equal(spam.messageIds.length, 1);
  assert.equal(oldDeliveries.messageIds.length, 1);

  // Verify Transactions and Personal contacts are never flagged
  const allFlagged = new Set(report.buckets.flatMap((b) => b.messageIds));
  assert.equal(allFlagged.has(1001), false); // HDFC UPI
  assert.equal(allFlagged.has(1008), false); // Personal RCS

  const { nextState, deletedCount } = deleteMessagesByIds(
    state,
    oldPromos.messageIds
  );
  assert.equal(deletedCount, 3);
  assert.equal(nextState.messages.length, state.messages.length - 3);
});

test('Weekly scan: transaction and active-reminder exclusions beat blocked senders and Spam', () => {
  const now = new Date(2026, 10, 21, 12, 0, 0).getTime();
  const DAY = 24 * 60 * 60 * 1000;
  const mk = (id: number, body: string, ageMs: number): RawSms => ({
    id, threadId: 900, sender: 'XX-SCAMMR', body, timestampMs: now - ageMs,
    isKnownContact: false, isOutgoing: false, simSlot: 0,
  });
  const upi = mk(9001, 'Rs.2,500.00 debited from HDFC Bank A/C XX4821 via UPI to SWIGGY on 20-Nov-26. Ref 632418902311. Avl Bal Rs.9,840.25', 10 * DAY);
  const bill = mk(9002, 'Your electricity bill of Rs.1,240 is due. Pay by 28 Nov to avoid a late fee.', 10 * DAY);
  const pnr = mk(9003, 'PNR: 4521890341 Train: 12952 RAJDHANI EXP Confirmed B3/42 Dep 22-Nov 16:55.', 30 * DAY);
  const freshDelivery = mk(9004, 'Out for delivery: your order will be delivered today by 5:00 PM.', 2 * DAY);
  const oldDelivery = mk(9005, 'Out for delivery: your order will be delivered today by 7:00 PM.', 12 * DAY);
  const engine = new SmsEngine();
  assert.ok(engine.analyzeMessage(upi).transaction);
  assert.equal(engine.analyzeMessage(bill).reminder?.kind, 'BillDue');
  assert.equal(engine.analyzeMessage(pnr).reminder?.kind, 'TravelPnr');
  assert.equal(engine.analyzeMessage(freshDelivery).reminder?.kind, 'Delivery');

  const seed = createSeedMessages(now);
  const messages = [...seed, upi, bill, pnr, freshDelivery, oldDelivery];
  const everyone = [...new Set(messages.map((m) => m.sender))];
  const report = runWeeklyUselessScan(messages, everyone, now, engine);
  const flagged = new Set(report.buckets.flatMap((b) => b.messageIds));
  for (const id of [9001, 9002, 9003, 9004, 1001]) assert.equal(flagged.has(id), false, `id ${id}`);
  assert.equal(flagged.has(9005), true); // expired delivery from a blocked sender stays deletable
  for (const m of messages.filter((x) => flagged.has(x.id))) {
    const p = engine.analyzeMessage(m);
    assert.equal(p.transaction, null);
    assert.ok(!p.reminder || (p.reminder.kind === 'Delivery' && now - m.timestampMs >= 7 * DAY));
  }
});

test('10,000+ SMS history generator produces at least 10,000 messages cleanly and fast', () => {
  const now = Date.now();
  const seed = createSeedMessages(now);
  const start = performance.now();
  const archive = generateBulkHistoryMessages(10_000, seed, now);
  const elapsed = performance.now() - start;

  assert.equal(archive.length, 10_000);
  assert.ok(elapsed < 1000, `Expected <1s generation, got ${elapsed}ms`);
});

test('AES-256-GCM OTP encryption vault encrypts and decrypts cleanly', async () => {
  const encrypted = await encryptOtpCode('849201');
  assert.equal(encrypted.algorithm, 'AES-256-GCM');
  assert.notEqual(encrypted.ciphertextB64, '849201');
  const plain = await decryptOtpCode(encrypted);
  assert.equal(plain, '849201');
});

test('Reminder extractor, FNV-1a hash, RCS filter, and OTP purge work end-to-end', () => {
  assert.equal(fnv1a32(''), 0x811c9dc5);
  assert.equal(fnv1a32('foobar'), 0xbf9cf968);

  const pnr = extractReminder('PNR: 4521890341 Train: 12952 RAJDHANI EXP.');
  assert.ok(pnr);
  assert.equal(pnr.reference, '4521890341');

  const engine = new SmsEngine();
  const promo = engine.analyzeMessage(
    makeSms('JM-ZOMATO-P', 'Flat 60% OFF tonight! Order https://zoma.to/deal')
  );
  assert.equal(promo.classification.category, 'Promotions');

  const now = new Date(2026, 10, 21, 12, 0, 0).getTime();
  const state: StoreState = {
    messages: createSeedMessages(now),
    archivedThreadIds: [],
    blockedSenders: [],
    completedReminderSmsIds: [],
  };
  const threads = buildThreads(state);
  const rcsThreads = filterThreads(threads, 'RCS', 'all', '');
  assert.equal(rcsThreads.length, 4); // Aarav, IndiGo RCS, Priya, Zaid Khan (Urdu RCS)

  const otpsBefore = collectOtpItems(threads, now);
  assert.ok(otpsBefore.some((o) => o.isExpired));

  const { nextState, purgedCount } = purgeExpiredOtpMessages(state, now);
  assert.equal(purgedCount, 2);

  const { createdSms } = ingestIncomingSms(nextState, {
    sender: 'VM-AIRTEL-T',
    body: '492011 is your Airtel OTP.',
    isKnownContact: false,
    simSlot: 0,
    timestampMs: now + 1000,
  });
  assert.equal(createdSms.sender, 'VM-AIRTEL-T');
});

test('Offline language detector and English translator handle Hindi, Urdu, Hinglish, Marathi, Tamil, Telugu, and Bengali at zero cost', () => {
  assert.equal(detectMessageLanguage('Sent Rs.450.50 via UPI to Swiggy.').code, 'en');

  const hi = translateSmsToEnglish(
    'आपका OTP 482910 है। किसी के साथ साझा न करें। धन्यवाद।'
  );
  assert.equal(hi.language.code, 'hi');
  assert.equal(hi.needsTranslation, true);
  assert.ok(hi.translatedText.includes('Your OTP'));
  assert.ok(hi.translatedText.includes('482910'));
  assert.ok(hi.translatedText.includes('Do not share with anyone'));

  const ur = translateSmsToEnglish(
    'السلام علیکم، میں گھر پہنچ گیا ہوں۔ کل شام 7 بجے ملتے ہیں۔ شکریہ۔'
  );
  assert.equal(ur.language.code, 'ur');
  assert.equal(ur.language.isRtl, true);
  assert.ok(ur.translatedText.includes('reached home'));
  assert.ok(ur.translatedText.includes('Thank you'));

  const hinglish = translateSmsToEnglish(
    'Bhai kal shaam 8 baje milte hain khana khane, paise bhej diye hain.'
  );
  assert.equal(hinglish.language.code, 'hinglish');
  assert.ok(hinglish.translatedText.toLowerCase().includes('tomorrow evening'));
  assert.ok(hinglish.translatedText.toLowerCase().includes('sent the money'));

  const mr = translateSmsToEnglish(
    'तुमचे पार्सल आज संध्याकाळी 6 पर्यंत पोहोचेल. धन्यवाद.'
  );
  assert.equal(mr.language.code, 'mr');
  assert.ok(mr.translatedText.includes('Your parcel'));

  const ta = translateSmsToEnglish(
    'உங்கள் ஆர்டர் இன்று மாலை டெலிவரி செய்யப்படும். நன்றி.'
  );
  assert.equal(ta.language.code, 'ta');
  assert.ok(ta.translatedText.includes('Your order'));
  assert.ok(ta.translatedText.includes('will be delivered'));

  const te = translateSmsToEnglish(
    'మీ ఆర్డర్ ఈరోజు సాయంత్రం డెలివరీ అవుతుంది. ధన్యవాదాలు.'
  );
  assert.equal(te.language.code, 'te');
  assert.ok(te.translatedText.includes('Your order'));

  const bn = translateSmsToEnglish(
    'আপনার অর্ডার আজ সন্ধ্যায় ডেলিভারি করা হবে। ধন্যবাদ।'
  );
  assert.equal(bn.language.code, 'bn');
  assert.ok(bn.translatedText.includes('Your order'));
});

test('PBKDF2 + AES-256-GCM encrypted backup round-trips cleanly and rejects wrong passphrase', async () => {
  const now = Date.now();
  const seed = createSeedMessages(now);
  const { envelope, serialized } = await createEncryptedBackup(
    {
      schemaVersion: 1,
      app: 'mySMS',
      createdAtMs: now,
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
    'my-secret-pass-2026'
  );

  assert.equal(envelope.cipher, 'AES-256-GCM');
  assert.equal(envelope.provider, 'onedrive');
  assert.equal(envelope.messageCount, seed.length);
  assert.ok(!serialized.includes('SWIGGY INSTAMART'));

  const restored = await restoreEncryptedBackup(serialized, 'my-secret-pass-2026');
  assert.equal(restored.messages.length, seed.length);
  assert.equal(restored.cycleConfig['Credit Card'], 12);
  assert.equal(restored.budgetLimits['Food & Dining'], 6500);
  assert.equal(restored.autoTranslateToEnglish, true);

  await assert.rejects(
    () => restoreEncryptedBackup(serialized, 'wrong-passphrase'),
    /Incorrect passphrase/
  );
});
