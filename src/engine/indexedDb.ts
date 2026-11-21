import type { RawSms } from './types';

const DB_NAME = 'mysms-history-db';
const DB_VERSION = 1;
const STORE_NAME = 'messages_snapshot';
const SNAPSHOT_KEY = 'primary';

export const MIN_HISTORY_CAPACITY = 10_000;

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('IndexedDB unavailable'));
      return;
    }
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('Failed to open IndexedDB'));
  });
}

export async function loadMessagesFromIndexedDb(): Promise<RawSms[] | null> {
  try {
    const db = await openDb();
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readonly');
      const store = tx.objectStore(STORE_NAME);
      const req = store.get(SNAPSHOT_KEY);
      req.onsuccess = () => {
        const result = req.result;
        resolve(Array.isArray(result) ? (result as RawSms[]) : null);
      };
      req.onerror = () => reject(req.error);
    });
  } catch {
    return null;
  }
}

export async function saveMessagesToIndexedDb(messages: RawSms[]): Promise<boolean> {
  try {
    const db = await openDb();
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readwrite');
      const store = tx.objectStore(STORE_NAME);
      store.put(messages, SNAPSHOT_KEY);
      tx.oncomplete = () => resolve(true);
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    return false;
  }
}

/**
 * Generates a realistic high-volume SMS archive (10,000+ SMS) so users can verify
 * 10,000+ history capacity, fast search, and Weekly Useless SMS Cleanup at scale.
 */
export function generateBulkHistoryMessages(
  targetTotal: number,
  existingMessages: RawSms[],
  nowMs: number = Date.now()
): RawSms[] {
  const needed = Math.max(0, targetTotal - existingMessages.length);
  if (needed === 0) return existingMessages;

  let nextId = existingMessages.reduce((m, s) => Math.max(m, s.id), 10_000) + 1;
  const dayMs = 24 * 60 * 60 * 1000;
  const generated: RawSms[] = new Array(needed);

  const templates: Array<{
    threadId: number;
    sender: string;
    contactName?: string;
    isKnownContact: boolean;
    simSlot: 0 | 1;
    protocol?: 'SMS' | 'RCS';
    makeBody: (i: number) => string;
  }> = [
    {
      threadId: 201,
      sender: 'JM-MYNTRA-P',
      isKnownContact: false,
      simSlot: 1,
      makeBody: (i) =>
        `Flat ${30 + (i % 40)}% OFF flash sale #${i}! Use code SAVE${10 + (i % 80)} on styles. Shop https://myntra.com/deal`,
    },
    {
      threadId: 202,
      sender: 'AD-SWIGGY-P',
      isKnownContact: false,
      simSlot: 0,
      makeBody: (i) =>
        `Weekend cravings? Flat 50% OFF upto Rs.120 on order #${i}. Offer valid tonight!`,
    },
    {
      threadId: 203,
      sender: 'VM-OTPBOT-T',
      isKnownContact: false,
      simSlot: 0,
      makeBody: (i) =>
        `${100000 + ((i * 7919) % 899999)} is your OTP for secure login verification. Valid for 5 mins. Do not share.`,
    },
    {
      threadId: 204,
      sender: 'VM-BLUEDR-S',
      isKnownContact: false,
      simSlot: 1,
      makeBody: (i) =>
        `Shipment AWB-${400000 + i} is out for delivery today by 6 PM.`,
    },
    {
      threadId: 205,
      sender: '+918800291844',
      isKnownContact: false,
      simSlot: 1,
      makeBody: (i) =>
        `URGENT KYC #${i}: Your account is suspended! Claim lottery winner prize at http://bit.ly/verify-${i}`,
    },
    {
      threadId: 104,
      sender: '+919845012345',
      contactName: 'Aarav Mehta',
      isKnownContact: true,
      simSlot: 0,
      protocol: 'RCS',
      makeBody: (i) => `Archived sprint check-in note #${i} from earlier this quarter.`,
    },
    {
      threadId: 101,
      sender: 'AX-HDFCBK-S',
      isKnownContact: false,
      simSlot: 0,
      makeBody: (i) =>
        `Sent Rs.${120 + (i % 480)}.00 From HDFC Bank A/C *4821 To SWIGGY INSTAMART On 04/09/26 Ref ${631000000000 + i}. Avl Bal Rs.45,100.00`,
    },
  ];

  for (let i = 0; i < needed; i += 1) {
    const tpl = templates[i % templates.length];
    // Spread across 10 days to 180 days ago so weekly cleanup detects old promos/OTPs/deliveries cleanly
    const ageDays = 10 + (i % 170);
    generated[i] = {
      id: nextId++,
      threadId: tpl.threadId,
      sender: tpl.sender,
      contactName: tpl.contactName,
      body: tpl.makeBody(i),
      timestampMs: nowMs - ageDays * dayMs - (i % 3600) * 1000,
      isKnownContact: tpl.isKnownContact,
      isOutgoing: false,
      simSlot: tpl.simSlot,
      isRead: true,
      isStarred: false,
      protocol: tpl.protocol ?? 'SMS',
    };
  }

  return [...existingMessages, ...generated];
}
