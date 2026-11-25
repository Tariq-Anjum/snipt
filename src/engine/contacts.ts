/**
 * On-device Contacts directory & Indian phone-number normalizer.
 * On Android, `READ_CONTACTS` populates this map via `ContactsContract.PhoneLookup`
 * (see docs/PENDING-LOCAL-UPDATES.md). In browser / local preview, users can grant
 * contact access and add/edit saved contacts directly.
 */

export interface SavedContact {
  id: string;
  name: string;
  phone: string;
  normalizedPhone: string;
  label?: string;
}

const CONTACTS_STORAGE_KEY = 'snipt-contacts-v1';
const CONTACTS_PERM_KEY = 'snipt-contacts-permission-v1';

/** Normalize Indian & international phone numbers to last 10 digits for reliable matching. */
export function normalizePhone(raw: string): string {
  const digits = raw.replace(/\D/g, '');
  if (digits.length === 12 && digits.startsWith('91')) {
    return digits.slice(2);
  }
  if (digits.length === 11 && digits.startsWith('0')) {
    return digits.slice(1);
  }
  if (digits.length > 10) {
    return digits.slice(-10);
  }
  return digits;
}

export function looksLikePhoneNumber(sender: string): boolean {
  const trimmed = sender.trim();
  if (!trimmed) return false;
  if (/^[A-Za-z]{2}-[A-Za-z0-9]+/.test(trimmed)) return false;
  const digits = trimmed.replace(/\D/g, '');
  return digits.length >= 10 && digits.length <= 15 && /^[+\d\s()-]+$/.test(trimmed);
}

export const DEFAULT_SAVED_CONTACTS: SavedContact[] = [
  { id: 'c1', name: 'Aarav Mehta', phone: '+91 98450 12345', normalizedPhone: '9845012345', label: 'Mobile' },
  { id: 'c2', name: 'Zaid Khan', phone: '+91 98112 23344', normalizedPhone: '9811223344', label: 'Mobile' },
  { id: 'c3', name: 'Rohan Verma', phone: '+91 98765 01122', normalizedPhone: '9876501122', label: 'Mobile' },
  { id: 'c4', name: 'Priya Nair', phone: '+91 99001 44556', normalizedPhone: '9900144556', label: 'Work' },
  { id: 'c5', name: 'Munish Sharma', phone: '+91 98201 77889', normalizedPhone: '9820177889', label: 'Mobile' },
  { id: 'c6', name: 'Dr. Kapoor Clinic', phone: '+91 98109 33441', normalizedPhone: '9810933441', label: 'Clinic' },
  { id: 'c7', name: 'Neha Deshmukh', phone: '+91 97654 22110', normalizedPhone: '9765422110', label: 'Family' },
];

export function loadContacts(): SavedContact[] {
  try {
    const raw = window.localStorage.getItem(CONTACTS_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as SavedContact[];
      if (Array.isArray(parsed) && parsed.length > 0) return parsed;
    }
  } catch {
    // ignore
  }
  return DEFAULT_SAVED_CONTACTS;
}

export function saveContacts(contacts: SavedContact[]): void {
  try {
    window.localStorage.setItem(CONTACTS_STORAGE_KEY, JSON.stringify(contacts));
  } catch {
    // ignore
  }
}

export function loadContactsPermission(): boolean {
  try {
    const raw = window.localStorage.getItem(CONTACTS_PERM_KEY);
    if (raw === 'true') return true;
    if (raw === 'false') return false;
  } catch {
    // ignore
  }
  return true;
}

export function saveContactsPermission(granted: boolean): void {
  try {
    window.localStorage.setItem(CONTACTS_PERM_KEY, String(granted));
  } catch {
    // ignore
  }
}

export function resolveContactName(
  sender: string,
  contacts: SavedContact[],
  permissionGranted: boolean
): string | null {
  if (!permissionGranted || !looksLikePhoneNumber(sender)) return null;
  const norm = normalizePhone(sender);
  if (norm.length < 10) return null;
  const hit = contacts.find((c) => c.normalizedPhone === norm);
  return hit ? hit.name : null;
}

export function searchContacts(query: string, contacts: SavedContact[]): SavedContact[] {
  const q = query.trim().toLowerCase();
  if (!q) return contacts;
  const qDigits = q.replace(/\D/g, '');
  return contacts.filter(
    (c) =>
      c.name.toLowerCase().includes(q) ||
      c.phone.toLowerCase().includes(q) ||
      (qDigits.length >= 2 && c.normalizedPhone.includes(qDigits)) ||
      (c.label && c.label.toLowerCase().includes(q))
  );
}

export function upsertContact(
  contacts: SavedContact[],
  input: { name: string; phone: string; label?: string }
): SavedContact[] {
  const name = input.name.trim();
  const phone = input.phone.trim();
  const normalizedPhone = normalizePhone(phone);
  if (!name || normalizedPhone.length < 7) return contacts;

  const existing = contacts.find((c) => c.normalizedPhone === normalizedPhone);
  if (existing) {
    return contacts.map((c) =>
      c.normalizedPhone === normalizedPhone
        ? { ...c, name, phone, label: input.label ?? c.label }
        : c
    );
  }
  return [
    {
      id: `c-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      name,
      phone,
      normalizedPhone,
      label: input.label || 'Mobile',
    },
    ...contacts,
  ];
}
