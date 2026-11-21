import type { ExtractedOtp, SenderKind } from './types';

const OTP_AFTER_KW =
  /\b(?:otp|one[- ]?time[- ]?(?:password|passcode|pin|code)|verification[- ]?code|security[- ]?code|auth(?:entication)?[- ]?code|login[- ]?code|passcode|mpin)\b[^0-9]{0,24}\b([0-9]{4,8})\b/i;

const OTP_BEFORE_KW =
  /\b([0-9]{4,8})\b\s+(?:is|as)\s+(?:your|the)\s+(?:[a-z0-9]+\s+){0,4}(?:otp|one[- ]?time|verification|security|login|secret|passcode|code)\b/i;

const OTP_PREFIXED = /\bG-([0-9]{6})\b/i;

const FALSE_POSITIVE_PREFIX = /(?:rs\.?|inr|₹|a\/c|acct|xx+|ending)\s*$/i;

export function extractOtp(body: string, senderKind: SenderKind): ExtractedOtp | null {
  const regexes = [OTP_PREFIXED, OTP_BEFORE_KW, OTP_AFTER_KW];

  for (const re of regexes) {
    const match = re.exec(body);
    if (!match || !match[1]) continue;

    const code = match[1];
    const matchIndex = match.index + match[0].indexOf(code);
    const prefixStart = Math.max(0, matchIndex - 12);
    const prefix = body.slice(prefixStart, matchIndex);

    if (FALSE_POSITIVE_PREFIX.test(prefix)) {
      continue;
    }

    if (code.length === 4) {
      const num = Number.parseInt(code, 10);
      if (num >= 2020 && num <= 2035 && !body.toLowerCase().includes('otp')) {
        continue;
      }
    }

    let serviceHint: string | null = null;
    if (senderKind.type === 'DltHeader') {
      serviceHint = senderKind.entity;
    } else if (senderKind.type === 'Alphanumeric') {
      serviceHint = senderKind.value;
    }

    return {
      code,
      serviceHint,
      ttlSeconds: 86_400,
    };
  }

  return null;
}
