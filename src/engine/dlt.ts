import type { DltSuffix, SenderKind } from './types';

const DLT_WITH_SUFFIX = /^([A-Z]{2})[- ]?([A-Z0-9]{3,9})-([STPG])$/i;
const DLT_LEGACY = /^([A-Z]{2})-([A-Z0-9]{3,9})$/i;
const PHONE_RE = /^\+?[0-9]{10,15}$/;
const SHORTCODE_RE = /^[0-9]{4,8}$/;

export function parseSender(sender: string, isKnownContact: boolean): SenderKind {
  if (isKnownContact) {
    return { type: 'Contact' };
  }

  const cleaned = sender.trim();

  const suffixMatch = DLT_WITH_SUFFIX.exec(cleaned);
  if (suffixMatch) {
    const rawSuffix = suffixMatch[3].toUpperCase();
    let suffix: DltSuffix | null = null;
    if (rawSuffix === 'S') suffix = 'Service';
    else if (rawSuffix === 'T') suffix = 'Transactional';
    else if (rawSuffix === 'P') suffix = 'Promotional';
    else if (rawSuffix === 'G') suffix = 'Government';

    return {
      type: 'DltHeader',
      prefix: suffixMatch[1].toUpperCase(),
      entity: suffixMatch[2].toUpperCase(),
      suffix,
    };
  }

  const legacyMatch = DLT_LEGACY.exec(cleaned);
  if (legacyMatch) {
    return {
      type: 'DltHeader',
      prefix: legacyMatch[1].toUpperCase(),
      entity: legacyMatch[2].toUpperCase(),
      suffix: null,
    };
  }

  const compact = cleaned.replace(/[^0-9+]/g, '');
  if (PHONE_RE.test(compact)) {
    return { type: 'PhoneNumber' };
  }
  if (SHORTCODE_RE.test(compact)) {
    return { type: 'Shortcode' };
  }

  return { type: 'Alphanumeric', value: cleaned };
}

export function formatSenderKindLabel(kind: SenderKind): string {
  switch (kind.type) {
    case 'Contact':
      return 'Saved Contact';
    case 'PhoneNumber':
      return 'Phone Number';
    case 'Shortcode':
      return 'Shortcode';
    case 'DltHeader':
      return kind.suffix
        ? `DLT ${kind.suffix} (${kind.entity})`
        : `DLT Entity (${kind.entity})`;
    case 'Alphanumeric':
      return `Alphanumeric (${kind.value})`;
  }
}
