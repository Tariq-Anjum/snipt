import { X } from 'lucide-react';
import { useMemo, useState, type FormEvent } from 'react';
import {
  defaultEngine,
  formatSenderKindLabel,
  translateSmsToEnglish,
  type MessageProtocol,
} from '../engine';
import { SIMULATION_PRESETS } from '../engine/seedData';
import { formatInr } from '../utils/format';

interface SimulateSmsModalProps {
  isOpen: boolean;
  onClose: () => void;
  onDeliver: (payload: {
    sender: string;
    contactName?: string;
    body: string;
    isKnownContact: boolean;
    simSlot: 0 | 1;
    protocol: MessageProtocol;
  }) => void;
}

export function SimulateSmsModal({
  isOpen,
  onClose,
  onDeliver,
}: SimulateSmsModalProps) {
  const [sender, setSender] = useState(SIMULATION_PRESETS[0].sender);
  const [contactName, setContactName] = useState('');
  const [body, setBody] = useState(SIMULATION_PRESETS[0].body);
  const [isKnownContact, setIsKnownContact] = useState(false);
  const [simSlot, setSimSlot] = useState<0 | 1>(0);
  const [protocol, setProtocol] = useState<MessageProtocol>(SIMULATION_PRESETS[0].protocol);
  const [error, setError] = useState<string | null>(null);

  const livePreview = useMemo(() => {
    if (!sender.trim() || !body.trim()) return null;
    const analyzed = defaultEngine.analyzeMessage({
      id: 0,
      threadId: 0,
      sender: sender.trim(),
      body: body.trim(),
      timestampMs: Date.now(),
      isKnownContact,
      isOutgoing: false,
      simSlot,
      protocol,
    });
    const translation = translateSmsToEnglish(body.trim());
    return { analyzed, translation };
  }, [sender, body, isKnownContact, simSlot, protocol]);

  if (!isOpen) return null;

  const applyPreset = (presetId: string) => {
    const found = SIMULATION_PRESETS.find((p) => p.id === presetId);
    if (!found) return;
    setSender(found.sender);
    setContactName(found.contactName ?? '');
    setBody(found.body);
    setIsKnownContact(found.isKnownContact);
    setSimSlot(found.simSlot);
    setProtocol(found.protocol);
    setError(null);
  };

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!sender.trim()) {
      setError('Enter a sender ID or phone number.');
      return;
    }
    if (!body.trim()) {
      setError('Enter the message body to classify.');
      return;
    }

    onDeliver({
      sender: sender.trim(),
      contactName: isKnownContact ? contactName.trim() || undefined : undefined,
      body: body.trim(),
      isKnownContact,
      simSlot,
      protocol,
    });
    setError(null);
    onClose();
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="simulate-sms-title"
      className="fixed inset-0 z-50 bg-slate-950/50 backdrop-blur-xs flex items-end md:items-center justify-center p-0 md:p-4"
    >
      <div className="w-full md:max-w-xl bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-t-2xl md:rounded-2xl shadow-xl max-h-[92vh] overflow-y-auto">
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-200 dark:border-slate-800">
          <div>
            <h2
              id="simulate-sms-title"
              className="font-display text-base font-semibold text-slate-900 dark:text-slate-100"
            >
              Simulate Incoming SMS / RCS
            </h2>
            <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
              Test Indian UPI, Credit Card, Savings Account, Encrypted OTP, RCS chat, and notifications live.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close simulate SMS dialog"
            className="min-w-[40px] min-h-[40px] flex items-center justify-center text-slate-500 hover:text-slate-900 dark:hover:text-slate-100 rounded-lg"
          >
            <X className="w-5 h-5" aria-hidden="true" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-6 space-y-5">
          <div>
            <p className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-2">
              Quick Indian SMS &amp; RCS presets
            </p>
            <div className="flex flex-wrap gap-1.5">
              {SIMULATION_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => applyPreset(preset.id)}
                  className={`px-2.5 py-1.5 text-xs font-medium rounded-md border transition-colors whitespace-nowrap ${
                    body === preset.body && sender === preset.sender
                      ? 'bg-blue-50 border-blue-300 text-blue-800 dark:bg-blue-950/60 dark:border-blue-700 dark:text-blue-300'
                      : 'bg-slate-50 border-slate-200 text-slate-700 hover:bg-slate-100 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-700'
                  }`}
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div className="sm:col-span-1">
              <label
                htmlFor="sim-sender"
                className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
              >
                Sender ID / Phone
              </label>
              <input
                id="sim-sender"
                type="text"
                value={sender}
                onChange={(e) => {
                  setSender(e.target.value);
                  setError(null);
                }}
                placeholder="e.g. AX-HDFCBK-S"
                className="w-full px-3 py-2 text-sm font-mono bg-white dark:bg-slate-950 border border-slate-300 dark:border-slate-700 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
              />
            </div>

            <div>
              <span className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5">
                Protocol
              </span>
              <div className="grid grid-cols-2 gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg">
                {(['SMS', 'RCS'] as const).map((proto) => (
                  <button
                    key={proto}
                    type="button"
                    onClick={() => setProtocol(proto)}
                    className={`py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                      protocol === proto
                        ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                        : 'text-slate-600 dark:text-slate-400'
                    }`}
                  >
                    {proto}
                  </button>
                ))}
              </div>
            </div>

            <div>
              <span className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5">
                SIM Slot
              </span>
              <div className="grid grid-cols-2 gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg">
                <button
                  type="button"
                  onClick={() => setSimSlot(0)}
                  className={`py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                    simSlot === 0
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  SIM 1
                </button>
                <button
                  type="button"
                  onClick={() => setSimSlot(1)}
                  className={`py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                    simSlot === 1
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  SIM 2
                </button>
              </div>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-4">
            <label className="inline-flex items-center gap-2 text-xs font-medium text-slate-700 dark:text-slate-300 cursor-pointer">
              <input
                type="checkbox"
                checked={isKnownContact}
                onChange={(e) => setIsKnownContact(e.target.checked)}
                className="w-4 h-4 rounded border-slate-300 text-blue-700 focus:ring-blue-600"
              />
              <span>Saved in phonebook contacts</span>
            </label>

            {isKnownContact && (
              <input
                type="text"
                aria-label="Contact display name"
                value={contactName}
                onChange={(e) => setContactName(e.target.value)}
                placeholder="Contact name (optional)"
                className="flex-1 min-w-[160px] px-3 py-1.5 text-xs bg-white dark:bg-slate-950 border border-slate-300 dark:border-slate-700 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
              />
            )}
          </div>

          <div>
            <label
              htmlFor="sim-body"
              className="block text-xs font-medium text-slate-700 dark:text-slate-300 mb-1.5"
            >
              Message Body
            </label>
            <textarea
              id="sim-body"
              rows={3}
              value={body}
              onChange={(e) => {
                setBody(e.target.value);
                setError(null);
              }}
              placeholder="Paste any UPI alert, Credit Card spend, OTP, RCS chat, or PNR..."
              className="w-full px-3 py-2 text-sm bg-white dark:bg-slate-950 border border-slate-300 dark:border-slate-700 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
            />
          </div>

          {error && (
            <p role="alert" className="text-xs font-medium text-red-600 dark:text-red-400">
              {error}
            </p>
          )}

          {livePreview && (
            <div className="p-3.5 rounded-xl bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 space-y-1.5">
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                <span className="font-medium text-slate-900 dark:text-slate-100">
                  Live Engine Preview: {livePreview.analyzed.classification.category} · {protocol}
                </span>
                <span className="font-mono tabular-nums text-slate-500 dark:text-slate-400">
                  {(livePreview.analyzed.classification.confidence * 100).toFixed(0)}% ·{' '}
                  {livePreview.analyzed.classification.reason}
                </span>
              </div>
              <div className="text-xs text-slate-600 dark:text-slate-400 flex flex-wrap gap-x-3 gap-y-1">
                <span>Sender: {formatSenderKindLabel(livePreview.analyzed.senderKind)}</span>
                <span>Language: {livePreview.translation.language.label}</span>
                {livePreview.analyzed.otp && (
                  <span className="font-mono text-blue-700 dark:text-blue-400">
                    Encrypted OTP: {livePreview.analyzed.otp.code}
                  </span>
                )}
                {livePreview.analyzed.transaction && (
                  <span className="font-mono text-emerald-700 dark:text-emerald-400">
                    {livePreview.analyzed.transaction.paymentMethod} ·{' '}
                    {livePreview.analyzed.transaction.direction}:{' '}
                    {formatInr(livePreview.analyzed.transaction.amount)}
                  </span>
                )}
                {livePreview.analyzed.reminder && (
                  <span className="text-amber-700 dark:text-amber-400">
                    Reminder: {livePreview.analyzed.reminder.summary}
                  </span>
                )}
              </div>
              {livePreview.translation.needsTranslation && (
                <div className="pt-1.5 border-t border-slate-200/80 dark:border-slate-800 text-xs text-blue-800 dark:text-blue-300">
                  <span className="font-semibold">
                    Offline English ({livePreview.translation.language.label}):{' '}
                  </span>
                  <span>{livePreview.translation.translatedText}</span>
                </div>
              )}
            </div>
          )}

          <div className="flex items-center justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-xs font-medium text-slate-700 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors"
            >
              Deliver {protocol}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
