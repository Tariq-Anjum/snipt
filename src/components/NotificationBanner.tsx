import { Check, Copy, Lock, Reply, Send, Trash2, X } from 'lucide-react';
import { AnimatePresence, motion } from 'motion/react';
import { useState, type FormEvent } from 'react';
import type { ParsedSms } from '../engine';
import { formatInr } from '../utils/format';

interface NotificationBannerProps {
  parsed: ParsedSms | null;
  onDismiss: () => void;
  onOpenThread: (threadId: number) => void;
  onCopyOtp: (code: string) => void;
  onMarkRead: (smsId: number) => void;
  onDeleteSms: (smsId: number) => void;
  onQuickReply: (threadId: number, body: string, simSlot: 0 | 1, protocol: 'SMS' | 'RCS') => void;
}

export function NotificationBanner({
  parsed,
  onDismiss,
  onOpenThread,
  onCopyOtp,
  onMarkRead,
  onDeleteSms,
  onQuickReply,
}: NotificationBannerProps) {
  const [copied, setCopied] = useState(false);
  const [replyOpen, setReplyOpen] = useState(false);
  const [replyBody, setReplyBody] = useState('');

  const protocol = parsed?.sms.protocol ?? 'SMS';

  const handleCopy = () => {
    if (!parsed?.otp) return;
    onCopyOtp(parsed.otp.code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1800);
  };

  const handleQuickReplySubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!parsed || !replyBody.trim()) return;
    onQuickReply(parsed.sms.threadId, replyBody.trim(), parsed.sms.simSlot, protocol);
    setReplyBody('');
    setReplyOpen(false);
    onDismiss();
  };

  return (
    <AnimatePresence>
      {parsed && (
        <motion.div
          key={parsed.sms.id}
          initial={{ opacity: 0, y: -14, scale: 0.98 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: -10, scale: 0.98 }}
          transition={{ type: 'spring', stiffness: 460, damping: 34, mass: 0.8 }}
          role="status"
          aria-live="polite"
          className="fixed top-16 right-4 left-4 md:left-auto md:w-[430px] z-50 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-700 rounded-2xl shadow-lg p-4"
        >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
            <span className="font-semibold text-slate-900 dark:text-slate-100 truncate">
              {parsed.sms.contactName || parsed.sms.sender}
            </span>
            <span aria-hidden="true">·</span>
            <span>{parsed.classification.category}</span>
            <span aria-hidden="true">·</span>
            <span>{protocol === 'RCS' ? 'RCS Chat' : `SIM ${parsed.sms.simSlot + 1}`}</span>
          </div>
          <p className="mt-1 text-sm text-slate-700 dark:text-slate-300 line-clamp-2">
            {parsed.sms.body}
          </p>
        </div>

        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss notification"
          className="p-1.5 text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 rounded-md shrink-0"
        >
          <X className="w-4 h-4" aria-hidden="true" />
        </button>
      </div>

      {/* Extracted Smart Metadata Line */}
      {(parsed.otp || parsed.transaction || parsed.reminder) && (
        <div className="mt-2.5 text-xs text-slate-600 dark:text-slate-300 font-mono tabular-nums flex items-center gap-1.5">
          {parsed.otp && (
            <>
              <Lock className="w-3.5 h-3.5 text-blue-700 dark:text-blue-400 shrink-0" aria-hidden="true" />
              <span>Encrypted OTP: {parsed.otp.code}</span>
            </>
          )}
          {!parsed.otp && parsed.transaction && (
            <span>
              {parsed.transaction.paymentMethod} · {parsed.transaction.direction}{' '}
              {formatInr(parsed.transaction.amount)}
            </span>
          )}
          {!parsed.otp && !parsed.transaction && parsed.reminder && (
            <span>{parsed.reminder.summary}</span>
          )}
        </div>
      )}

      {/* Interactive Android-style Notification Actions: Copy OTP / Mark Read / Reply / Delete */}
      <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-1.5">
          {parsed.otp && (
            <button
              type="button"
              onClick={handleCopy}
              className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 rounded-md transition-colors whitespace-nowrap"
            >
              {copied ? (
                <>
                  <Check className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Copied</span>
                </>
              ) : (
                <>
                  <Copy className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Copy OTP</span>
                </>
              )}
            </button>
          )}

          <button
            type="button"
            onClick={() => {
              onMarkRead(parsed.sms.id);
              onDismiss();
            }}
            className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-md transition-colors whitespace-nowrap"
          >
            <Check className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Mark Read</span>
          </button>

          <button
            type="button"
            onClick={() => setReplyOpen((open) => !open)}
            aria-expanded={replyOpen}
            className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-blue-700 dark:text-blue-300 bg-blue-50 hover:bg-blue-100 dark:bg-blue-950/60 dark:hover:bg-blue-950 rounded-md transition-colors whitespace-nowrap"
          >
            <Reply className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Reply</span>
          </button>

          <button
            type="button"
            onClick={() => {
              onDeleteSms(parsed.sms.id);
              onDismiss();
            }}
            className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-red-700 dark:text-red-300 bg-red-50 hover:bg-red-100 dark:bg-red-950/50 dark:hover:bg-red-950 rounded-md transition-colors whitespace-nowrap"
          >
            <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Delete</span>
          </button>
        </div>

        <button
          type="button"
          onClick={() => {
            onOpenThread(parsed.sms.threadId);
            onDismiss();
          }}
          className="text-xs font-medium text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100 whitespace-nowrap"
        >
          Open
        </button>
      </div>

      {/* Inline Quick Reply Box */}
      {replyOpen && (
        <form
          onSubmit={handleQuickReplySubmit}
          className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 flex items-center gap-2"
        >
          <input
            type="text"
            aria-label="Quick reply message"
            value={replyBody}
            onChange={(e) => setReplyBody(e.target.value)}
            placeholder={`Quick reply via ${protocol === 'RCS' ? 'RCS' : `SIM ${parsed.sms.simSlot + 1}`}...`}
            className="flex-1 min-w-0 px-3 py-1.5 text-xs bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-md text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
          />
          <button
            type="submit"
            disabled={!replyBody.trim()}
            aria-label="Send quick reply"
            className="inline-flex items-center gap-1 px-3 py-1.5 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 disabled:opacity-40 rounded-md transition-colors shrink-0"
          >
            <Send className="w-3 h-3" aria-hidden="true" />
            <span>Send</span>
          </button>
        </form>
      )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
