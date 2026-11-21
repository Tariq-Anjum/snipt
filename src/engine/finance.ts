import type { ExtractedTransaction, PaymentMethod, TxnDirection } from './types';

const AMOUNT_RE = /(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/i;
const ACCOUNT_RE =
  /(?:a\/c|ac|acct|account|card)\s*(?:no\.?\s*)?(?:ending\s*(?:with\s*)?|xx+|x+|\*+)?\s*([0-9]{3,4})\b/i;
const BALANCE_RE =
  /(?:avl\.?\s*bal|avail(?:able)?\s*bal(?:ance)?|bal(?:ance)?|avl\.?\s*lmt|avail(?:able)?\s*limit)\s*(?:is|:)?\s*(?:rs\.?|inr|₹)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)/i;
const REF_RE =
  /(?:upi\s*(?:ref(?:\s*no)?|id|txn)?|ref(?:\s*no)?|utr)\s*[:#-]?[\s]*([0-9]{10,16}|[a-zA-Z0-9._-]+@[a-zA-Z]{2,})/i;
const MERCHANT_RE =
  /\b(?:at|to|from|towards|paid\s+to|trf\s+to|vpa)\s+([A-Za-z0-9][A-Za-z0-9 &._@-]{1,32}?)(?:\s+(?:on|via|ref|upi|avl|bal|using|for|at|if)\b|[.,;]|$)/i;
const DEBIT_RE =
  /\b(?:debited|spent|paid|sent|withdrawn|deducted|purchase|dr|charged)\b/i;
const CREDIT_RE =
  /\b(?:credited|received|deposited|refund(?:ed)?|cashback\s+credited|cr)\b/i;
const PROMO_BLOCKER_RE =
  /\b(?:pre-approved|eligible\s+for|get\s+loan|apply\s+now|win\s+upto|flat\s+[0-9]+%\s+off|offer\s+valid)\b/i;

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

export function extractTransaction(body: string): ExtractedTransaction | null {
  if (PROMO_BLOCKER_RE.test(body)) {
    return null;
  }

  const debitMatch = DEBIT_RE.exec(body);
  const creditMatch = CREDIT_RE.exec(body);
  if (!debitMatch && !creditMatch) {
    return null;
  }

  const amountMatch = AMOUNT_RE.exec(body);
  if (!amountMatch) return null;

  const amount = parseInr(amountMatch[1]);
  if (amount === null || amount <= 0) return null;

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
  const merchantMatch = MERCHANT_RE.exec(body);

  const merchantCandidate = merchantMatch ? merchantMatch[1].trim() : null;
  const merchantOrVpa =
    merchantCandidate && merchantCandidate.length >= 2 ? merchantCandidate : null;

  const referenceId = refMatch ? refMatch[1] : null;
  const paymentMethod = detectPaymentMethod(body, referenceId);

  return {
    amount,
    currency: 'INR',
    direction,
    paymentMethod,
    accountLast4: accountMatch ? accountMatch[1] : null,
    merchantOrVpa,
    referenceId,
    availableBalance: balanceMatch ? parseInr(balanceMatch[1]) : null,
    isUpi: paymentMethod === 'UPI',
  };
}
