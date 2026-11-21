import { CATEGORY_COUNT, type Category, type RawSms, type SenderKind } from './types';

export const DEFAULT_DIM = 8192;
const STRUCT_SLOTS = 16;

/**
 * Deterministic FNV-1a 32-bit hash matching crates/sms-engine/src/ml.rs
 * and ml/train.py byte-for-byte.
 */
export function fnv1a32(input: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i += 1) {
    const code = input.charCodeAt(i) & 0xff;
    hash ^= code;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

export function extractFeatures(
  sms: RawSms,
  senderKind: SenderKind,
  dim: number = DEFAULT_DIM
): Array<[number, number]> {
  const feats: Array<[number, number]> = [];
  const body = sms.body;
  const lower = body.toLowerCase();
  const len = Math.max(body.length, 1);

  const hasUrl =
    lower.includes('http://') ||
    lower.includes('https://') ||
    lower.includes('bit.ly') ||
    lower.includes('.in/');
  const hasCurrency = lower.includes('rs.') || lower.includes('inr') || body.includes('₹');
  const hasOtpKw =
    lower.includes('otp') ||
    lower.includes('verification code') ||
    lower.includes('passcode');
  const hasPromoKw = [
    'off',
    'sale',
    'offer',
    'discount',
    'cashback',
    'free',
    'click',
    'unsubscribe',
    'flat ',
    'apply now',
  ].some((k) => lower.includes(k));
  const hasTxnKw = [
    'debited',
    'credited',
    'upi',
    'a/c',
    'acct',
    'balance',
    'imps',
    'neft',
    'spent',
  ].some((k) => lower.includes(k));
  const hasRemKw = [
    'pnr',
    'due date',
    'pay by',
    'flight',
    'train',
    'out for delivery',
  ].some((k) => lower.includes(k));

  let digits = 0;
  let uppers = 0;
  let alphas = 0;
  for (let i = 0; i < body.length; i += 1) {
    const c = body.charCodeAt(i);
    if (c >= 48 && c <= 57) digits += 1;
    else if (c >= 65 && c <= 90) {
      uppers += 1;
      alphas += 1;
    } else if (c >= 97 && c <= 122) {
      alphas += 1;
    }
  }
  alphas = Math.max(alphas, 1);

  const dt = new Date(sms.timestampMs);
  const hourFrac = Number.isFinite(dt.getTime()) ? dt.getUTCHours() / 24 : 0.5;
  const hourSin = Math.sin(Math.PI * 2 * hourFrac);
  const hourCos = Math.cos(Math.PI * 2 * hourFrac);

  let isAlphaSender = 0;
  let isPhone = 0;
  let dltSvcTxn = 0;
  let dltPromo = 0;

  if (senderKind.type === 'DltHeader') {
    isAlphaSender = 1;
    if (
      senderKind.suffix === 'Service' ||
      senderKind.suffix === 'Transactional' ||
      senderKind.suffix === 'Government'
    ) {
      dltSvcTxn = 1;
    } else if (senderKind.suffix === 'Promotional') {
      dltPromo = 1;
    }
  } else if (senderKind.type === 'Alphanumeric') {
    isAlphaSender = 1;
  } else if (senderKind.type === 'PhoneNumber' || senderKind.type === 'Contact') {
    isPhone = 1;
  }

  const dense: number[] = [
    hasUrl ? 1 : 0,
    hasCurrency ? 1 : 0,
    hasOtpKw ? 1 : 0,
    digits / len,
    uppers / alphas,
    Math.min(Math.log(len) / 6.0, 1.5),
    hasPromoKw ? 1 : 0,
    hasTxnKw ? 1 : 0,
    hasRemKw ? 1 : 0,
    isAlphaSender,
    isPhone,
    hourSin,
    hourCos,
    dltSvcTxn,
    dltPromo,
    body.includes('!') ? 1 : 0,
  ];

  for (let i = 0; i < STRUCT_SLOTS; i += 1) {
    if (dense[i] !== 0) {
      feats.push([i, dense[i]]);
    }
  }

  const hashSpace = dim - STRUCT_SLOTS;
  const tokens = lower
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length >= 2 && t.length <= 24)
    .slice(0, 64);

  const norm = Math.sqrt(Math.max(tokens.length, 1));
  for (let idx = 0; idx < tokens.length; idx += 1) {
    const tok = tokens[idx];
    const h = fnv1a32(tok);
    const slot = STRUCT_SLOTS + (h % hashSpace);
    const sign = (h >>> 31) === 0 ? 1.0 : -1.0;
    feats.push([slot, sign / norm]);

    if (idx + 1 < tokens.length) {
      const bg = `${tok}_${tokens[idx + 1]}`;
      const bh = fnv1a32(bg);
      const bslot = STRUCT_SLOTS + (bh % hashSpace);
      const bsign = (bh >>> 31) === 0 ? 1.0 : -1.0;
      feats.push([bslot, (0.7 * bsign) / norm]);
    }
  }

  return feats;
}

function softmax(logits: [number, number, number, number, number]): [number, number, number, number, number] {
  const maxL = Math.max(...logits);
  const exps = logits.map((l) => Math.exp(l - maxL));
  const sum = Math.max(
    exps.reduce((a, b) => a + b, 0),
    1e-9
  );
  return [
    exps[0] / sum,
    exps[1] / sum,
    exps[2] / sum,
    exps[3] / sum,
    exps[4] / sum,
  ];
}

const CAT_INDEX: Record<Category, number> = {
  Personal: 0,
  Transactions: 1,
  Promotions: 2,
  Priority: 3,
  Spam: 4,
};

export class LinearClassifier {
  private readonly dim: number;
  private readonly biases: [number, number, number, number, number];
  private readonly weights: Float32Array;

  constructor(
    dim: number,
    biases: [number, number, number, number, number],
    weights: Float32Array
  ) {
    this.dim = dim;
    this.biases = biases;
    this.weights = weights;
  }

  static bootstrapHeuristic(): LinearClassifier {
    const dim = DEFAULT_DIM;
    const weights = new Float32Array(CATEGORY_COUNT * dim);
    const biases: [number, number, number, number, number] = [0.2, -0.3, -0.2, -0.4, -0.6];

    const set = (cat: Category, slot: number, val: number) => {
      weights[CAT_INDEX[cat] * dim + slot] = val;
    };

    const setWord = (cat: Category, token: string, val: number) => {
      const hashSpace = dim - STRUCT_SLOTS;
      const h = fnv1a32(token);
      const slot = STRUCT_SLOTS + (h % hashSpace);
      const sign = (h >>> 31) === 0 ? 1.0 : -1.0;
      weights[CAT_INDEX[cat] * dim + slot] += val * sign;
    };

    // Personal
    set('Personal', 10, 2.2);
    set('Personal', 9, -2.0);
    set('Personal', 0, -1.2);
    set('Personal', 6, -1.5);

    // Transactions
    set('Transactions', 1, 1.8);
    set('Transactions', 7, 2.5);
    set('Transactions', 13, 1.2);

    // Promotions
    set('Promotions', 6, 2.4);
    set('Promotions', 0, 1.4);
    set('Promotions', 14, 3.0);
    set('Promotions', 15, 0.6);

    // Priority
    set('Priority', 2, 3.0);
    set('Priority', 8, 2.2);
    set('Priority', 13, 1.0);

    // Spam (unknown numbers with suspicious phishing / lottery / kyc links)
    set('Spam', 0, 1.5);
    set('Spam', 6, 1.2);
    set('Spam', 10, 1.3);
    for (const kw of ['kyc', 'lottery', 'winner', 'prize', 'suspended', 'blocked', 'claim', 'urgent', 'loan']) {
      setWord('Spam', kw, 2.8);
    }

    return new LinearClassifier(dim, biases, weights);
  }

  predictProba(sms: RawSms, senderKind: SenderKind): [number, number, number, number, number] {
    const feats = extractFeatures(sms, senderKind, this.dim);
    const logits: [number, number, number, number, number] = [...this.biases];

    for (let c = 0; c < CATEGORY_COUNT; c += 1) {
      const base = c * this.dim;
      let sum = logits[c];
      for (let i = 0; i < feats.length; i += 1) {
        const [idx, val] = feats[i];
        sum += this.weights[base + idx] * val;
      }
      logits[c] = sum;
    }

    return softmax(logits);
  }
}
