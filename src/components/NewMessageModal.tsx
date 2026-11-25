import { Search, Send, UserPlus, X } from 'lucide-react';
import { useMemo, useState, type FormEvent } from 'react';
import {
  looksLikePhoneNumber,
  searchContacts,
  type SavedContact,
} from '../engine/contacts';
import { useBackLayer } from '../utils/backStack';

interface NewMessageModalProps {
  isOpen: boolean;
  onClose: () => void;
  contacts: SavedContact[];
  contactsGranted: boolean;
  onGrantContacts: () => void;
  onSaveContact: (input: { name: string; phone: string; label?: string }) => void;
  onSend: (payload: {
    address: string;
    contactName?: string;
    body: string;
    simSlot: 0 | 1;
  }) => Promise<void>;
  simSlots: number;
  companionMode: boolean;
}

export function NewMessageModal({
  isOpen,
  onClose,
  contacts,
  contactsGranted,
  onGrantContacts,
  onSaveContact,
  onSend,
  simSlots,
  companionMode,
}: NewMessageModalProps) {
  const [recipientQuery, setRecipientQuery] = useState('');
  const [selectedContact, setSelectedContact] = useState<SavedContact | null>(null);
  const [newContactName, setNewContactName] = useState('');
  const [saveNewContact, setSaveNewContact] = useState(false);
  const [body, setBody] = useState('');
  const [simSlot, setSimSlot] = useState<0 | 1>(0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useBackLayer(isOpen, onClose);

  const matchingContacts = useMemo(
    () => (contactsGranted ? searchContacts(recipientQuery, contacts) : []),
    [contactsGranted, recipientQuery, contacts]
  );

  if (!isOpen) return null;

  const activeAddress = selectedContact ? selectedContact.phone : recipientQuery.trim();
  const isCustomNumber = !selectedContact && looksLikePhoneNumber(activeAddress);
  const canSubmit = activeAddress.length >= 4 && body.trim().length > 0 && !sending;

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSending(true);
    setError(null);
    try {
      let contactName = selectedContact?.name;
      if (isCustomNumber && saveNewContact && newContactName.trim()) {
        onSaveContact({ name: newContactName.trim(), phone: activeAddress });
        contactName = newContactName.trim();
      }
      await onSend({
        address: activeAddress,
        contactName,
        body: body.trim(),
        simSlot,
      });
      setRecipientQuery('');
      setSelectedContact(null);
      setNewContactName('');
      setSaveNewContact(false);
      setBody('');
      onClose();
    } catch {
      setError('Could not queue this message. Check the recipient number and try again.');
    } finally {
      setSending(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-end md:items-center justify-center p-0 md:p-4"
      role="presentation"
    >
      <button
        type="button"
        aria-label="Close new message dialog"
        onClick={onClose}
        className="absolute inset-0 bg-black/50"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New Message"
        className="sheet-enter relative w-full md:max-w-lg max-h-[90dvh] flex flex-col rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl overflow-hidden"
      >
        <div className="px-5 pt-4 pb-3 border-b border-slate-200/80 dark:border-slate-800 flex items-center justify-between">
          <div>
            <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
              New Message
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              {companionMode
                ? 'Companion Mode · SMS compose or handoff to Google Messages RCS'
                : `Default SMS · Sending via SIM ${simSlot + 1}`}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="min-w-[40px] min-h-[40px] flex items-center justify-center rounded-full text-slate-500 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            <X className="w-5 h-5" aria-hidden="true" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="flex-1 overflow-y-auto p-5 space-y-4">
          {/* Recipient input */}
          <div className="space-y-2">
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400">
              To (Saved contact or new phone number)
            </label>
            {selectedContact ? (
              <div className="flex items-center justify-between gap-3 px-3.5 py-2.5 rounded-2xl border border-blue-200 dark:border-blue-800 bg-blue-50/70 dark:bg-blue-950/40">
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-slate-900 dark:text-slate-100 truncate">
                    {selectedContact.name}
                  </p>
                  <p className="font-mono text-xs text-slate-500 dark:text-slate-400">
                    {selectedContact.phone}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setSelectedContact(null);
                    setRecipientQuery('');
                  }}
                  className="px-2.5 py-1 text-xs font-semibold text-blue-700 dark:text-blue-300 hover:underline"
                >
                  Change
                </button>
              </div>
            ) : (
              <div className="relative">
                <Search
                  className="w-4 h-4 text-slate-400 absolute left-3.5 top-1/2 -translate-y-1/2 pointer-events-none"
                  aria-hidden="true"
                />
                <input
                  type="text"
                  value={recipientQuery}
                  onChange={(e) => setRecipientQuery(e.target.value)}
                  placeholder="Type name or +91 98XXX XXXXX..."
                  aria-label="Recipient name or phone number"
                  className="w-full pl-10 pr-3.5 py-2.5 text-sm rounded-2xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                />
              </div>
            )}
          </div>

          {/* Contacts Permission prompt or saved contact picker */}
          {!selectedContact && (
            <div className="space-y-2">
              {!contactsGranted ? (
                <div className="p-3.5 rounded-2xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/60 flex items-center justify-between gap-3">
                  <div>
                    <p className="text-xs font-semibold text-slate-900 dark:text-slate-100">
                      Contacts permission is off
                    </p>
                    <p className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
                      Allow on-device contact access to pick saved contacts and display names on threads.
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={onGrantContacts}
                    className="px-3 py-1.5 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid rounded-xl whitespace-nowrap shrink-0"
                  >
                    Allow
                  </button>
                </div>
              ) : (
                <div className="max-h-40 overflow-y-auto rounded-2xl border border-slate-200/80 dark:border-slate-800 divide-y divide-slate-100 dark:divide-slate-800/80 bg-white/60 dark:bg-slate-950/40">
                  {matchingContacts.length === 0 ? (
                    <div className="px-3.5 py-3 text-xs text-slate-500 dark:text-slate-400">
                      {isCustomNumber
                        ? `New number detected (${activeAddress}) — ready to send directly.`
                        : 'No matching saved contacts. Enter a 10-digit Indian or international phone number.'}
                    </div>
                  ) : (
                    matchingContacts.slice(0, 6).map((c) => (
                      <button
                        key={c.id}
                        type="button"
                        onClick={() => setSelectedContact(c)}
                        className="w-full px-3.5 py-2.5 flex items-center justify-between gap-3 text-left hover:bg-slate-100/80 dark:hover:bg-slate-800/60 transition-colors"
                      >
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span className="w-8 h-8 rounded-full bg-slate-200/80 dark:bg-slate-800 text-slate-700 dark:text-slate-200 text-xs font-semibold flex items-center justify-center shrink-0">
                            {c.name
                              .split(/\s+/)
                              .slice(0, 2)
                              .map((w) => w[0])
                              .join('')
                              .toUpperCase()}
                          </span>
                          <div className="min-w-0">
                            <p className="text-sm font-medium text-slate-900 dark:text-slate-100 truncate">
                              {c.name}
                            </p>
                            <p className="font-mono text-[11px] text-slate-500 dark:text-slate-400">
                              {c.phone}
                            </p>
                          </div>
                        </div>
                        {c.label && (
                          <span className="text-[11px] text-slate-500 dark:text-slate-400 shrink-0">
                            {c.label}
                          </span>
                        )}
                      </button>
                    ))
                  )}
                </div>
              )}
            </div>
          )}

          {/* Option to save a brand new number */}
          {isCustomNumber && (
            <div className="p-3 rounded-2xl border border-slate-200 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-950/50 space-y-2">
              <label className="flex items-center gap-2 text-xs font-medium text-slate-700 dark:text-slate-300 cursor-pointer">
                <input
                  type="checkbox"
                  checked={saveNewContact}
                  onChange={(e) => setSaveNewContact(e.target.checked)}
                  className="rounded border-slate-300 text-blue-600 focus:ring-blue-600"
                />
                <UserPlus className="w-3.5 h-3.5 text-slate-500" aria-hidden="true" />
                <span>Save this number to Contacts</span>
              </label>
              {saveNewContact && (
                <input
                  type="text"
                  value={newContactName}
                  onChange={(e) => setNewContactName(e.target.value)}
                  placeholder="Contact name (e.g. Kabir Singh)"
                  className="w-full px-3 py-2 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100"
                />
              )}
            </div>
          )}

          {/* SIM selector + Message body */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-xs font-semibold text-slate-600 dark:text-slate-400">
                Message
              </label>
              {simSlots > 1 && (
                <div className="inline-flex items-center gap-1 p-0.5 bg-slate-100 dark:bg-slate-800 rounded-lg">
                  {([0, 1] as const).map((slot) => (
                    <button
                      key={slot}
                      type="button"
                      onClick={() => setSimSlot(slot)}
                      className={`px-2.5 py-1 text-[11px] font-mono font-semibold rounded-md transition-colors ${
                        simSlot === slot
                          ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                          : 'text-slate-500 dark:text-slate-400'
                      }`}
                    >
                      SIM {slot + 1}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <textarea
              rows={3}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder="Write your message..."
              aria-label="Message text"
              className="w-full px-3.5 py-2.5 text-sm rounded-2xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600 resize-none"
            />
          </div>

          {error && (
            <p role="alert" className="text-xs text-red-600 dark:text-red-400">
              {error}
            </p>
          )}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2.5 text-xs font-semibold text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-xl"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!canSubmit}
              className="inline-flex items-center gap-1.5 px-5 py-2.5 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid rounded-xl disabled:opacity-40 transition-colors"
            >
              <Send className="w-3.5 h-3.5" aria-hidden="true" />
              <span>{sending ? 'Sending...' : 'Send Message'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
