import type { ExtractedReminder } from './types';

const PNR_RE = /\bpnr\s*(?:no\.?|number|#|:|-)?\s*([A-Z0-9]{6,10})\b/i;
const DUE_DATE_RE =
  /\b(?:due\s*(?:date|by|on)|pay\s*(?:by|before)|last\s*date)\s*(?:is|:)?\s*([0-9]{1,2}[-/ ][A-Za-z0-9]{2,9}(?:[-/ ][0-9]{2,4})?)/i;
const DELIVERY_RE =
  /\b(?:out\s+for\s+delivery|arriving\s+today|will\s+be\s+delivered\s+today)\b/i;

export function extractReminder(body: string): ExtractedReminder | null {
  const pnrMatch = PNR_RE.exec(body);
  if (pnrMatch) {
    const pnr = pnrMatch[1].toUpperCase();
    return {
      kind: 'TravelPnr',
      reference: pnr,
      dateText: null,
      summary: `Travel Booking (PNR: ${pnr})`,
    };
  }

  const dueMatch = DUE_DATE_RE.exec(body);
  if (dueMatch) {
    const dateText = dueMatch[1].trim();
    return {
      kind: 'BillDue',
      reference: null,
      dateText,
      summary: `Bill payment due on ${dateText}`,
    };
  }

  if (DELIVERY_RE.test(body)) {
    return {
      kind: 'Delivery',
      reference: null,
      dateText: 'Today',
      summary: 'Package out for delivery today',
    };
  }

  return null;
}
