import {
  Archive,
  ArchiveRestore,
  ArrowLeft,
  Check,
  CheckCheck,
  Copy,
  Database,
  Droplets,
  Eye,
  EyeOff,
  Languages,
  Lock,
  Moon,
  RotateCcw,
  Search,
  Send,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Sun,
  Trash2,
  X,
} from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useState, type FormEvent } from 'react';
import type { DesignStyle } from './TopBar';
import {
  CATEGORY_LIST,
  formatSenderKindLabel,
  MIN_HISTORY_CAPACITY,
  translateSmsToEnglish,
  type Category,
  type MessageProtocol,
  type ThreadSummary,
} from '../engine';
import type { InboxFilter } from '../engine/store';
import {
  formatCountdown,
  formatFullDateTime,
  formatInr,
  formatShortTime,
  getInitials,
} from '../utils/format';

interface InboxViewProps {
  allThreads: ThreadSummary[];
  filteredThreads: ThreadSummary[];
  totalMessageCount: number;
  isLoadingBulk: boolean;
  onLoadTenThousand: () => void;
  activeFilter: InboxFilter;
  onChangeFilter: (filter: InboxFilter) => void;
  simFilter: 'all' | 0 | 1;
  onChangeSimFilter: (sim: 'all' | 0 | 1) => void;
  searchQuery: string;
  onChangeSearch: (q: string) => void;
  selectedThreadId: number | null;
  onSelectThread: (threadId: number | null) => void;
  onSendReply: (
    threadId: number,
    body: string,
    simSlot: 0 | 1,
    protocol: MessageProtocol
  ) => void;
  onToggleStar: (smsId: number) => void;
  onDeleteMessage: (smsId: number) => void;
  onToggleArchive: (threadId: number) => void;
  onToggleBlock: (sender: string) => void;
  onCopyOtp: (code: string) => void;
  completedReminderIds: number[];
  onToggleReminderDone: (smsId: number) => void;
  theme: 'light' | 'dark';
  onToggleTheme: () => void;
  designStyle: DesignStyle;
  onChangeDesignStyle: (style: DesignStyle) => void;
  autoTranslateToEnglish: boolean;
  onToggleAutoTranslate: () => void;
  onResetDemo: () => void;
  nowMs: number;
}

const PRIMARY_FILTERS: InboxFilter[] = [
  'All',
  'Personal',
  'Priority',
  'Transactions',
  'Promotions',
  'Spam',
];

const SECONDARY_FILTERS: InboxFilter[] = ['RCS', 'Starred', 'Archived'];

const INITIAL_MSG_WINDOW = 40;

function categoryAvatarClass(cat: Category): string {
  switch (cat) {
    case 'Personal':
      return 'bg-blue-50 text-blue-700 dark:bg-blue-950/70 dark:text-blue-300';
    case 'Transactions':
      return 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/70 dark:text-emerald-300';
    case 'Priority':
      return 'bg-amber-50 text-amber-800 dark:bg-amber-950/70 dark:text-amber-300';
    case 'Promotions':
      return 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300';
    case 'Spam':
      return 'bg-red-50 text-red-700 dark:bg-red-950/70 dark:text-red-300';
  }
}

export function InboxView({
  allThreads,
  filteredThreads,
  totalMessageCount,
  isLoadingBulk,
  onLoadTenThousand,
  activeFilter,
  onChangeFilter,
  simFilter,
  onChangeSimFilter,
  searchQuery,
  onChangeSearch,
  selectedThreadId,
  onSelectThread,
  onSendReply,
  onToggleStar,
  onDeleteMessage,
  onToggleArchive,
  onToggleBlock,
  onCopyOtp,
  completedReminderIds,
  onToggleReminderDone,
  theme,
  onToggleTheme,
  designStyle,
  onChangeDesignStyle,
  autoTranslateToEnglish,
  onToggleAutoTranslate,
  onResetDemo,
  nowMs,
}: InboxViewProps) {
  const [replyText, setReplyText] = useState('');
  const [replySim, setReplySim] = useState<0 | 1>(0);
  const [replyProtocol, setReplyProtocol] = useState<MessageProtocol>('SMS');
  const [showInspector, setShowInspector] = useState(false);
  const [copiedOtpId, setCopiedOtpId] = useState<number | null>(null);
  const [revealedOtpIds, setRevealedOtpIds] = useState<Record<number, boolean>>({});
  const [translationOverrides, setTranslationOverrides] = useState<
    Record<number, boolean>
  >({});
  const [msgWindow, setMsgWindow] = useState(INITIAL_MSG_WINDOW);

  const selectedThread =
    allThreads.find((t) => t.threadId === selectedThreadId) ?? null;

  useEffect(() => {
    setMsgWindow(INITIAL_MSG_WINDOW);
    if (selectedThread) {
      setReplyProtocol(selectedThread.isRcsCapable ? 'RCS' : 'SMS');
      setReplySim(selectedThread.simSlot);
    }
  }, [selectedThreadId]);

  const completedSet = new Set(completedReminderIds);

  const countForFilter = (filter: InboxFilter): number => {
    if (filter === 'Archived') {
      return allThreads.filter((t) => t.isArchived).length;
    }
    const active = allThreads.filter((t) => !t.isArchived);
    if (filter === 'All') {
      return active.filter((t) => t.classification.category !== 'Spam').length;
    }
    if (filter === 'RCS') {
      return active.filter((t) => t.isRcsCapable).length;
    }
    if (filter === 'Starred') {
      return active.filter((t) => t.hasStarred).length;
    }
    return active.filter((t) => t.classification.category === filter).length;
  };

  const handleSend = (e: FormEvent) => {
    e.preventDefault();
    if (!selectedThread || !replyText.trim()) return;
    onSendReply(selectedThread.threadId, replyText, replySim, replyProtocol);
    setReplyText('');
  };

  const handleCopyCode = (smsId: number, code: string) => {
    onCopyOtp(code);
    setCopiedOtpId(smsId);
    window.setTimeout(() => setCopiedOtpId(null), 1800);
  };

  const toggleRevealOtp = (smsId: number) => {
    setRevealedOtpIds((prev) => ({ ...prev, [smsId]: !prev[smsId] }));
  };

  const visibleMessages = selectedThread
    ? selectedThread.messages.length > msgWindow
      ? selectedThread.messages.slice(selectedThread.messages.length - msgWindow)
      : selectedThread.messages
    : [];

  const hiddenCount = selectedThread
    ? Math.max(0, selectedThread.messages.length - visibleMessages.length)
    : 0;

  return (
    <div className="flex-1 flex min-h-0 overflow-hidden bg-slate-50 dark:bg-slate-950 glass:bg-transparent">
      {/* Pane 1: Desktop Category, RCS, SIM, Capacity, Translation & Surface Style Rail */}
      <aside
        aria-label="Inbox folders and SIM filter"
        className="hidden lg:flex w-60 shrink-0 flex-col justify-between bg-white dark:bg-slate-900 glass-chrome border-r border-slate-200 dark:border-slate-800 px-3.5 py-3 overflow-y-auto smooth-scroll-layer"
      >
        <div className="space-y-3.5">
          <div>
            <h2 className="px-2.5 text-[11px] font-semibold text-slate-500 dark:text-slate-400 mb-1.5">
              Categories
            </h2>
            <div className="space-y-0.5">
              {PRIMARY_FILTERS.map((filter) => {
                const active = activeFilter === filter;
                const count = countForFilter(filter);
                return (
                  <button
                    key={filter}
                    type="button"
                    onClick={() => onChangeFilter(filter)}
                    aria-pressed={active}
                    className={`w-full flex items-center justify-between px-2.5 py-1.5 text-sm rounded-lg transition-colors ${
                      active
                        ? 'bg-blue-50 text-blue-800 font-semibold dark:bg-blue-950/60 dark:text-blue-300'
                        : 'text-slate-700 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800/70'
                    }`}
                  >
                    <span className="truncate">{filter}</span>
                    <span className="font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
                      {count}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="pt-3 border-t border-slate-100 dark:border-slate-800">
            <h2 className="px-2.5 text-[11px] font-semibold text-slate-500 dark:text-slate-400 mb-1.5">
              Collections
            </h2>
            <div className="space-y-0.5">
              {SECONDARY_FILTERS.map((filter) => {
                const active = activeFilter === filter;
                const count = countForFilter(filter);
                return (
                  <button
                    key={filter}
                    type="button"
                    onClick={() => onChangeFilter(filter)}
                    aria-pressed={active}
                    className={`w-full flex items-center justify-between px-2.5 py-1.5 text-sm rounded-lg transition-colors ${
                      active
                        ? 'bg-blue-50 text-blue-800 font-semibold dark:bg-blue-950/60 dark:text-blue-300'
                        : 'text-slate-700 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800/70'
                    }`}
                  >
                    <span className="truncate">
                      {filter === 'RCS' ? 'RCS Chats' : filter}
                    </span>
                    <span className="font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
                      {count}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="pt-3 border-t border-slate-100 dark:border-slate-800">
            <h2 className="px-2.5 text-[11px] font-semibold text-slate-500 dark:text-slate-400 mb-1.5">
              Dual-SIM Filter
            </h2>
            <div className="grid grid-cols-3 gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg">
              {(
                [
                  { id: 'all', label: 'All' },
                  { id: 0, label: 'SIM 1' },
                  { id: 1, label: 'SIM 2' },
                ] as const
              ).map((item) => (
                <button
                  key={String(item.id)}
                  type="button"
                  onClick={() => onChangeSimFilter(item.id)}
                  className={`py-1 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                    simFilter === item.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
            <p className="mt-1.5 px-1 text-[11px] text-slate-500 dark:text-slate-400">
              SIM 1: Airtel 5G · SIM 2: Jio 5G
            </p>
          </div>

          {/* 10,000+ SMS History Capacity Control */}
          <div className="pt-3 border-t border-slate-100 dark:border-slate-800 space-y-1.5">
            <div className="px-1 flex items-center justify-between text-xs">
              <span className="font-semibold text-slate-600 dark:text-slate-400">
                History Storage
              </span>
              <span className="font-mono tabular-nums text-slate-900 dark:text-slate-100 font-medium">
                {totalMessageCount.toLocaleString('en-IN')} / {MIN_HISTORY_CAPACITY.toLocaleString('en-IN')}+
              </span>
            </div>
            <div className="h-1.5 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-700 dark:bg-blue-500 rounded-full"
                style={{
                  width: `${Math.min(100, Math.max(4, (totalMessageCount / MIN_HISTORY_CAPACITY) * 100))}%`,
                }}
              />
            </div>
            {totalMessageCount < MIN_HISTORY_CAPACITY && (
              <button
                type="button"
                onClick={onLoadTenThousand}
                disabled={isLoadingBulk}
                className="w-full inline-flex items-center justify-center gap-1.5 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors"
              >
                <Database className="w-3.5 h-3.5" aria-hidden="true" />
                <span>
                  {isLoadingBulk ? 'Indexing 10,000...' : 'Load 10,000 SMS'}
                </span>
              </button>
            )}
          </div>

          {/* Offline Lightweight Language Pack Toggle */}
          <div className="pt-3 border-t border-slate-100 dark:border-slate-800 space-y-1">
            <div className="px-1 flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-slate-600 dark:text-slate-400">
                Translate to English
              </span>
              <button
                type="button"
                role="switch"
                aria-checked={autoTranslateToEnglish}
                aria-label="Auto-translate Hindi, Urdu, Hinglish and regional SMS to English"
                onClick={onToggleAutoTranslate}
                className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
                  autoTranslateToEnglish
                    ? 'bg-blue-700 dark:bg-blue-600'
                    : 'bg-slate-300 dark:bg-slate-700'
                }`}
              >
                <span
                  className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                    autoTranslateToEnglish ? 'translate-x-4.5' : 'translate-x-1'
                  }`}
                />
              </button>
            </div>
            <p className="px-1 text-[11px] text-slate-500 dark:text-slate-400 leading-snug">
              Offline · Hindi, Urdu, Hinglish, Marathi, Tamil, Telugu, Bengali
            </p>
          </div>
        </div>

        <div className="pt-3 mt-3 border-t border-slate-100 dark:border-slate-800 space-y-2">
          {/* Liquid Glass vs Crisp Solid Surface Switcher */}
          <div>
            <p className="px-1 text-[11px] font-semibold text-slate-500 dark:text-slate-400 mb-1">
              Interface Surface
            </p>
            <div className="grid grid-cols-2 gap-1 p-1 bg-slate-100/90 dark:bg-slate-800/80 rounded-lg">
              {(
                [
                  { id: 'glass', label: 'Liquid Glass' },
                  { id: 'solid', label: 'Crisp Solid' },
                ] as const
              ).map((mode) => (
                <button
                  key={mode.id}
                  type="button"
                  onClick={() => onChangeDesignStyle(mode.id)}
                  aria-pressed={designStyle === mode.id}
                  className={`py-1 text-[11px] font-medium rounded-md transition-colors whitespace-nowrap ${
                    designStyle === mode.id
                      ? 'bg-white dark:bg-slate-900 text-blue-700 dark:text-blue-300 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {mode.label}
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-0.5">
            <button
              type="button"
              onClick={onToggleTheme}
              className="w-full flex items-center gap-2 px-2.5 py-1.5 text-xs font-medium text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-100 dark:hover:bg-slate-800 rounded-lg transition-colors"
            >
              {theme === 'dark' ? (
                <>
                  <Sun className="w-4 h-4" aria-hidden="true" />
                  <span>Light appearance</span>
                </>
              ) : (
                <>
                  <Moon className="w-4 h-4" aria-hidden="true" />
                  <span>Dark appearance</span>
                </>
              )}
            </button>
            <button
              type="button"
              onClick={onResetDemo}
              className="w-full flex items-center gap-2 px-2.5 py-2 text-xs font-medium text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-100 dark:hover:bg-slate-800 rounded-lg transition-colors"
            >
              <RotateCcw className="w-4 h-4" aria-hidden="true" />
              <span>Reset sample inbox</span>
            </button>
          </div>
        </div>
      </aside>

      {/* Pane 2: Thread List */}
      <section
        aria-label="Conversation list"
        className={`${
          selectedThread ? 'hidden md:flex' : 'flex'
        } w-full md:w-[340px] xl:w-[380px] shrink-0 flex-col bg-white dark:bg-slate-900 glass-panel border-r border-slate-200 dark:border-slate-800 min-h-0`}
      >
        <div className="p-3.5 border-b border-slate-200 dark:border-slate-800 space-y-3">
          <div className="relative">
            <Search
              className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"
              aria-hidden="true"
            />
            <input
              type="search"
              aria-label="Search SMS and RCS messages, OTPs, UPI merchants, or PNRs"
              value={searchQuery}
              onChange={(e) => onChangeSearch(e.target.value)}
              placeholder="Search SMS, RCS, UPI, OTP, PNR..."
              className="w-full pl-9 pr-8 py-2 text-sm bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 rounded-lg text-slate-900 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-600"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => onChangeSearch('')}
                aria-label="Clear search query"
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-slate-400 hover:text-slate-700 dark:hover:text-slate-200"
              >
                <X className="w-3.5 h-3.5" aria-hidden="true" />
              </button>
            )}
          </div>

          {/* Mobile / Tablet Horizontal Category Selector */}
          <div className="flex lg:hidden items-center gap-1 overflow-x-auto pb-0.5">
            {[...PRIMARY_FILTERS, ...SECONDARY_FILTERS].map((filter) => {
              const active = activeFilter === filter;
              return (
                <button
                  key={filter}
                  type="button"
                  onClick={() => onChangeFilter(filter)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap shrink-0 min-h-[34px] ${
                    active
                      ? 'bg-blue-700 text-white dark:bg-blue-600'
                      : 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300'
                  }`}
                >
                  {filter === 'RCS' ? 'RCS Chats' : filter}
                </button>
              );
            })}
          </div>

          {/* Mobile / Tablet Dual-SIM Filter & Appearance Toggle */}
          <div className="flex lg:hidden items-center justify-between gap-2">
            <div className="inline-flex items-center gap-1 p-0.5 bg-slate-100 dark:bg-slate-800 rounded-lg">
              {(
                [
                  { id: 'all', label: 'All SIMs' },
                  { id: 0, label: 'SIM 1' },
                  { id: 1, label: 'SIM 2' },
                ] as const
              ).map((item) => (
                <button
                  key={String(item.id)}
                  type="button"
                  onClick={() => onChangeSimFilter(item.id)}
                  className={`px-2.5 py-1 text-[11px] font-medium rounded-md transition-colors whitespace-nowrap ${
                    simFilter === item.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>

            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() =>
                  onChangeDesignStyle(designStyle === 'glass' ? 'solid' : 'glass')
                }
                aria-pressed={designStyle === 'glass'}
                aria-label="Toggle Liquid Glass interface style"
                className={`min-h-[36px] px-2.5 inline-flex items-center gap-1 text-[11px] font-semibold rounded-lg transition-colors ${
                  designStyle === 'glass'
                    ? 'bg-blue-50 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300'
                    : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300'
                }`}
              >
                <Droplets className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Glass</span>
              </button>

              <button
                type="button"
                onClick={onToggleAutoTranslate}
                aria-pressed={autoTranslateToEnglish}
                aria-label="Toggle offline English translation"
                className={`min-h-[36px] px-2.5 inline-flex items-center gap-1 text-[11px] font-semibold rounded-lg transition-colors ${
                  autoTranslateToEnglish
                    ? 'bg-blue-50 text-blue-700 dark:bg-blue-950/60 dark:text-blue-300'
                    : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300'
                }`}
              >
                <Languages className="w-3.5 h-3.5" aria-hidden="true" />
                <span>EN</span>
              </button>

              <button
                type="button"
                onClick={onToggleTheme}
                aria-label={
                  theme === 'dark' ? 'Switch to light appearance' : 'Switch to dark appearance'
                }
                className="min-w-[36px] min-h-[36px] flex items-center justify-center text-slate-600 dark:text-slate-300 bg-slate-100 dark:bg-slate-800 rounded-lg"
              >
                {theme === 'dark' ? (
                  <Sun className="w-4 h-4" aria-hidden="true" />
                ) : (
                  <Moon className="w-4 h-4" aria-hidden="true" />
                )}
              </button>
            </div>
          </div>
        </div>

        {/* Thread rows */}
        <div className="flex-1 overflow-y-auto smooth-scroll-layer divide-y divide-slate-100 dark:divide-slate-800/80 pb-20 md:pb-4">
          {filteredThreads.length === 0 ? (
            <div className="p-8 text-center">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">
                No matching conversations
              </p>
              <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
                Try another keyword, switch SIM filters, or simulate an incoming message.
              </p>
              {(searchQuery || activeFilter !== 'All' || simFilter !== 'all') && (
                <button
                  type="button"
                  onClick={() => {
                    onChangeSearch('');
                    onChangeFilter('All');
                    onChangeSimFilter('all');
                  }}
                  className="mt-4 px-3.5 py-2 text-xs font-semibold text-blue-700 dark:text-blue-400 bg-blue-50 dark:bg-blue-950/50 rounded-lg hover:bg-blue-100 dark:hover:bg-blue-950 transition-colors"
                >
                  Clear filters
                </button>
              )}
            </div>
          ) : (
            filteredThreads.map((thread) => {
              const isSelected = selectedThreadId === thread.threadId;
              const latest = thread.latestMessage;
              const latestTranslation = translateSmsToEnglish(latest.sms.body);
              const snippetText =
                autoTranslateToEnglish && latestTranslation.needsTranslation
                  ? latestTranslation.translatedText
                  : latest.sms.body;
              const highlightMeta = latest.otp
                ? 'Encrypted OTP'
                : latest.transaction
                  ? `${latest.transaction.paymentMethod} ${
                      latest.transaction.direction === 'Debit' ? '-' : '+'
                    }${formatInr(latest.transaction.amount)}`
                  : latest.reminder
                    ? latest.reminder.reference || latest.reminder.dateText || latest.reminder.kind
                    : latestTranslation.needsTranslation
                      ? `${latestTranslation.language.label} → EN`
                      : null;

              return (
                <button
                  key={thread.threadId}
                  type="button"
                  onClick={() => onSelectThread(thread.threadId)}
                  className={`w-full text-left px-4 py-3.5 flex items-start gap-3 transition-colors min-h-[76px] focus-visible:outline-2 focus-visible:outline-inset focus-visible:outline-blue-600 ${
                    isSelected
                      ? 'bg-blue-50/80 dark:bg-blue-950/35'
                      : 'hover:bg-slate-50 dark:hover:bg-slate-800/40'
                  }`}
                >
                  <div
                    aria-hidden="true"
                    className={`w-9 h-9 rounded-full shrink-0 flex items-center justify-center text-xs font-semibold mt-0.5 ${categoryAvatarClass(
                      thread.classification.category
                    )}`}
                  >
                    {getInitials(thread.displayName)}
                  </div>

                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span
                        className={`text-sm truncate ${
                          thread.unreadCount > 0
                            ? 'font-semibold text-slate-950 dark:text-white'
                            : 'font-medium text-slate-800 dark:text-slate-200'
                        }`}
                      >
                        {thread.displayName}
                      </span>
                      <span className="text-[11px] font-mono tabular-nums text-slate-500 dark:text-slate-400 shrink-0">
                        {formatShortTime(latest.sms.timestampMs, nowMs)}
                      </span>
                    </div>

                    <p
                      className={`mt-0.5 text-xs line-clamp-2 ${
                        thread.unreadCount > 0
                          ? 'text-slate-800 dark:text-slate-200 font-medium'
                          : 'text-slate-500 dark:text-slate-400'
                      }`}
                    >
                      {latest.sms.isOutgoing ? `You: ${snippetText}` : snippetText}
                    </p>

                    <div className="mt-1.5 flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400 truncate">
                      <span>{thread.classification.category}</span>
                      <span aria-hidden="true">·</span>
                      <span>{thread.isRcsCapable ? 'RCS' : `SIM ${thread.simSlot + 1}`}</span>
                      {highlightMeta && (
                        <>
                          <span aria-hidden="true">·</span>
                          <span className="font-mono tabular-nums font-medium text-slate-700 dark:text-slate-300 truncate">
                            {highlightMeta}
                          </span>
                        </>
                      )}
                      {thread.unreadCount > 0 && (
                        <>
                          <span aria-hidden="true">·</span>
                          <span className="font-semibold text-blue-700 dark:text-blue-400 shrink-0">
                            {thread.unreadCount} new
                          </span>
                        </>
                      )}
                    </div>
                  </div>
                </button>
              );
            })
          )}
        </div>
      </section>

      {/* Pane 3: Conversation & Engine Inspector */}
      <section
        aria-label="Active conversation"
        className={`${
          selectedThread ? 'flex' : 'hidden md:flex'
        } flex-1 min-w-0 flex-col bg-slate-50 dark:bg-slate-950 glass:bg-transparent min-h-0`}
      >
        {!selectedThread ? (
          <div className="flex-1 flex flex-col items-center justify-center p-8 text-center">
            <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
              Select a conversation
            </h2>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400 max-w-sm">
              Choose any SMS or RCS thread to inspect encrypted OTPs, UPI/Card transactions, reminders, and engine scores.
            </p>
          </div>
        ) : (
          <>
            {/* Thread Header */}
            <div className="bg-white dark:bg-slate-900 glass-chrome border-b border-slate-200 dark:border-slate-800 px-4 md:px-6 py-3 flex items-center justify-between gap-4">
              <div className="flex items-center gap-3 min-w-0">
                <button
                  type="button"
                  onClick={() => onSelectThread(null)}
                  aria-label="Back to conversations"
                  className="md:hidden min-w-[40px] min-h-[40px] -ml-2 flex items-center justify-center text-slate-600 dark:text-slate-300 rounded-lg"
                >
                  <ArrowLeft className="w-5 h-5" aria-hidden="true" />
                </button>

                <div className="min-w-0">
                  <h1 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100 truncate">
                    {selectedThread.displayName}
                  </h1>
                  <p className="text-xs text-slate-500 dark:text-slate-400 truncate">
                    <span className="font-mono">{selectedThread.sender}</span>
                    <span aria-hidden="true"> · </span>
                    <span>{selectedThread.isRcsCapable ? 'RCS Encrypted Chat' : formatSenderKindLabel(selectedThread.senderKind)}</span>
                    <span aria-hidden="true"> · </span>
                    <span>
                      {selectedThread.classification.category} ({selectedThread.messages.length}{' '}
                      msg{selectedThread.messages.length === 1 ? '' : 's'})
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
                  onClick={() => onToggleArchive(selectedThread.threadId)}
                  aria-label={
                    selectedThread.isArchived
                      ? `Unarchive ${selectedThread.displayName}`
                      : `Archive ${selectedThread.displayName}`
                  }
                  className="min-w-[36px] min-h-[36px] flex items-center justify-center text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-100 dark:hover:bg-slate-800 rounded-lg transition-colors"
                >
                  {selectedThread.isArchived ? (
                    <ArchiveRestore className="w-4 h-4" aria-hidden="true" />
                  ) : (
                    <Archive className="w-4 h-4" aria-hidden="true" />
                  )}
                </button>

                <button
                  type="button"
                  onClick={() => onToggleBlock(selectedThread.sender)}
                  aria-label={
                    selectedThread.isBlocked
                      ? `Unblock ${selectedThread.displayName}`
                      : `Mark ${selectedThread.displayName} as spam`
                  }
                  className={`min-w-[36px] min-h-[36px] flex items-center justify-center rounded-lg transition-colors ${
                    selectedThread.isBlocked
                      ? 'text-red-600 bg-red-50 dark:text-red-400 dark:bg-red-950/40'
                      : 'text-slate-600 hover:text-red-600 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-red-400 dark:hover:bg-slate-800'
                  }`}
                >
                  {selectedThread.isBlocked ? (
                    <ShieldCheck className="w-4 h-4" aria-hidden="true" />
                  ) : (
                    <ShieldAlert className="w-4 h-4" aria-hidden="true" />
                  )}
                </button>
              </div>
            </div>

            {/* Collapsible Rust Engine Classification Inspector */}
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
                        {formatSenderKindLabel(selectedThread.senderKind)}
                      </span>
                    </p>
                    <p className="text-slate-600 dark:text-slate-400">
                      Transport:{' '}
                      <span className="font-mono text-slate-900 dark:text-slate-200">
                        {selectedThread.isRcsCapable
                          ? 'RCS Universal Profile (SMS fallback)'
                          : `Telephony SMS · SIM ${selectedThread.simSlot + 1}`}
                      </span>
                    </p>
                    <p className="text-slate-600 dark:text-slate-400">
                      Thread consensus ({selectedThread.messages.length} msg
                      {selectedThread.messages.length === 1 ? '' : 's'}):{' '}
                      <span className="font-mono text-slate-900 dark:text-slate-200">
                        {selectedThread.classification.category} ·{' '}
                        {selectedThread.latestMessage.classification.reason}
                      </span>
                    </p>
                  </div>

                  <div className="space-y-2">
                    <h3 className="text-xs font-semibold text-slate-700 dark:text-slate-300">
                      Category Softmax Distribution
                    </h3>
                    <div className="space-y-1.5">
                      {CATEGORY_LIST.map((cat, idx) => {
                        const pct = selectedThread.classification.scores[idx] * 100;
                        const isWinner = selectedThread.classification.category === cat;
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
                                  isWinner
                                    ? 'bg-blue-700 dark:bg-blue-500'
                                    : 'bg-slate-300 dark:bg-slate-600'
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

            {/* Message Feed */}
            <div className="flex-1 overflow-y-auto smooth-scroll-layer px-4 md:px-6 py-5 space-y-4">
              {hiddenCount > 0 && (
                <div className="text-center">
                  <button
                    type="button"
                    onClick={() => setMsgWindow((w) => w + 60)}
                    className="px-3.5 py-1.5 text-xs font-medium text-blue-700 dark:text-blue-400 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-lg hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors"
                  >
                    Load older messages ({hiddenCount.toLocaleString('en-IN')} archived in thread)
                  </button>
                </div>
              )}

              {visibleMessages.map((parsed) => {
                const { sms, otp, transaction, reminder } = parsed;
                const isOut = sms.isOutgoing;
                const isRcs = sms.protocol === 'RCS';
                const expiresAt = otp ? sms.timestampMs + otp.ttlSeconds * 1000 : 0;
                const isDone = completedSet.has(sms.id);
                const isRevealed = Boolean(revealedOtpIds[sms.id]);
                const translation = translateSmsToEnglish(sms.body);
                const showTranslated =
                  translation.needsTranslation &&
                  (translationOverrides[sms.id] ?? autoTranslateToEnglish);

                return (
                  <motion.div
                    key={sms.id}
                    initial={{ opacity: 0, y: 6 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ duration: 0.16, ease: [0.16, 1, 0.3, 1] }}
                    className={`flex flex-col ${isOut ? 'items-end' : 'items-start'}`}
                  >
                    <div
                      className={`max-w-[92%] md:max-w-[78%] rounded-2xl px-4 py-3 border ${
                        isOut
                          ? 'bg-blue-700 text-white border-blue-700 dark:bg-blue-600 dark:border-blue-600 glass-bubble-out'
                          : 'bg-white text-slate-900 border-slate-200 dark:bg-slate-900 dark:text-slate-100 dark:border-slate-800 glass-bubble-in'
                      }`}
                    >
                      {/* RCS Verified Rich Card */}
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
                        dir={!showTranslated && translation.language.isRtl ? 'rtl' : 'ltr'}
                        className="text-sm leading-relaxed whitespace-pre-wrap break-words"
                      >
                        {showTranslated ? translation.translatedText : sms.body}
                      </p>

                      {/* Lightweight Offline Translation Toggle Bar */}
                      {translation.needsTranslation && (
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
                            onClick={() =>
                              setTranslationOverrides((prev) => ({
                                ...prev,
                                [sms.id]: !showTranslated,
                              }))
                            }
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

                      {/* Inline Encrypted OTP Action Card */}
                      {otp && !isOut && (
                        <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-800 flex flex-wrap items-center justify-between gap-3">
                          <div>
                            <div className="flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400">
                              <Lock className="w-3 h-3 text-blue-700 dark:text-blue-400" aria-hidden="true" />
                              <span>
                                AES-256 Vault · {formatCountdown(expiresAt, nowMs)}
                              </span>
                            </div>
                            <div className="mt-0.5 font-mono text-lg font-semibold tracking-wider tabular-nums text-slate-900 dark:text-slate-100">
                              {isRevealed ? otp.code : '•'.repeat(otp.code.length)}
                            </div>
                          </div>
                          <div className="flex items-center gap-2">
                            <button
                              type="button"
                              onClick={() => toggleRevealOtp(sms.id)}
                              aria-label={isRevealed ? `Hide OTP ${sms.id}` : `Reveal OTP ${sms.id}`}
                              className="inline-flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors"
                            >
                              {isRevealed ? (
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
                              onClick={() => handleCopyCode(sms.id, otp.code)}
                              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors"
                            >
                              {copiedOtpId === sms.id ? (
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
                        </div>
                      )}

                      {/* Inline Extracted Transaction Receipt */}
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
                              Merchant:{' '}
                              <span className="font-medium">{transaction.merchantOrVpa}</span>
                              {transaction.accountLast4 && ` · ••${transaction.accountLast4}`}
                            </div>
                          )}
                        </div>
                      )}

                      {/* Inline Extracted Reminder Action */}
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

                    {/* Message meta & quick actions */}
                    <div className="mt-1 px-1 flex items-center gap-2 text-[11px] text-slate-500 dark:text-slate-400">
                      <span className="font-mono tabular-nums">
                        {formatFullDateTime(sms.timestampMs)}
                      </span>
                      <span aria-hidden="true">·</span>
                      <span>{isRcs ? 'RCS' : `SIM ${sms.simSlot + 1}`}</span>
                      {isOut && isRcs && (
                        <span className="inline-flex items-center gap-0.5 text-blue-700 dark:text-blue-400">
                          <CheckCheck className="w-3.5 h-3.5" aria-hidden="true" />
                          <span>{sms.rcsStatus === 'read' ? 'Read' : 'Delivered'}</span>
                        </span>
                      )}
                      <button
                        type="button"
                        onClick={() => onToggleStar(sms.id)}
                        aria-label={
                          sms.isStarred ? `Unstar message ${sms.id}` : `Star message ${sms.id}`
                        }
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
                        onClick={() => onDeleteMessage(sms.id)}
                        aria-label={`Delete message ${sms.id}`}
                        className="p-1 rounded text-slate-400 hover:text-red-600 transition-colors"
                      >
                        <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                      </button>
                    </div>
                  </motion.div>
                );
              })}
            </div>

            {/* Dual-SIM + RCS Compose Reply Bar */}
            <form
              onSubmit={handleSend}
              className="bg-white dark:bg-slate-900 glass-chrome border-t border-slate-200 dark:border-slate-800 px-4 md:px-6 py-3 mb-16 md:mb-0 flex items-center gap-2"
            >
              <button
                type="button"
                onClick={() => {
                  if (replyProtocol === 'RCS') {
                    setReplyProtocol('SMS');
                    setReplySim(0);
                  } else if (replySim === 0) {
                    setReplySim(1);
                  } else if (selectedThread.isRcsCapable) {
                    setReplyProtocol('RCS');
                  } else {
                    setReplySim(0);
                  }
                }}
                aria-label={`Switch sending channel (currently ${
                  replyProtocol === 'RCS' ? 'RCS Chat' : `SMS SIM ${replySim + 1}`
                })`}
                className={`px-2.5 py-2 text-xs font-mono font-semibold rounded-lg transition-colors whitespace-nowrap shrink-0 min-h-[40px] ${
                  replyProtocol === 'RCS'
                    ? 'bg-blue-50 text-blue-800 dark:bg-blue-950/60 dark:text-blue-300'
                    : 'text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700'
                }`}
              >
                {replyProtocol === 'RCS' ? 'RCS' : `SIM ${replySim + 1}`}
              </button>

              <input
                type="text"
                aria-label={`Reply to ${selectedThread.displayName}`}
                value={replyText}
                onChange={(e) => setReplyText(e.target.value)}
                placeholder={
                  replyProtocol === 'RCS'
                    ? `RCS message to ${selectedThread.displayName}...`
                    : `SMS to ${selectedThread.displayName} via SIM ${replySim + 1}...`
                }
                className="flex-1 min-w-0 px-3.5 py-2 text-sm bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600 min-h-[40px]"
              />

              <button
                type="submit"
                disabled={!replyText.trim()}
                aria-label="Send message reply"
                className="inline-flex items-center justify-center gap-1.5 px-4 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 disabled:opacity-40 disabled:cursor-not-allowed dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors shrink-0 min-h-[40px]"
              >
                <Send className="w-3.5 h-3.5" aria-hidden="true" />
                <span className="hidden sm:inline">Send</span>
              </button>
            </form>
          </>
        )}
      </section>
    </div>
  );
}
