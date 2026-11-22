import { useVirtualizer } from '@tanstack/react-virtual';
import {
  Archive,
  ArchiveRestore,
  ArrowLeft,
  Check,
  CheckCheck,
  Copy,
  Eye,
  EyeOff,
  Languages,
  Lock,
  Send,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Trash2,
} from 'lucide-react';
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type FormEvent,
} from 'react';
import {
  archiveThread,
  blockSender,
  deleteMessages,
  markRead,
  queueOutgoing,
  starMessage,
  vaultReveal,
  type Capabilities,
  type MessageView,
  type SendStatus,
  type ThreadRow,
} from '../api';
import { acquireVaultSecure } from '../api/telephony';
import { useThreadMessages, useTranslation } from '../hooks/data';
import { CATEGORY_LIST } from '../engine/types';
import { formatSenderKindLabel } from '../engine/dlt';
import {
  formatCountdown,
  formatFullDateTime,
  formatInr,
} from '../utils/format';

interface ThreadPaneProps {
  thread: ThreadRow;
  version: number;
  nowMs: number;
  autoTranslateToEnglish: boolean;
  completedReminderIds: number[];
  onToggleReminderDone: (smsId: number) => void;
  onCopyOtp: (code: string) => Promise<boolean>;
  onBack: () => void;
  onChanged: () => void;
  onThreadPatched: (patch: Partial<ThreadRow>) => void;
  caps: Capabilities;
}

const SEND_STATUS_LABEL: Record<SendStatus, string> = {
  queued: 'Queued',
  sending: 'Sending...',
  sent: 'Sent',
  unconfirmed: 'Sent · delivery unconfirmed',
  delivered: 'Delivered',
  failed: 'Not sent',
};

interface BubbleProps {
  parsed: MessageView;
  nowMs: number;
  autoTranslate: boolean;
  isDone: boolean;
  onToggleReminderDone: (smsId: number) => void;
  onCopyOtp: (code: string) => Promise<boolean>;
  onStar: (m: MessageView) => void;
  platform: Capabilities['platform'];
  onDelete: (m: MessageView) => void;
}

const Bubble = memo(function Bubble({
  parsed,
  nowMs,
  autoTranslate,
  isDone,
  onToggleReminderDone,
  onCopyOtp,
  onStar,
  onDelete,
  platform,
}: BubbleProps) {
  const { sms, otp, transaction, reminder } = parsed;
  const isOut = sms.isOutgoing;
  const isRcs = sms.protocol === 'RCS';
  const expiresAt = otp ? sms.timestampMs + otp.ttlSeconds * 1000 : 0;
  // The page carries the OTP masked; the code is decrypted from the vault
  // payload only on Reveal (after FLAG_SECURE is confirmed) or Copy.
  const [revealed, setRevealed] = useState<string | null>(null);
  const [otpError, setOtpError] = useState<string | null>(null);
  const releaseSecure = useRef<(() => void) | null>(null);
  const hideTimer = useRef<number | undefined>(undefined);
  const [copied, setCopied] = useState(false);
  const hide = useCallback(() => {
    window.clearTimeout(hideTimer.current);
    setRevealed(null);
    releaseSecure.current?.();
    releaseSecure.current = null;
  }, []);
  useEffect(() => hide, [hide]);
  const reveal = async () => {
    if (revealed !== null) {
      hide();
      return;
    }
    setOtpError(null);
    if (!parsed.otpVault) {
      setOtpError('The code vault is not ready.');
      return;
    }
    const release = await acquireVaultSecure(platform);
    if (!release) {
      setOtpError('Screen protection could not be turned on, so the code stays hidden.');
      return;
    }
    try {
      const code = await vaultReveal(parsed.otpVault);
      releaseSecure.current = release;
      setRevealed(code);
      hideTimer.current = window.setTimeout(hide, 30_000);
    } catch {
      release();
      setOtpError('Could not decrypt this code.');
    }
  };
  const copyCode = async () => {
    setOtpError(null);
    try {
      const code = revealed ?? (parsed.otpVault ? await vaultReveal(parsed.otpVault) : null);
      if (code === null) {
        setOtpError('The code vault is not ready.');
        return;
      }
      if (!(await onCopyOtp(code))) return;
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setOtpError('Could not decrypt this code.');
    }
  };
  const [override, setOverride] = useState<boolean | undefined>(undefined);
  const translation = useTranslation(sms.body);
  const needs = translation?.needsTranslation ?? false;
  const showTranslated = needs && (override ?? autoTranslate);

  return (
    <div className={`flex flex-col ${isOut ? 'items-end' : 'items-start'} pb-4`}>
      <div
        className={`max-w-[92%] md:max-w-[78%] rounded-2xl px-4 py-3 border ${
          isOut
            ? 'bg-blue-700 text-white border-blue-700 dark:bg-blue-600 dark:border-blue-600 glass-bubble-out'
            : 'bg-white text-slate-900 border-slate-200 dark:bg-slate-900 dark:text-slate-100 dark:border-slate-800 glass-bubble-in'
        }`}
      >
        {sms.rcsRichCard && !isOut && (
          <div className="mb-3 pb-3 border-b border-slate-100 dark:border-slate-800">
            <div className="text-[11px] font-medium text-blue-700 dark:text-blue-400">
              {sms.rcsRichCard.actionMeta || 'RCS Rich Card'}
            </div>
            <div className="mt-0.5 font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
              {sms.rcsRichCard.title}
            </div>
            <div className="mt-0.5 text-xs text-slate-600 dark:text-slate-300">
              {sms.rcsRichCard.subtitle}
            </div>
          </div>
        )}

        <p
          dir={!showTranslated && translation?.language.isRtl ? 'rtl' : 'ltr'}
          className="text-sm leading-relaxed whitespace-pre-wrap break-words"
        >
          {showTranslated && translation ? translation.translatedText : sms.body}
        </p>

        {translation && needs && (
          <div
            className={`mt-2.5 pt-2 border-t flex flex-wrap items-center justify-between gap-2 text-[11px] ${
              isOut
                ? 'border-blue-500/40 text-blue-100'
                : 'border-slate-100 dark:border-slate-800 text-slate-500 dark:text-slate-400'
            }`}
          >
            <span className="inline-flex items-center gap-1.5">
              <Languages className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
              <span>
                {showTranslated
                  ? `Translated offline from ${translation.language.label} (${translation.language.nativeLabel})`
                  : `Detected ${translation.language.label} (${translation.language.nativeLabel})`}
              </span>
            </span>
            <button
              type="button"
              onClick={() => setOverride(!showTranslated)}
              className={`font-semibold underline underline-offset-2 transition-colors ${
                isOut
                  ? 'text-white hover:text-blue-100'
                  : 'text-blue-700 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300'
              }`}
            >
              {showTranslated ? 'Show Original' : 'Translate to English'}
            </button>
          </div>
        )}

        {otp && !isOut && (
          <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <div>
              <div className="flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400">
                <Lock className="w-3 h-3 text-blue-700 dark:text-blue-400" aria-hidden="true" />
                <span>AES-256 Vault · {formatCountdown(expiresAt, nowMs)}</span>
              </div>
              <div className="mt-0.5 font-mono text-lg font-semibold tracking-wider tabular-nums text-slate-900 dark:text-slate-100">
                {revealed ?? otp.code}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => void reveal()}
                aria-label={revealed !== null ? `Hide OTP ${sms.id}` : `Reveal OTP ${sms.id}`}
                className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors"
              >
                {revealed !== null ? (
                  <>
                    <EyeOff className="w-3.5 h-3.5" aria-hidden="true" />
                    <span>Hide</span>
                  </>
                ) : (
                  <>
                    <Eye className="w-3.5 h-3.5" aria-hidden="true" />
                    <span>Reveal</span>
                  </>
                )}
              </button>
              <button
                type="button"
                onClick={() => void copyCode()}
                className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors"
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
            </div>
            {otpError && (
              <p role="alert" className="w-full text-[11px] text-red-700 dark:text-red-300">
                {otpError}
              </p>
            )}
          </div>
        )}

        {transaction && !isOut && (
          <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
            <div>
              <span className="text-slate-500 dark:text-slate-400 block">
                {transaction.direction} · {transaction.paymentMethod}
              </span>
              <span
                className={`font-mono text-sm font-semibold tabular-nums ${
                  transaction.direction === 'Credit'
                    ? 'text-emerald-700 dark:text-emerald-400'
                    : 'text-red-700 dark:text-red-400'
                }`}
              >
                {transaction.direction === 'Credit' ? '+' : '-'}
                {formatInr(transaction.amount)}
              </span>
            </div>
            {transaction.availableBalance !== null && (
              <div>
                <span className="text-slate-500 dark:text-slate-400 block">
                  {transaction.paymentMethod === 'Credit Card'
                    ? 'Available Limit'
                    : 'Available Balance'}
                </span>
                <span className="font-mono text-sm font-medium tabular-nums text-slate-900 dark:text-slate-100">
                  {formatInr(transaction.availableBalance)}
                </span>
              </div>
            )}
            {transaction.merchantOrVpa && (
              <div className="col-span-2 text-slate-600 dark:text-slate-300">
                Merchant: <span className="font-medium">{transaction.merchantOrVpa}</span>
                {transaction.accountLast4 && ` · ••${transaction.accountLast4}`}
              </div>
            )}
          </div>
        )}

        {reminder && !isOut && (
          <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <div className="text-xs">
              <span className="text-amber-700 dark:text-amber-400 font-medium block">
                Smart Reminder
              </span>
              <span className="text-slate-900 dark:text-slate-100 font-medium">
                {reminder.summary}
              </span>
            </div>
            <button
              type="button"
              onClick={() => onToggleReminderDone(sms.id)}
              className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors ${
                isDone
                  ? 'bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-950/50 dark:border-emerald-800 dark:text-emerald-300'
                  : 'bg-slate-50 border-slate-200 text-slate-700 hover:bg-slate-100 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-200'
              }`}
            >
              {isDone ? 'Completed' : 'Mark Done'}
            </button>
          </div>
        )}
      </div>

      <div className="mt-1 px-1 flex items-center gap-2 text-[11px] text-slate-500 dark:text-slate-400">
        <span className="font-mono tabular-nums">{formatFullDateTime(sms.timestampMs)}</span>
        <span aria-hidden="true">·</span>
        <span>{isRcs ? 'RCS' : `SIM ${sms.simSlot + 1}`}</span>
        {isOut && isRcs && (
          <span className="inline-flex items-center gap-0.5 text-blue-700 dark:text-blue-400">
            <CheckCheck className="w-3.5 h-3.5" aria-hidden="true" />
            <span>{sms.rcsStatus === 'read' ? 'Read' : 'Delivered'}</span>
          </span>
        )}
        {isOut && parsed.sendStatus && (
          <span
            className={
              parsed.sendStatus === 'failed'
                ? 'font-medium text-red-700 dark:text-red-400'
                : 'text-slate-500 dark:text-slate-400'
            }
          >
            {SEND_STATUS_LABEL[parsed.sendStatus]}
          </span>
        )}
        <button
          type="button"
          onClick={() => onStar(parsed)}
          aria-label={sms.isStarred ? `Unstar message ${sms.id}` : `Star message ${sms.id}`}
          className={`p-1 rounded hover:text-amber-600 transition-colors ${
            sms.isStarred ? 'text-amber-500' : 'text-slate-400'
          }`}
        >
          <Star
            className="w-3.5 h-3.5"
            fill={sms.isStarred ? 'currentColor' : 'none'}
            aria-hidden="true"
          />
        </button>
        <button
          type="button"
          onClick={() => onDelete(parsed)}
          aria-label={`Delete message ${sms.id}`}
          className="p-1 rounded text-slate-400 hover:text-red-600 transition-colors"
        >
          <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
});

export function ThreadPane({
  thread,
  version,
  nowMs,
  autoTranslateToEnglish,
  completedReminderIds,
  onToggleReminderDone,
  onCopyOtp,
  onBack,
  onChanged,
  onThreadPatched,
  caps,
}: ThreadPaneProps) {
  const [showInspector, setShowInspector] = useState(false);
  const { messages, hasOlder, loadOlder, patch, append, openedAt, prepended } =
    useThreadMessages(thread.threadId, version);
  const [replyText, setReplyText] = useState('');
  const [replySim, setReplySim] = useState<0 | 1>(thread.simSlot);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const done = new Set(completedReminderIds);

  const virt = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 132,
    overscan: 6,
    getItemKey: (i) => messages[i].sms.id,
  });

  // Open at the newest message.
  useLayoutEffect(() => {
    if (messages.length === 0) return;
    virt.scrollToIndex(messages.length - 1, { align: 'end' });
    const id = window.setTimeout(
      () => virt.scrollToIndex(messages.length - 1, { align: 'end' }),
      80
    );
    return () => window.clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openedAt]);

  // Keep the viewport anchored when older messages are prepended.
  useLayoutEffect(() => {
    if (prepended.current > 0) {
      virt.scrollToIndex(prepended.current, { align: 'start' });
      prepended.current = 0;
    }
  }, [messages, prepended, virt]);

  // Mark loaded incoming messages read once, then refresh the lists. A
  // message the system SMS store refused stays unread and is not retried in
  // this view (no refresh loop); reopening the thread retries it.
  const markTried = useRef(new Set<number>());
  useEffect(() => {
    const unread = messages.filter(
      (m) => !m.sms.isOutgoing && !m.sms.isRead && !markTried.current.has(m.sms.id)
    );
    if (unread.length === 0) return;
    let live = true;
    (async () => {
      let changed = false;
      for (const m of unread) {
        markTried.current.add(m.sms.id);
        changed = (await markRead(m.sms.id).then(() => true).catch(() => false)) || changed;
        if (!live) return;
      }
      if (changed) onChanged();
    })();
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages]);

  const handleSend = async (e: FormEvent) => {
    e.preventDefault();
    const body = replyText.trim();
    if (!body || sending || !caps.canSend) return;
    setSending(true);
    setSendError(null);
    try {
      const r = await queueOutgoing({ threadId: thread.threadId, body, simSlot: replySim });
      setReplyText('');
      append(r.message);
      onThreadPatched(r.thread);
      onChanged();
    } catch (err) {
      setSendError(err instanceof Error ? err.message : 'Could not queue the message.');
    } finally {
      setSending(false);
    }
  };

  const handleStar = (m: MessageView) => {
    const starred = !m.sms.isStarred;
    patch(m.sms.id, (x) => ({ ...x, sms: { ...x.sms, isStarred: starred } }));
    starMessage(m.sms.id, starred).then(onChanged).catch(onChanged);
  };

  const handleDelete = (m: MessageView) => {
    patch(m.sms.id, () => null);
    deleteMessages([m.sms.id])
      .then((r) => {
        if (r.failedIds.length > 0)
          setSendError('Could not delete the message from the system SMS store; it is kept. Try again.');
        onChanged();
      })
      .catch(onChanged);
  };

  const toggleArchive = () => {
    const archived = !thread.isArchived;
    onThreadPatched({ isArchived: archived });
    archiveThread(thread.threadId, archived).then(onChanged).catch(onChanged);
  };

  const toggleBlock = () => {
    const blocked = !thread.isBlocked;
    onThreadPatched({ isBlocked: blocked });
    blockSender(thread.sender, blocked).then(onChanged).catch(onChanged);
  };

  const latestReason = messages[messages.length - 1]?.classification.reason;

  return (
    <>
      <div className="bg-white dark:bg-slate-900 glass-chrome border-b border-slate-200 dark:border-slate-800 px-4 md:px-6 py-3 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <button
            type="button"
            onClick={onBack}
            aria-label="Back to conversations"
            className="md:hidden min-w-[40px] min-h-[40px] -ml-2 flex items-center justify-center text-slate-600 dark:text-slate-300 rounded-lg"
          >
            <ArrowLeft className="w-5 h-5" aria-hidden="true" />
          </button>

          <div className="min-w-0">
            <h1 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100 truncate">
              {thread.displayName}
            </h1>
            <p className="text-xs text-slate-500 dark:text-slate-400 truncate">
              <span className="font-mono">{thread.sender}</span>
              <span aria-hidden="true"> · </span>
              <span>
                {thread.isRcsCapable
                  ? 'RCS Encrypted Chat'
                  : formatSenderKindLabel(thread.senderKind)}
              </span>
              <span aria-hidden="true"> · </span>
              <span>
                {thread.classification.category}
                {thread.messageCount > 0 &&
                  ` (${thread.messageCount.toLocaleString('en-IN')} msg${thread.messageCount === 1 ? '' : 's'})`}
              </span>
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          <button
            type="button"
            onClick={() => setShowInspector((prev) => !prev)}
            aria-expanded={showInspector}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors whitespace-nowrap min-h-[36px] ${
              showInspector
                ? 'bg-blue-50 border-blue-200 text-blue-800 dark:bg-blue-950/60 dark:border-blue-800 dark:text-blue-300'
                : 'bg-white border-slate-200 text-slate-700 hover:bg-slate-50 dark:bg-slate-900 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800'
            }`}
          >
            <SlidersHorizontal className="w-3.5 h-3.5" aria-hidden="true" />
            <span className="hidden sm:inline">Engine Inspector</span>
          </button>

          <button
            type="button"
            onClick={toggleArchive}
            aria-label={
              thread.isArchived ? `Unarchive ${thread.displayName}` : `Archive ${thread.displayName}`
            }
            className="min-w-[36px] min-h-[36px] flex items-center justify-center text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-100 dark:hover:bg-slate-800 rounded-lg transition-colors"
          >
            {thread.isArchived ? (
              <ArchiveRestore className="w-4 h-4" aria-hidden="true" />
            ) : (
              <Archive className="w-4 h-4" aria-hidden="true" />
            )}
          </button>

          <button
            type="button"
            onClick={toggleBlock}
            aria-label={
              thread.isBlocked
                ? `Unblock ${thread.displayName}`
                : `Mark ${thread.displayName} as spam`
            }
            className={`min-w-[36px] min-h-[36px] flex items-center justify-center rounded-lg transition-colors ${
              thread.isBlocked
                ? 'text-red-600 bg-red-50 dark:text-red-400 dark:bg-red-950/40'
                : 'text-slate-600 hover:text-red-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-red-400 dark:hover:bg-slate-800'
            }`}
          >
            {thread.isBlocked ? (
              <ShieldCheck className="w-4 h-4" aria-hidden="true" />
            ) : (
              <ShieldAlert className="w-4 h-4" aria-hidden="true" />
            )}
          </button>
        </div>
      </div>

      {showInspector && (
        <div className="bg-white dark:bg-slate-900 border-b border-slate-200 dark:border-slate-800 px-4 md:px-6 py-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="space-y-1.5 text-xs">
              <h2 className="font-display font-semibold text-slate-900 dark:text-slate-100">
                On-Device Pipeline Trace
              </h2>
              <p className="text-slate-600 dark:text-slate-400">
                Sender parser:{' '}
                <span className="font-mono text-slate-900 dark:text-slate-200">
                  {formatSenderKindLabel(thread.senderKind)}
                </span>
              </p>
              <p className="text-slate-600 dark:text-slate-400">
                Transport:{' '}
                <span className="font-mono text-slate-900 dark:text-slate-200">
                  {thread.isRcsCapable
                    ? 'RCS Universal Profile (SMS fallback)'
                    : `Telephony SMS · SIM ${thread.simSlot + 1}`}
                </span>
              </p>
              <p className="text-slate-600 dark:text-slate-400">
                Thread consensus:{' '}
                <span className="font-mono text-slate-900 dark:text-slate-200">
                  {thread.classification.category}
                  {latestReason ? ` · ${latestReason}` : ''}
                </span>
              </p>
            </div>

            <div className="space-y-2">
              <h3 className="text-xs font-semibold text-slate-700 dark:text-slate-300">
                Category Softmax Distribution
              </h3>
              <div className="space-y-1.5">
                {CATEGORY_LIST.map((cat, idx) => {
                  const pct = thread.classification.scores[idx] * 100;
                  const isWinner = thread.classification.category === cat;
                  return (
                    <div key={cat} className="flex items-center gap-3 text-xs">
                      <span
                        className={`w-24 shrink-0 ${
                          isWinner
                            ? 'font-semibold text-slate-900 dark:text-slate-100'
                            : 'text-slate-500 dark:text-slate-400'
                        }`}
                      >
                        {cat}
                      </span>
                      <div className="flex-1 h-1.5 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
                        <div
                          className={`h-full rounded-full ${
                            isWinner ? 'bg-blue-700 dark:bg-blue-500' : 'bg-slate-300 dark:bg-slate-600'
                          }`}
                          style={{ width: `${Math.max(pct, 1.5)}%` }}
                        />
                      </div>
                      <span className="w-12 text-right font-mono tabular-nums text-slate-600 dark:text-slate-400">
                        {pct.toFixed(1)}%
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>
      )}

      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto smooth-scroll-layer px-4 md:px-6 py-5"
      >
        {hasOlder && (
          <div className="text-center pb-4">
            <button
              type="button"
              onClick={loadOlder}
              className="px-3.5 py-1.5 text-xs font-medium text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-lg hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors"
            >
              Load older messages
            </button>
          </div>
        )}
        <div style={{ height: virt.getTotalSize(), position: 'relative', width: '100%' }}>
          {virt.getVirtualItems().map((v) => {
            const m = messages[v.index];
            return (
              <div
                key={v.key}
                data-index={v.index}
                ref={virt.measureElement}
                className="vrow"
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${v.start}px)`,
                }}
              >
                <Bubble
                  parsed={m}
                  nowMs={nowMs}
                  autoTranslate={autoTranslateToEnglish}
                  isDone={done.has(m.sms.id)}
                  onToggleReminderDone={onToggleReminderDone}
                  onCopyOtp={onCopyOtp}
                  onStar={handleStar}
                  onDelete={handleDelete}
                  platform={caps.platform}
                />
              </div>
            );
          })}
        </div>
      </div>

      {caps.canSend ? (
        <form
          onSubmit={handleSend}
          className="bg-white dark:bg-slate-900 glass-chrome border-t border-slate-200 dark:border-slate-800 px-4 md:px-6 py-3 mb-16 md:mb-0"
        >
          <div className="flex items-center gap-2">
            {caps.simSlots > 1 && (
              <button
                type="button"
                onClick={() => setReplySim((s) => (s === 0 ? 1 : 0))}
                aria-label={`Switch sending SIM (currently SIM ${replySim + 1})`}
                className="px-2.5 py-2 text-xs font-mono font-semibold rounded-lg transition-colors whitespace-nowrap shrink-0 min-h-[40px] text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700"
              >
                SIM {replySim + 1}
              </button>
            )}
            <input
              type="text"
              aria-label={`Reply to ${thread.displayName}`}
              value={replyText}
              onChange={(e) => setReplyText(e.target.value)}
              placeholder={`SMS to ${thread.displayName} via SIM ${replySim + 1}...`}
              className="flex-1 min-w-0 px-3.5 py-2 text-sm bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600 min-h-[40px]"
            />
            <button
              type="submit"
              disabled={!replyText.trim() || sending}
              aria-label="Send message reply"
              className="inline-flex items-center justify-center gap-1.5 px-4 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 disabled:opacity-40 disabled:cursor-not-allowed dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors shrink-0 min-h-[40px]"
            >
              <Send className="w-3.5 h-3.5" aria-hidden="true" />
              <span className="hidden sm:inline">{sending ? 'Queuing...' : 'Send'}</span>
            </button>
          </div>
          {sendError && (
            <p role="alert" className="mt-1.5 text-[11px] text-red-700 dark:text-red-400">
              {sendError}
            </p>
          )}
        </form>
      ) : (
        <div className="bg-white dark:bg-slate-900 glass-chrome border-t border-slate-200 dark:border-slate-800 px-4 md:px-6 py-2.5 mb-16 md:mb-0 text-[11px] text-slate-500 dark:text-slate-400">
          Read-only here: this device shows your phone&apos;s backup. Reply from Snipt on your phone.
        </div>
      )}
    </>
  );
}
