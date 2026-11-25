import {
  ArrowUpRight,
  Calendar,
  CreditCard,
  FileText,
  Flag,
  MessageSquare,
  Tag,
  X,
} from 'lucide-react';
import { useState } from 'react';
import type { TransactionOverride } from '../engine/personalFinance';
import { BUDGET_CATEGORIES } from '../engine/types';
import { useBackLayer } from '../utils/backStack';
import { formatFullDateTime, formatMoney } from '../utils/format';

export interface InspectableTransaction {
  id: string;
  direction: 'Debit' | 'Credit';
  amount: number;
  currency?: string;
  merchant: string;
  category: string;
  subcategory: string;
  timestampMs: number;
  accountLabel: string;
  sender?: string;
  originalSmsBody?: string;
  threadId?: number;
  isManual?: boolean;
}

interface TransactionDetailModalProps {
  item: InspectableTransaction | null;
  override?: TransactionOverride;
  onClose: () => void;
  onSaveOverride: (id: string, patch: TransactionOverride) => void;
  onOpenThread?: (threadId: number) => void;
}

export function TransactionDetailModal({
  item,
  override,
  onClose,
  onSaveOverride,
  onOpenThread,
}: TransactionDetailModalProps) {
  const [category, setCategory] = useState(override?.category ?? item?.category ?? 'General & Other');
  const [subcategory, setSubcategory] = useState(override?.subcategory ?? item?.subcategory ?? 'Merchant Spend');
  const [note, setNote] = useState(override?.note ?? '');
  const [flagged, setFlagged] = useState(override?.flagged ?? false);

  useBackLayer(Boolean(item), onClose);

  if (!item) return null;

  const commit = (patch: Partial<TransactionOverride>) => {
    const next: TransactionOverride = {
      category,
      subcategory,
      note,
      flagged,
      ...patch,
    };
    onSaveOverride(item.id, next);
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-end md:items-center justify-center p-0 md:p-4"
      role="presentation"
    >
      <button
        type="button"
        aria-label="Close transaction detail"
        onClick={onClose}
        className="absolute inset-0 bg-black/50"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Transaction Details"
        className="sheet-enter relative w-full md:max-w-md max-h-[92dvh] flex flex-col rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl overflow-hidden"
      >
        <div className="px-5 pt-4 pb-3 flex items-center justify-between border-b border-slate-100 dark:border-slate-800">
          <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
            Transaction
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="min-w-[40px] min-h-[40px] flex items-center justify-center rounded-full text-slate-500 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            <X className="w-5 h-5" aria-hidden="true" />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-5 space-y-5">
          {/* Hero Amount Card (inspired by screenshot 6.jpg) */}
          <div className="rounded-3xl border border-slate-200/80 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-950/60 p-5">
            <p className="font-mono text-[11px] font-semibold tracking-wider text-slate-500 uppercase">
              {item.direction}
            </p>
            <p className="mt-1 font-display text-lg font-semibold text-slate-900 dark:text-slate-100">
              {item.merchant}
            </p>
            <p
              className={`mt-1 font-mono text-3xl font-bold tabular-nums ${
                item.direction === 'Debit'
                  ? 'text-slate-900 dark:text-white'
                  : 'text-emerald-700 dark:text-emerald-400'
              }`}
            >
              {item.direction === 'Credit' ? '+' : ''}
              {formatMoney(item.amount, item.currency)}
            </p>
          </div>

          {/* Details Card */}
          <div className="space-y-1.5">
            <p className="px-1 font-mono text-[11px] font-semibold tracking-wider text-slate-500 uppercase">
              Details
            </p>
            <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-white/70 dark:bg-slate-950/50 divide-y divide-slate-100 dark:divide-slate-800">
              <div className="px-4 py-3 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                  <Tag className="w-4 h-4" aria-hidden="true" />
                  Category
                </span>
                <select
                  value={category}
                  onChange={(e) => {
                    setCategory(e.target.value);
                    commit({ category: e.target.value });
                  }}
                  aria-label="Edit transaction category"
                  className="text-xs font-semibold text-slate-900 dark:text-slate-100 bg-transparent text-right focus:outline-none"
                >
                  {BUDGET_CATEGORIES.map((c) => (
                    <option key={c} value={c} className="bg-white dark:bg-slate-900">
                      {c}
                    </option>
                  ))}
                  <option value="Health" className="bg-white dark:bg-slate-900">
                    Health
                  </option>
                  <option value="Investments" className="bg-white dark:bg-slate-900">
                    Investments
                  </option>
                </select>
              </div>

              <div className="px-4 py-3 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                  <Tag className="w-4 h-4" aria-hidden="true" />
                  Subcategory
                </span>
                <input
                  type="text"
                  value={subcategory}
                  onChange={(e) => setSubcategory(e.target.value)}
                  onBlur={() => commit({ subcategory })}
                  aria-label="Edit subcategory"
                  className="text-xs font-semibold text-right text-slate-900 dark:text-slate-100 bg-transparent focus:outline-none w-44"
                />
              </div>

              <div className="px-4 py-3 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                  <Calendar className="w-4 h-4" aria-hidden="true" />
                  Date
                </span>
                <span className="font-mono text-xs font-medium text-slate-900 dark:text-slate-100">
                  {formatFullDateTime(item.timestampMs)}
                </span>
              </div>

              <div className="px-4 py-3 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
                  <CreditCard className="w-4 h-4" aria-hidden="true" />
                  Account
                </span>
                <span className="font-mono text-xs font-semibold text-slate-900 dark:text-slate-100">
                  {item.accountLabel}
                </span>
              </div>

              <div className="px-4 py-3 flex items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400 shrink-0">
                  <FileText className="w-4 h-4" aria-hidden="true" />
                  Note
                </span>
                <input
                  type="text"
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  onBlur={() => commit({ note })}
                  placeholder="Add a note..."
                  aria-label="Transaction note"
                  className="text-xs font-medium text-right text-blue-700 dark:text-blue-400 placeholder:text-blue-600/70 bg-transparent focus:outline-none flex-1 min-w-0"
                />
              </div>
            </div>
          </div>

          {/* Original SMS verbatim box */}
          {item.originalSmsBody && (
            <div className="space-y-1.5">
              <div className="px-1 flex items-center justify-between">
                <p className="font-mono text-[11px] font-semibold tracking-wider text-slate-500 uppercase">
                  Original SMS
                </p>
                {item.threadId !== undefined && onOpenThread && (
                  <button
                    type="button"
                    onClick={() => {
                      onClose();
                      onOpenThread(item.threadId!);
                    }}
                    className="inline-flex items-center gap-1 text-xs font-semibold text-blue-700 dark:text-blue-400 hover:underline"
                  >
                    <span>Open thread</span>
                    <ArrowUpRight className="w-3.5 h-3.5" aria-hidden="true" />
                  </button>
                )}
              </div>
              <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/70 p-4 space-y-2">
                {item.sender && (
                  <p className="inline-flex items-center gap-1.5 font-mono text-xs text-slate-500 dark:text-slate-400">
                    <MessageSquare className="w-3.5 h-3.5" aria-hidden="true" />
                    {item.sender}
                  </p>
                )}
                <p className="text-xs leading-relaxed text-slate-800 dark:text-slate-200">
                  {item.originalSmsBody}
                </p>
              </div>
            </div>
          )}

          {/* Flag as incorrect */}
          <div className="flex items-center justify-center pt-1">
            <button
              type="button"
              onClick={() => {
                const next = !flagged;
                setFlagged(next);
                commit({ flagged: next });
              }}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium transition-colors ${
                flagged
                  ? 'text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/40'
                  : 'text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200'
              }`}
            >
              <Flag className="w-3.5 h-3.5" aria-hidden="true" />
              <span>{flagged ? 'Flagged for parser review' : 'Flag as incorrect'}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
