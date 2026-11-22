import type { ExtractedTransaction, PaymentMethod, TxnDirection } from './types';

const ACCOUNT_RE =
  /(?:a\/c|ac|acct|account|card)\s*(?:no\.?\s*)?(?:ending\s*(?:with\s*)?|xx+|x+|\*+)?\s*([0-9]{3,4})\b/i;
const BALANCE_RE =
  /(?:avl\.?\s*bal|avail(?:able)?\s*bal(?:ance)?|bal(?:ance)?|avl\.?\s*lmt|avl\.?\s*limit|avail(?:able)?\s*limit)\s*(?:is|:)?\s*(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/i;
const REF_RE =
  /(?:upi\s*(?:ref(?:\s*no)?|id|txn)?|ref(?:\s*no)?|utr)\s*[:#-]?[\s]*([0-9]{10,16}|[a-zA-Z0-9._-]+@[a-zA-Z]{2,})/i;
const DEBIT_RE =
  /\b(?:debited|spent|paid|sent|withdrawn|deducted|purchase|dr|charged)\b/i;
const CREDIT_RE =
  /\b(?:credited|received|deposited|refund(?:ed)?|cashback\s+credited|cr)\b/i;
const PROMO_BLOCKER_RE =
  /\b(?:pre-approved|eligible\s+for|get\s+loan|apply\s+now|win\s+upto|flat\s+[0-9]+%\s+off|offer\s+valid)\b/i;

// Phone round 1 (real-device review): notices that mention an amount but are
// not a completed transaction, and amounts that belong to a balance/limit.
const DUE_NOTICE_RE =
  /\b(?:is\s+due|are\s+due|due\s+(?:on|by|date)|amount\s+due|min(?:imum)?\.?\s+(?:amt\.?|amount)\s+due|ignore\s+if\s+(?:already\s+)?paid|bill\s+(?:is\s+)?generated)\b/i;
const COMPLETED_RE = /\b(?:debited|credited|spent|withdrawn|deducted|charged)\b/i;
const FUTURE_DEBIT_RE = /\bwill\s+be\s+(?:auto-?\s*)?(?:debited|deducted|charged)\b/i;
const BILLER_RECEIPT_RE =
  /\b(?:payment\b[^\n]{0,100}?\b(?:has\s+been\s+|is\s+)?received|received\s+(?:your\s+)?payment)\b/i;
const CREDITED_RE = /\bcredited\b/i;
// Internal fund transfer between the user's own accounts (e.g. a card bill
// paid from savings, "InfoBIL*INFT*"): not spending, and the card spends it
// settles are already counted.
const OWN_TRANSFER_RE = /\b(?:INFT|self\s+transfer|own\s+account\s+transfer)\b/i;
const AMOUNT_ALL_RE = /(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/gi;
const BALANCE_ALL_RE =
  /(?:avl\.?\s*bal|avail(?:able)?\s*bal(?:ance)?|bal(?:ance)?|avl\.?\s*lmt|avl\.?\s*limit|avail(?:able)?\s*limit)\s*(?:is|:)?\s*(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/gi;
const FOREIGN_AMOUNT_RE = /\b(USD|EUR|GBP|AED|SGD)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/i;
const MERCHANT_ALL_RE =
  /\b(?:at|to|from|towards|paid\s+to|trf\s+to|vpa)\s+([A-Za-z0-9][A-Za-z0-9 &._@-]{1,32}?)(?:\s+(?:on|via|ref|upi|avl|bal|using|for|at|if)\b|[.,;]|$)/gi;
const UPI_PAYEE_RE = /\bUPI[-:/][0-9]{6,}[-/]([A-Za-z][A-Za-z0-9 &._-]{1,32}?)(?:\.(?:\s|$)|\s+to\s+dispute\b|$)/i;
const NAME_CREDITED_RE = /;\s*([A-Za-z][A-Za-z0-9 &._-]{1,40}?)\s+credited\b/i;
const PHONE_LIKE_RE = /^\+?[0-9][0-9 -]{6,}$/;
const EXPLICIT_CREDIT_CARD_RE = /\bcredit\s*card\b/i;

const CREDIT_CARD_RE =
  /\b(?:credit\s*card|cc\s*(?:xx|ending|\d{4})|regalia|millennia|infinia|coral\s*card|amex|sbi\s*card|icici\s*bank\s*card|axis\s*bank\s*card|card\s*(?:xx+|\*+|ending)\s*[0-9]{3,4}|avl\.?\s*lmt|available\s*limit)\b/i;

function parseInr(raw: string): number | null {
  const num = Number.parseFloat(raw.replace(/,/g, ''));
  return Number.isFinite(num) ? num : null;
}

export function detectPaymentMethod(
  body: string,
  refId: string | null
): PaymentMethod {
  // A credit card charged through UPI (RuPay CC on UPI) is a card spend: it
  // belongs to the card's billing cycle.
  if (EXPLICIT_CREDIT_CARD_RE.test(body)) {
    return 'Credit Card';
  }
  const lower = body.toLowerCase();
  const isUpi =
    lower.includes('upi') ||
    lower.includes('vpa') ||
    lower.includes('gpay') ||
    lower.includes('phonepe') ||
    lower.includes('paytm') ||
    body.includes('@') ||
    (/\bsent\s+(?:rs\.?|inr|₹)/i.test(body) && Boolean(refId && /^\d{12}$/.test(refId)));

  if (isUpi) {
    return 'UPI';
  }

  if (CREDIT_CARD_RE.test(body)) {
    return 'Credit Card';
  }

  return 'Savings Account';
}

/** First INR amount that is not part of a balance / available-limit phrase. */
function pickAmount(body: string): { amount: number; currency: string } | null {
  const balanceSpans: Array<[number, number]> = [];
  for (const m of body.matchAll(BALANCE_ALL_RE)) {
    balanceSpans.push([m.index ?? 0, (m.index ?? 0) + m[0].length]);
  }
  let inr: { amount: number; index: number } | null = null;
  for (const m of body.matchAll(AMOUNT_ALL_RE)) {
    const i = m.index ?? 0;
    if (balanceSpans.some(([s, e]) => i >= s && i < e)) continue;
    const n = parseInr(m[1]);
    if (n !== null && n > 0) {
      inr = { amount: n, index: i };
      break;
    }
  }
  const foreign = FOREIGN_AMOUNT_RE.exec(body);
  if (foreign && (inr === null || foreign.index < inr.index)) {
    const n = parseInr(foreign[2]);
    if (n !== null && n > 0) return { amount: n, currency: foreign[1].toUpperCase() };
  }
  return inr ? { amount: inr.amount, currency: 'INR' } : null;
}

function pickMerchant(body: string): string | null {
  const payee = UPI_PAYEE_RE.exec(body) ?? NAME_CREDITED_RE.exec(body);
  if (payee) {
    const p = payee[1].trim();
    if (p.length >= 2) return p;
  }
  for (const m of body.matchAll(MERCHANT_ALL_RE)) {
    const c = m[1].trim();
    if (c.length < 2 || PHONE_LIKE_RE.test(c)) continue;
    return c;
  }
  return null;
}

export function extractTransaction(body: string): ExtractedTransaction | null {
  if (PROMO_BLOCKER_RE.test(body)) {
    return null;
  }
  if (FUTURE_DEBIT_RE.test(body)) {
    return null;
  }
  if (DUE_NOTICE_RE.test(body) && !COMPLETED_RE.test(body)) {
    return null;
  }
  if (BILLER_RECEIPT_RE.test(body) && !CREDITED_RE.test(body)) {
    return null;
  }
  if (OWN_TRANSFER_RE.test(body)) {
    return null;
  }

  const debitMatch = DEBIT_RE.exec(body);
  const creditMatch = CREDIT_RE.exec(body);
  if (!debitMatch && !creditMatch) {
    return null;
  }

  const picked = pickAmount(body);
  if (!picked) return null;
  const { amount, currency } = picked;

  let direction: TxnDirection = 'Unknown';
  if (debitMatch && !creditMatch) {
    direction = 'Debit';
  } else if (!debitMatch && creditMatch) {
    direction = 'Credit';
  } else if (debitMatch && creditMatch) {
    direction = debitMatch.index <= creditMatch.index ? 'Debit' : 'Credit';
  }

  const accountMatch = ACCOUNT_RE.exec(body);
  const balanceMatch = BALANCE_RE.exec(body);
  const refMatch = REF_RE.exec(body);
  const merchantOrVpa = pickMerchant(body);

  const referenceId = refMatch ? refMatch[1] : null;
  const paymentMethod = detectPaymentMethod(body, referenceId);

  return {
    amount,
    currency,
    direction,
    paymentMethod,
    accountLast4: accountMatch ? accountMatch[1] : null,
    merchantOrVpa,
    referenceId,
    availableBalance: balanceMatch ? parseInr(balanceMatch[1]) : null,
    isUpi: paymentMethod === 'UPI',
  };
}
