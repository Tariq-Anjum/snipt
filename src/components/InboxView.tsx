import { useVirtualizer } from '@tanstack/react-virtual';
import {
  Database,
  Moon,
  Search,
  Sun,
  X,
} from 'lucide-react';
import { memo, useEffect, useRef } from 'react';
import type { DesignStyle } from './TopBar';
import { ThreadPane } from './ThreadPane';
import type {
  Capabilities,
  Category,
  InboxCounts,
  InboxFilter,
  SimFilter,
  ThreadRow,
} from '../api';
import { useThreadList, useTranslation } from '../hooks/data';
import { formatMoney, formatShortTime, getInitials } from '../utils/format';

interface InboxViewProps {
  version: number;
  activeFilter: InboxFilter;
  onChangeFilter: (filter: InboxFilter) => void;
  simFilter: SimFilter;
  onChangeSimFilter: (sim: SimFilter) => void;
  searchQuery: string;
  debouncedQuery: string;
  onChangeSearch: (q: string) => void;
  selectedThread: ThreadRow | null;
  onSelectThread: (thread: ThreadRow | null) => void;
  onThreadPatched: (patch: Partial<ThreadRow>) => void;
  onChanged: () => void;
  counts: InboxCounts | null;
  caps: Capabilities;
  onCopyOtp: (code: string) => Promise<boolean>;
  completedReminderIds: number[];
  onToggleReminderDone: (smsId: number) => void;
  theme: 'light' | 'dark';
  onToggleTheme: () => void;
  designStyle: DesignStyle;
  glassActive: boolean;
  onChangeDesignStyle: (style: DesignStyle) => void;
  autoTranslateToEnglish: boolean;
  onToggleAutoTranslate: () => void;
  /** Android only (undefined hides the control): FLAG_SECURE on every screen. */
  secureScreen?: boolean;
  onToggleSecureScreen: () => void;
  onLoadSample?: () => void;
  isLoadingSample: boolean;
  onGoToBackup: () => void;
  nowMs: number;
  textScale: number;
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

/** Row height at text scale 1; the text part (~68 px) grows with the scale. */
const ROW_BASE = 24;
const ROW_TEXT = 68;
const rowHeightFor = (scale: number) => Math.round(ROW_BASE + ROW_TEXT * scale);

function FilterCount({ n }: { n: number | undefined }) {
  if (n === undefined) return null;
  return (
    <span className="font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
      {n.toLocaleString('en-IN')}
    </span>
  );
}

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

interface RowProps {
  thread: ThreadRow;
  selected: boolean;
  autoTranslate: boolean;
  nowMs: number;
  rowHeight: number;
  onSelect: (t: ThreadRow) => void;
}

const ThreadListRow = memo(function ThreadListRow({
  thread,
  selected,
  autoTranslate,
  nowMs,
  rowHeight,
  onSelect,
}: RowProps) {
  const tr = useTranslation(thread.preview || null);
  const needs = tr?.needsTranslation ?? false;
  const snippet = autoTranslate && needs && tr ? tr.translatedText : thread.preview;
  const txn = thread.latestTransaction;
  const rem = thread.latestReminder;
  const highlightMeta = thread.latestHasOtp
    ? 'Encrypted OTP'
    : txn
      ? `${txn.paymentMethod} ${txn.direction === 'Debit' ? '-' : '+'}${formatMoney(txn.amount, txn.currency)}`
      : rem
        ? rem.reference || rem.dateText || rem.kind
        : needs && tr
          ? `${tr.language.label} → EN`
          : null;

  return (
    <button
      type="button"
      onClick={() => onSelect(thread)}
      style={{ height: rowHeight }}
      className={`w-full text-left px-4 py-3 flex items-start gap-3 transition-colors border-b border-slate-100 dark:border-slate-800/80 focus-visible:outline-2 focus-visible:outline-inset focus-visible:outline-blue-600 ${
        selected ? 'bg-blue-50/80 dark:bg-blue-950/35' : 'hover:bg-slate-50 dark:hover:bg-slate-800/40'
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
            className={`ts-name truncate ${
              thread.unreadCount > 0
                ? 'font-semibold text-slate-950 dark:text-white'
                : 'font-medium text-slate-800 dark:text-slate-200'
            }`}
          >
            {thread.displayName}
          </span>
          <span className="text-[11px] font-mono tabular-nums text-slate-500 dark:text-slate-400 shrink-0">
            {formatShortTime(thread.latestTs, nowMs)}
          </span>
        </div>

        <p
          className={`mt-0.5 ts-preview line-clamp-2 break-words ${
            thread.unreadCount > 0
              ? 'text-slate-800 dark:text-slate-200 font-medium'
              : 'text-slate-500 dark:text-slate-400'
          }`}
        >
          {thread.previewIsOutgoing ? `You: ${snippet}` : snippet}
        </p>

        <div className="mt-1 flex items-center gap-1.5 text-[11px] text-slate-500 dark:text-slate-400 truncate">
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
});

function ThreadList({
  threads,
  hasMore,
  loadMore,
  selectedId,
  autoTranslate,
  nowMs,
  onSelect,
  scrollRef,
  textScale,
}: {
  threads: ThreadRow[];
  hasMore: boolean;
  loadMore: () => void;
  selectedId: number | null;
  autoTranslate: boolean;
  nowMs: number;
  onSelect: (t: ThreadRow) => void;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  textScale: number;
}) {
  const rowHeight = rowHeightFor(textScale);
  const virt = useVirtualizer({
    count: threads.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 8,
    getItemKey: (i) => threads[i].threadId,
    // React 19: no flushSync re-render on every scroll event (it blocked the
    // main thread mid-fling); fixed-height rows need no synchronous commit.
    useFlushSync: false,
  });
  // Text scale changed: every row height changed.
  useEffect(() => {
    virt.measure();
  }, [rowHeight, virt]);
  const items = virt.getVirtualItems();
  const lastIndex = items.length ? items[items.length - 1].index : -1;

  // Prefetch the next keyset page at 70 % of what is loaded.
  useEffect(() => {
    if (hasMore && lastIndex >= Math.floor(threads.length * 0.7)) loadMore();
  }, [hasMore, lastIndex, threads.length, loadMore]);

  return (
    <div style={{ height: virt.getTotalSize(), position: 'relative', width: '100%' }}>
      {items.map((v) => {
        const t = threads[v.index];
        return (
          <div
            key={v.key}
            className="vrow"
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '100%',
              height: rowHeight,
              transform: `translateY(${v.start}px)`,
            }}
          >
            <ThreadListRow
              thread={t}
              selected={selectedId === t.threadId}
              autoTranslate={autoTranslate}
              nowMs={nowMs}
              rowHeight={rowHeight}
              onSelect={onSelect}
            />
          </div>
        );
      })}
    </div>
  );
}

export function InboxView({
  version,
  activeFilter,
  onChangeFilter,
  simFilter,
  onChangeSimFilter,
  searchQuery,
  debouncedQuery,
  onChangeSearch,
  selectedThread,
  onSelectThread,
  onThreadPatched,
  onChanged,
  counts,
  caps,
  onCopyOtp,
  completedReminderIds,
  onToggleReminderDone,
  theme,
  onToggleTheme,
  designStyle,
  glassActive,
  onChangeDesignStyle,
  autoTranslateToEnglish,
  onToggleAutoTranslate,
  secureScreen,
  onToggleSecureScreen,
  onLoadSample,
  isLoadingSample,
  onGoToBackup,
  nowMs,
  textScale,
}: InboxViewProps) {
  const { threads, hasMore, loading, error, loadMore } = useThreadList(
    activeFilter,
    simFilter,
    debouncedQuery,
    version
  );
  const listRef = useRef<HTMLDivElement>(null);
  const noFilters = !searchQuery && activeFilter === 'All' && simFilter === 'all';

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
                    <FilterCount n={counts?.filters[filter]} />
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
                    <FilterCount n={counts?.filters[filter]} />
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
              Uses the SIM slot recorded on each message.
            </p>
          </div>

          {/* 10,000+ SMS History Capacity Meter */}
          {counts && (
            <div className="pt-3 border-t border-slate-100 dark:border-slate-800 space-y-1.5">
              <div className="px-1 flex items-center justify-between text-xs">
                <span className="font-semibold text-slate-600 dark:text-slate-400">
                  History Storage
                </span>
                <span className="font-mono tabular-nums text-slate-900 dark:text-slate-100 font-medium">
                  {counts.totalMessages.toLocaleString('en-IN')} /{' '}
                  {counts.capacity.toLocaleString('en-IN')}+
                </span>
              </div>
              <div
                role="meter"
                aria-label="Stored SMS history"
                aria-valuemin={0}
                aria-valuemax={counts.capacity}
                aria-valuenow={Math.min(counts.totalMessages, counts.capacity)}
                className="h-1.5 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden"
              >
                <div
                  className="h-full bg-blue-700 dark:bg-blue-500 rounded-full"
                  style={{
                    width: `${Math.min(100, Math.max(4, (counts.totalMessages / counts.capacity) * 100))}%`,
                  }}
                />
              </div>
            </div>
          )}

          {import.meta.env.DEV && onLoadSample && (
            <div className="pt-3 border-t border-slate-100 dark:border-slate-800">
              <button
                type="button"
                onClick={onLoadSample}
                disabled={isLoadingSample}
                className="w-full inline-flex items-center justify-center gap-1.5 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors"
              >
                <Database className="w-3.5 h-3.5" aria-hidden="true" />
                <span>{isLoadingSample ? 'Indexing 10,000...' : 'Load 10,000 sample SMS (dev)'}</span>
              </button>
            </div>
          )}

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

          {secureScreen !== undefined && (
            <div className="pt-3 border-t border-slate-100 dark:border-slate-800 space-y-1">
              <div className="px-1 flex items-center justify-between gap-2">
                <span className="text-xs font-semibold text-slate-600 dark:text-slate-400">
                  Block screenshots
                </span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={secureScreen}
                  aria-label="Block screenshots and screen recording on every Snipt screen"
                  onClick={onToggleSecureScreen}
                  className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors ${
                    secureScreen ? 'bg-blue-700 dark:bg-blue-600' : 'bg-slate-300 dark:bg-slate-700'
                  }`}
                >
                  <span
                    className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                      secureScreen ? 'translate-x-4.5' : 'translate-x-1'
                    }`}
                  />
                </button>
              </div>
              <p className="px-1 text-[11px] text-slate-500 dark:text-slate-400 leading-snug">
                The code vault is always protected
              </p>
            </div>
          )}
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
            {designStyle === 'glass' && !glassActive && (
              <p className="mt-1.5 px-1 text-[11px] text-amber-700 dark:text-amber-400 leading-snug">
                Crisp Solid is on for this session (performance or power saver).
              </p>
            )}
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

          {/* Mobile / Tablet Dual-SIM filter (appearance lives in Settings) */}
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

          </div>
        </div>

        {/* Thread rows (virtualized, keyset-paged) */}
        <div
          ref={listRef}
          data-pinch-text
          className="flex-1 overflow-y-auto smooth-scroll-layer pb-20 md:pb-4"
        >
          {error ? (
            <div className="p-8 text-center text-xs text-red-700 dark:text-red-300" role="alert">
              Could not load conversations: {error}
            </div>
          ) : threads.length === 0 && !loading ? (
            <div className="p-8 text-center">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">
                {noFilters ? 'No messages yet' : 'No matching conversations'}
              </p>
              <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
                {noFilters
                  ? 'Import an encrypted .mysms.enc backup to see your messages here.'
                  : 'Try another keyword or switch SIM filters.'}
              </p>
              {noFilters ? (
                <button
                  type="button"
                  onClick={onGoToBackup}
                  className="mt-4 px-3.5 py-2 text-xs font-semibold text-blue-700 dark:text-blue-400 bg-blue-50 dark:bg-blue-950/50 rounded-lg hover:bg-blue-100 dark:hover:bg-blue-950 transition-colors"
                >
                  Open Vault &amp; Backup
                </button>
              ) : (
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
            <ThreadList
              threads={threads}
              hasMore={hasMore}
              loadMore={loadMore}
              selectedId={selectedThread?.threadId ?? null}
              autoTranslate={autoTranslateToEnglish}
              nowMs={nowMs}
              onSelect={onSelectThread}
              scrollRef={listRef}
              textScale={textScale}
            />
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
              Choose a thread to inspect encrypted OTPs, UPI/Card transactions, reminders, and engine scores.
            </p>
          </div>
        ) : (
          <ThreadPane
            key={selectedThread.threadId}
            thread={selectedThread}
            version={version}
            nowMs={nowMs}
            autoTranslateToEnglish={autoTranslateToEnglish}
            completedReminderIds={completedReminderIds}
            onToggleReminderDone={onToggleReminderDone}
            onCopyOtp={onCopyOtp}
            onBack={() => onSelectThread(null)}
            onChanged={onChanged}
            onThreadPatched={onThreadPatched}
            caps={caps}
          />
        )}
      </section>
    </div>
  );
}
