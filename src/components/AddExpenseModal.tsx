import {
  Banknote,
  Calendar,
  Clapperboard,
  GraduationCap,
  Heart,
  LayoutGrid,
  Shield,
  ShoppingCart,
  Utensils,
  X,
} from 'lucide-react';
import { useState, type FormEvent } from 'react';
import {
  MANUAL_CATEGORIES,
  type ManualCategoryId,
  type ManualTransaction,
} from '../engine/personalFinance';
import { useBackLayer } from '../utils/backStack';

interface AddExpenseModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (entry: ManualTransaction) => void;
}

const CATEGORY_ICONS: Record<ManualCategoryId, typeof Banknote> = {
  Cash: Banknote,
  Apps: LayoutGrid,
  Education: GraduationCap,
  Fun: Clapperboard,
  Food: Utensils,
  Grocery: ShoppingCart,
  Health: Heart,
  Insurance: Shield,
};

export function AddExpenseModal({ isOpen, onClose, onSave }: AddExpenseModalProps) {
  const [direction, setDirection] = useState<'Debit' | 'Credit'>('Debit');
  const [amountStr, setAmountStr] = useState('');
  const [dateIso, setDateIso] = useState(() => new Date().toISOString().slice(0, 10));
  const [note, setNote] = useState('');
  const [category, setCategory] = useState<ManualCategoryId>('Food');

  useBackLayer(isOpen, onClose);

  if (!isOpen) return null;

  const parsedAmount = Number(amountStr.replace(/,/g, ''));
  const valid = Number.isFinite(parsedAmount) && parsedAmount > 0;

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!valid) return;
    const ts = Date.parse(`${dateIso}T12:00:00`) || Date.now();
    onSave({
      id: `man-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      direction,
      amount: Math.round(parsedAmount * 100) / 100,
      category,
      note: note.trim() || `${category} ${direction === 'Debit' ? 'expense' : 'income'}`,
      timestampMs: ts,
    });
    setAmountStr('');
    setNote('');
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-end md:items-center justify-center p-0 md:p-4"
      role="presentation"
    >
      <button
        type="button"
        aria-label="Close add expense dialog"
        onClick={onClose}
        className="absolute inset-0 bg-black/50"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Add Expense or Income"
        className="sheet-enter relative w-full md:max-w-md max-h-[92dvh] flex flex-col rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl overflow-hidden"
      >
        <div className="px-5 pt-4 pb-3 flex items-center justify-between border-b border-slate-100 dark:border-slate-800">
          <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
            {direction === 'Debit' ? 'Add Expense' : 'Add Income'}
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

        <form onSubmit={handleSubmit} className="flex-1 overflow-y-auto p-5 space-y-5">
          {/* Expense | Income segmented toggle */}
          <div className="grid grid-cols-2 gap-1 p-1 rounded-2xl bg-slate-100 dark:bg-slate-800">
            {(
              [
                { id: 'Debit', label: 'Expense' },
                { id: 'Credit', label: 'Income' },
              ] as const
            ).map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => setDirection(opt.id)}
                className={`py-2.5 text-sm font-semibold rounded-xl transition-colors ${
                  direction === opt.id
                    ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                    : 'text-slate-500 dark:text-slate-400'
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>

          {/* Hero Amount Box (inspired by Expense Keeper Add Expense screen) */}
          <div className="rounded-3xl border border-slate-200/80 dark:border-slate-800 bg-slate-50/70 dark:bg-slate-950/60 px-5 py-6 text-center">
            <p className="font-mono text-[11px] font-semibold tracking-widest text-slate-400 uppercase">
              Amount
            </p>
            <div className="mt-2 flex items-center justify-center gap-1">
              <span className="font-mono text-3xl font-bold text-slate-400">₹</span>
              <input
                type="number"
                inputMode="decimal"
                step="any"
                min="0"
                value={amountStr}
                onChange={(e) => setAmountStr(e.target.value)}
                placeholder="0"
                aria-label="Amount in Rupees"
                autoFocus
                className="w-40 bg-transparent text-center font-mono text-4xl font-bold tabular-nums text-slate-900 dark:text-slate-100 focus:outline-none placeholder:text-slate-300 dark:placeholder:text-slate-700"
              />
            </div>
          </div>

          {/* Date & Note */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1.5">
                Date
              </label>
              <div className="relative">
                <input
                  type="date"
                  value={dateIso}
                  onChange={(e) => setDateIso(e.target.value)}
                  className="w-full px-3.5 py-2.5 text-sm rounded-2xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                />
                <Calendar
                  className="w-4 h-4 text-slate-400 absolute right-3.5 top-1/2 -translate-y-1/2 pointer-events-none hidden sm:block"
                  aria-hidden="true"
                />
              </div>
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1.5">
                Note (optional)
              </label>
              <input
                type="text"
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="e.g. Auto fare, tea"
                className="w-full px-3.5 py-2.5 text-sm rounded-2xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
              />
            </div>
          </div>

          {/* 8-Category Icon Tile Grid */}
          <div>
            <p className="text-xs font-semibold text-slate-600 dark:text-slate-400 mb-2.5">
              Category
            </p>
            <div className="grid grid-cols-4 gap-2.5">
              {MANUAL_CATEGORIES.map((cat) => {
                const Icon = CATEGORY_ICONS[cat.id];
                const active = category === cat.id;
                return (
                  <button
                    key={cat.id}
                    type="button"
                    onClick={() => setCategory(cat.id)}
                    aria-pressed={active}
                    className={`flex flex-col items-center gap-1.5 p-2 rounded-2xl border transition-all ${
                      active
                        ? 'border-blue-600 dark:border-blue-400 bg-blue-50/60 dark:bg-blue-950/40 scale-[1.02]'
                        : 'border-transparent hover:bg-slate-100/70 dark:hover:bg-slate-800/50'
                    }`}
                  >
                    <span
                      className={`w-11 h-11 rounded-2xl flex items-center justify-center ${cat.tone}`}
                    >
                      <Icon className="w-5 h-5" aria-hidden="true" />
                    </span>
                    <span className="text-[11px] font-medium text-slate-700 dark:text-slate-300">
                      {cat.id}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <button
            type="submit"
            disabled={!valid}
            className="w-full py-3.5 rounded-2xl text-sm font-semibold text-white snipt-accent-bg snipt-accent-on-solid disabled:opacity-35 transition-colors"
          >
            Save {direction === 'Debit' ? 'Expense' : 'Income'}
          </button>
        </form>
      </div>
    </div>
  );
}
