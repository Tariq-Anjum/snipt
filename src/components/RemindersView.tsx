import {
  ArrowUpRight,
  Check,
  Copy,
  Eye,
  EyeOff,
  Lock,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import {
  vaultReveal,
  type CleanupBucketId,
  type CloudBackupProvider,
  type OtpItemView,
  type ReminderItem,
  type RestoreReport,
  type WeeklyScanReport,
} from '../api';
import { formatCountdown, formatFullDateTime } from '../utils/format';
import { CloudBackupSection } from './CloudBackupSection';

interface RemindersViewProps {
  preferredCloudProvider: CloudBackupProvider;
  onChangeCloudProvider: (provider: CloudBackupProvider) => void;
  lastBackupAtMs: number | null;
  onBackupSuccess: (timestampMs: number) => void;
  onRestored: (report: RestoreReport) => void;
  readOnly: boolean;
  nativePicker: boolean;
  otps: OtpItemView[];
  reminders: ReminderItem[];
  remindersLoading: boolean;
  weeklyScan: WeeklyScanReport | null;
  scanScheduleDay: string;
  onChangeScanScheduleDay: (day: string) => void;
  onApplyCleanup: (buckets: CleanupBucketId[]) => Promise<number>;
  onPurgeExpiredOtps: () => Promise<number>;
  onCopyOtp: (code: string) => void;
  clipboardClearSecondsLeft: number | null;
  onToggleReminderDone: (smsId: number) => void;
  onOpenThread: (threadId: number) => void;
  nowMs: number;
}

type ReminderFilter = 'pending' | 'completed' | 'all';

const SCHEDULE_DAYS = [
  'Every Sunday · 9:00 AM',
  'Every Saturday · 9:00 AM',
  'Every Wednesday · 8:00 PM',
  'Manual scan only',
];

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function RemindersView({
  preferredCloudProvider,
  onChangeCloudProvider,
  lastBackupAtMs,
  onBackupSuccess,
  onRestored,
  readOnly,
  nativePicker,
  otps,
  reminders,
  remindersLoading,
  weeklyScan: scan,
  scanScheduleDay,
  onChangeScanScheduleDay,
  onApplyCleanup,
  onPurgeExpiredOtps,
  onCopyOtp,
  clipboardClearSecondsLeft,
  onToggleReminderDone,
  onOpenThread,
  nowMs,
}: RemindersViewProps) {
  const [copiedId, setCopiedId] = useState<number | null>(null);
  // Plaintext codes live here only while revealed (auto-hidden after 30 s).
  const [revealed, setRevealed] = useState<Record<number, string>>({});
  const hideTimers = useRef<Record<number, number>>({});
  const weeklyScan: WeeklyScanReport = scan ?? {
    scannedAtMs: 0,
    totalScanned: 0,
    totalUselessCount: 0,
    totalEstimatedBytes: 0,
    buckets: [],
  };
  const [purgeNotice, setPurgeNotice] = useState<string | null>(null);
  const [cleanupNotice, setCleanupNotice] = useState<string | null>(null);
  const [remFilter, setRemFilter] = useState<ReminderFilter>('all');
  const [selectedBuckets, setSelectedBuckets] = useState<Record<CleanupBucketId, boolean>>({
    expired_otps: true,
    old_promotions: true,
    spam_phishing: true,
    old_deliveries: true,
  });

  useEffect(
    () => () => {
      Object.values(hideTimers.current).forEach((t) => window.clearTimeout(t));
    },
    []
  );

  const expiredCount = otps.filter((o) => o.item.isExpired).length;
  const activeCount = otps.length - expiredCount;

  const selectedDeleteIds = weeklyScan.buckets
    .filter((b) => selectedBuckets[b.id])
    .flatMap((b) => b.messageIds);

  const selectedBytes = weeklyScan.buckets
    .filter((b) => selectedBuckets[b.id])
    .reduce((sum, b) => sum + b.estimatedBytes, 0);

  const filteredReminders = reminders.filter((r) => {
    if (remFilter === 'pending') return !r.isDone;
    if (remFilter === 'completed') return r.isDone;
    return true;
  });

  const hide = (smsId: number) => {
    window.clearTimeout(hideTimers.current[smsId]);
    setRevealed((prev) => {
      const { [smsId]: _gone, ...rest } = prev;
      return rest;
    });
  };

  const toggleReveal = async (view: OtpItemView) => {
    const id = view.item.smsId;
    if (revealed[id] !== undefined) {
      hide(id);
      return;
    }
    try {
      const code = await vaultReveal(view.vault);
      setRevealed((prev) => ({ ...prev, [id]: code }));
      hideTimers.current[id] = window.setTimeout(() => hide(id), 30_000);
    } catch {
      setPurgeNotice('Could not decrypt this vault item (the vault key is per session).');
    }
  };

  const handleCopy = async (view: OtpItemView) => {
    try {
      const code = revealed[view.item.smsId] ?? (await vaultReveal(view.vault));
      onCopyOtp(code);
      setCopiedId(view.item.smsId);
      window.setTimeout(() => setCopiedId(null), 1800);
    } catch {
      setPurgeNotice('Could not decrypt this vault item (the vault key is per session).');
    }
  };

  const handlePurge = async () => {
    const removed = await onPurgeExpiredOtps().catch(() => 0);
    setPurgeNotice(
      removed > 0
        ? `Purged ${removed} expired OTP message${removed === 1 ? '' : 's'} from encrypted vault.`
        : 'No expired OTP-only messages eligible for cleanup.'
    );
  };

  const handleDeleteUseless = async () => {
    if (selectedDeleteIds.length === 0) return;
    const buckets = weeklyScan.buckets.filter((b) => selectedBuckets[b.id]).map((b) => b.id);
    const bytes = selectedBytes;
    const removed = await onApplyCleanup(buckets).catch(() => 0);
    setCleanupNotice(
      `Cleaned ${removed.toLocaleString('en-IN')} useless SMS (${formatBytes(bytes)} reclaimed). Personal chats, bank alerts, and starred SMS were untouched.`
    );
  };

  return (
    <main className="flex-1 overflow-y-auto smooth-scroll-layer bg-slate-50 dark:bg-slate-950 glass:bg-transparent px-4 md:px-8 py-6 pb-24 md:pb-10">
      <div className="max-w-7xl mx-auto space-y-8">
        <div>
          <h1 className="font-display text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
            OTP Vault, Cloud Backup &amp; Weekly Cleanup
          </h1>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            Encrypted backup to Google Drive, OneDrive, Dropbox, or device storage, plus AES-256 OTP isolation, weekly clutter scan, and reminders.
          </p>
        </div>

        {/* Section 0: Encrypted Multi-Cloud & Device Backup */}
        <CloudBackupSection
          preferredProvider={preferredCloudProvider}
          onChangeProvider={onChangeCloudProvider}
          lastBackupAtMs={lastBackupAtMs}
          onBackupSuccess={onBackupSuccess}
          onRestored={onRestored}
          readOnly={readOnly}
          nativePicker={nativePicker}
        />

        {/* Section 1: Weekly Useless SMS Scan (Categorized Cleanup) */}
        <section
          aria-label="Weekly useless SMS scanner"
          className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden"
        >
          <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col lg:flex-row lg:items-center justify-between gap-4">
            <div>
              <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                Weekly Useless SMS Scan
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5 tabular-nums">
                Scanned {weeklyScan.totalScanned.toLocaleString('en-IN')} messages · Found{' '}
                {weeklyScan.totalUselessCount.toLocaleString('en-IN')} safe-to-delete SMS (
                {formatBytes(weeklyScan.totalEstimatedBytes)})
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2.5">
              <label htmlFor="weekly-scan-schedule" className="sr-only">
                Weekly scan schedule
              </label>
              <select
                id="weekly-scan-schedule"
                value={scanScheduleDay}
                onChange={(e) => onChangeScanScheduleDay(e.target.value)}
                className="px-3 py-2 text-xs bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-lg text-slate-800 dark:text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-600"
              >
                {SCHEDULE_DAYS.map((opt) => (
                  <option key={opt} value={opt}>
                    {opt}
                  </option>
                ))}
              </select>

              <button
                type="button"
                onClick={() => void handleDeleteUseless()}
                disabled={selectedDeleteIds.length === 0}
                className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold text-white bg-red-600 hover:bg-red-700 disabled:opacity-40 disabled:cursor-not-allowed rounded-lg transition-colors whitespace-nowrap"
              >
                <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                <span>
                  Delete Selected ({selectedDeleteIds.length.toLocaleString('en-IN')})
                </span>
              </button>
            </div>
          </div>

          {cleanupNotice && (
            <div
              role="status"
              className="px-5 py-2.5 text-xs font-medium text-emerald-800 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 border-b border-emerald-100 dark:border-emerald-900/60"
            >
              {cleanupNotice}
            </div>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 divide-y md:divide-y-0 md:gap-px bg-slate-100 dark:bg-slate-800">
            {weeklyScan.buckets.map((bucket) => {
              const checked = selectedBuckets[bucket.id];
              const count = bucket.messageIds.length;
              return (
                <div
                  key={bucket.id}
                  className="bg-white dark:bg-slate-900 p-4 flex flex-col justify-between gap-3"
                >
                  <div className="flex items-start justify-between gap-3">
                    <label className="flex items-start gap-2.5 cursor-pointer flex-1 min-w-0">
                      <input
                        type="checkbox"
                        checked={checked}
                        disabled={count === 0}
                        onChange={(e) =>
                          setSelectedBuckets((prev) => ({
                            ...prev,
                            [bucket.id]: e.target.checked,
                          }))
                        }
                        className="mt-0.5 w-4 h-4 rounded border-slate-300 text-blue-700 focus:ring-blue-600"
                      />
                      <div className="min-w-0">
                        <div className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          {bucket.title}
                        </div>
                        <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                          {bucket.description}
                        </p>
                      </div>
                    </label>
                    <div className="text-right shrink-0 font-mono tabular-nums">
                      <div className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                        {count.toLocaleString('en-IN')}
                      </div>
                      <div className="text-[11px] text-slate-500 dark:text-slate-400">
                        {formatBytes(bucket.estimatedBytes)}
                      </div>
                    </div>
                  </div>

                  {bucket.samples.length > 0 && (
                    <div className="pl-6 space-y-1 text-[11px] text-slate-500 dark:text-slate-400">
                      {bucket.samples.slice(0, 2).map((s) => (
                        <p key={s.smsId} className="truncate">
                          <span className="font-mono text-slate-700 dark:text-slate-300">
                            {s.sender}
                          </span>
                          : {s.body}
                        </p>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </section>

        {/* Section 2: Encrypted OTP Vault */}
        <section
          aria-label="Encrypted OTP Vault"
          className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden"
        >
          <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div>
              <div className="flex items-center gap-2">
                <Lock className="w-4 h-4 text-blue-700 dark:text-blue-400" aria-hidden="true" />
                <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                  Encrypted OTP Vault
                </h2>
              </div>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5 tabular-nums">
                AES-256-GCM isolated from third-party apps · {activeCount} active · {expiredCount}{' '}
                expired
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              {clipboardClearSecondsLeft !== null && (
                <span className="text-xs font-mono tabular-nums text-blue-700 dark:text-blue-400 flex items-center gap-1">
                  <ShieldCheck className="w-3.5 h-3.5" aria-hidden="true" />
                  <span>Clipboard clears in {clipboardClearSecondsLeft}s</span>
                </span>
              )}

              <button
                type="button"
                onClick={() => void handlePurge()}
                disabled={expiredCount === 0}
                className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold text-red-700 dark:text-red-300 bg-red-50 hover:bg-red-100 disabled:opacity-45 disabled:cursor-not-allowed dark:bg-red-950/50 dark:hover:bg-red-950 rounded-lg transition-colors whitespace-nowrap"
              >
                <RefreshCw className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Purge Expired OTPs ({expiredCount})</span>
              </button>
            </div>
          </div>

          {purgeNotice && (
            <div
              role="status"
              className="px-5 py-2.5 text-xs font-medium text-emerald-800 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 border-b border-emerald-100 dark:border-emerald-900/60"
            >
              {purgeNotice}
            </div>
          )}

          {otps.length === 0 ? (
            <div className="p-8 text-center text-xs text-slate-500 dark:text-slate-400">
              No OTP messages in your encrypted vault.
            </div>
          ) : (
            <div className="divide-y divide-slate-100 dark:divide-slate-800">
              {otps.slice(0, 15).map((view) => {
                const item = view.item;
                const code = revealed[item.smsId];
                const isRevealed = code !== undefined;
                return (
                  <div
                    key={item.smsId}
                    className="px-5 py-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-slate-500 dark:text-slate-400">
                        <span className="font-semibold text-slate-900 dark:text-slate-100">
                          {item.sender}
                        </span>
                        <span aria-hidden="true">·</span>
                        <span>SIM {item.simSlot + 1}</span>
                        <span aria-hidden="true">·</span>
                        <span className="font-mono tabular-nums">
                          {formatFullDateTime(item.timestampMs)}
                        </span>
                        <span aria-hidden="true">·</span>
                        <span
                          className={
                            item.isExpired
                              ? 'text-red-600 dark:text-red-400 font-medium'
                              : 'text-emerald-700 dark:text-emerald-400 font-medium'
                          }
                        >
                          {formatCountdown(item.expiresAtMs, nowMs)}
                        </span>
                      </div>
                      <p className="mt-1 text-xs text-slate-600 dark:text-slate-300 line-clamp-2">
                        {isRevealed ? item.body : 'Message hidden until you reveal the code.'}
                      </p>
                    </div>

                    <div className="flex items-center gap-2.5 shrink-0">
                      <span className="font-mono text-lg font-semibold tracking-wider tabular-nums text-slate-900 dark:text-slate-100">
                        {isRevealed ? code : item.otp.code}
                      </span>

                      <button
                        type="button"
                        onClick={() => void toggleReveal(view)}
                        aria-label={
                          isRevealed
                            ? `Hide OTP from ${item.sender}`
                            : `Reveal OTP from ${item.sender}`
                        }
                        className="p-2 text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100 bg-slate-100 dark:bg-slate-800 rounded-lg"
                      >
                        {isRevealed ? (
                          <EyeOff className="w-4 h-4" aria-hidden="true" />
                        ) : (
                          <Eye className="w-4 h-4" aria-hidden="true" />
                        )}
                      </button>

                      <button
                        type="button"
                        onClick={() => void handleCopy(view)}
                        aria-label={`Copy OTP from ${item.sender}`}
                        className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors whitespace-nowrap"
                      >
                        {copiedId === item.smsId ? (
                          <>
                            <Check className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-3.5 h-3.5" aria-hidden="true" />
                            <span>Copy</span>
                          </>
                        )}
                      </button>

                      <button
                        type="button"
                        onClick={() => onOpenThread(item.threadId)}
                        aria-label={`Open SMS thread from ${item.sender}`}
                        className="p-2 text-slate-500 hover:text-blue-700 dark:text-slate-400 dark:hover:text-blue-400 rounded-lg"
                      >
                        <ArrowUpRight className="w-4 h-4" aria-hidden="true" />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* Section 3: Smart Reminders */}
        <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden">
          <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3">
            <div>
              <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                Smart Reminders
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                Travel PNRs, credit card bill due dates, and package deliveries extracted offline.
              </p>
            </div>

            <div className="flex items-center gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg self-start sm:self-auto">
              {(
                [
                  { id: 'all', label: 'All' },
                  { id: 'pending', label: 'Pending' },
                  { id: 'completed', label: 'Completed' },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setRemFilter(tab.id)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                    remFilter === tab.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          </div>

          {filteredReminders.length === 0 ? (
            <div className="p-8 text-center text-xs text-slate-500 dark:text-slate-400">
              {remindersLoading ? 'Scanning recent threads for reminders...' : 'No reminders in this view.'}
            </div>
          ) : (
            <div className="divide-y divide-slate-100 dark:divide-slate-800">
              {filteredReminders.slice(0, 15).map((item) => (
                <div
                  key={item.smsId}
                  className="px-5 py-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-slate-500 dark:text-slate-400">
                      <span className="font-medium text-amber-700 dark:text-amber-400">
                        {item.reminder.kind === 'TravelPnr'
                          ? 'Travel PNR'
                          : item.reminder.kind === 'BillDue'
                            ? 'Bill Due'
                            : 'Delivery'}
                      </span>
                      <span aria-hidden="true">·</span>
                      <span>{item.sender}</span>
                      <span aria-hidden="true">·</span>
                      <span className="font-mono tabular-nums">
                        {formatFullDateTime(item.timestampMs)}
                      </span>
                    </div>

                    <p
                      className={`mt-1 text-sm font-semibold ${
                        item.isDone
                          ? 'line-through text-slate-400 dark:text-slate-500'
                          : 'text-slate-900 dark:text-slate-100'
                      }`}
                    >
                      {item.reminder.summary}
                    </p>
                    <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 line-clamp-2">
                      {item.body}
                    </p>
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      type="button"
                      onClick={() => onToggleReminderDone(item.smsId)}
                      className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors whitespace-nowrap ${
                        item.isDone
                          ? 'bg-emerald-50 border-emerald-200 text-emerald-800 dark:bg-emerald-950/50 dark:border-emerald-800 dark:text-emerald-300'
                          : 'bg-white border-slate-200 text-slate-700 hover:bg-slate-50 dark:bg-slate-900 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800'
                      }`}
                    >
                      {item.isDone ? 'Completed' : 'Mark Done'}
                    </button>

                    <button
                      type="button"
                      onClick={() => onOpenThread(item.threadId)}
                      aria-label={`Open source SMS for ${item.reminder.summary}`}
                      className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-blue-700 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300"
                    >
                      <span>SMS</span>
                      <ArrowUpRight className="w-3.5 h-3.5" aria-hidden="true" />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
