import {
  ArrowUpRight,
  Building2,
  Calendar,
  CreditCard,
  Eye,
  EyeOff,
  PiggyBank,
  Plus,
  RotateCcw,
  Search,
  Target,
  Trash2,
  Undo2,
  X,
} from 'lucide-react';
import { useMemo, useState, type FormEvent } from 'react';
import { getCycleWindow } from '../engine/budget';
import {
  computeAnalyticsOverview,
  computeGoalPlan,
  type ManualTransaction,
  type SavingsGoal,
  type TransactionOverride,
} from '../engine/personalFinance';
import { PAYMENT_METHODS } from '../engine/types';
import type {
  BudgetCategoryId,
  BudgetPeriod,
  BudgetPlannerSummary,
  FinanceSummary,
  PaymentCycleConfig,
  PaymentMethod,
} from '../api';
import { formatFullDateTime, formatInr, formatMoney } from '../utils/format';
import { AddExpenseModal } from './AddExpenseModal';
import {
  TransactionDetailModal,
  type InspectableTransaction,
} from './TransactionDetailModal';

interface FinanceViewProps {
  summary: FinanceSummary;
  budgetStats: BudgetPlannerSummary;
  cycleConfig: PaymentCycleConfig;
  onChangeCycleDay: (method: PaymentMethod, day: number) => void;
  onlyActiveCycle: boolean;
  onToggleOnlyActiveCycle: (val: boolean) => void;
  onChangeBudgetLimit: (category: BudgetCategoryId, limit: number) => void;
  onOpenThread: (threadId: number) => void;
  nowMs: number;
  budgetPeriod: BudgetPeriod;
  onChangeBudgetPeriod: (p: BudgetPeriod) => void;
  onRestoreDefaultLimits: () => void;
  manualTransactions: ManualTransaction[];
  onAddManualTransaction: (entry: ManualTransaction) => void;
  onDeleteManualTransaction: (id: string) => void;
  txnOverrides: Record<string, TransactionOverride>;
  onSaveTxnOverride: (id: string, patch: TransactionOverride) => void;
  goals: SavingsGoal[];
  onAddGoal: (goal: SavingsGoal) => void;
  onUpdateGoalSaved: (id: string, savedAmount: number) => void;
  onDeleteGoal: (id: string) => void;
  epfBalance: number;
  onChangeEpfBalance: (val: number) => void;
}

type FinanceSubTab = 'overview' | 'budgets' | 'goals';
type LedgerFilter =
  | 'all'
  | 'UPI'
  | 'Credit Card'
  | 'Savings Account'
  | 'manual'
  | 'debit'
  | 'credit';

export function FinanceView({
  summary,
  budgetStats,
  cycleConfig,
  onChangeCycleDay,
  onlyActiveCycle,
  onToggleOnlyActiveCycle,
  onChangeBudgetLimit,
  onOpenThread,
  nowMs,
  budgetPeriod,
  onChangeBudgetPeriod,
  onRestoreDefaultLimits,
  manualTransactions,
  onAddManualTransaction,
  onDeleteManualTransaction,
  txnOverrides,
  onSaveTxnOverride,
  goals,
  onAddGoal,
  onUpdateGoalSaved,
  onDeleteGoal,
  epfBalance,
  onChangeEpfBalance,
}: FinanceViewProps) {
  const [subTab, setSubTab] = useState<FinanceSubTab>('overview');
  const [analyticsScope, setAnalyticsScope] = useState<'month' | 'year' | 'lifetime'>('month');
  const [filter, setFilter] = useState<LedgerFilter>('all');
  const [confirm, setConfirm] = useState<'reset' | 'limits' | null>(null);
  const [query, setQuery] = useState('');
  const [epfVisible, setEpfVisible] = useState(false);
  const [addExpenseOpen, setAddExpenseOpen] = useState(false);
  const [inspectItem, setInspectItem] = useState<InspectableTransaction | null>(null);

  // Add Goal modal state
  const [goalModalOpen, setGoalModalOpen] = useState(false);
  const [goalTitle, setGoalTitle] = useState('');
  const [goalNick, setGoalNick] = useState('');
  const [goalTarget, setGoalTarget] = useState('');
  const [goalSaved, setGoalSaved] = useState('0');
  const [goalStart, setGoalStart] = useState(() => new Date().toISOString().slice(0, 10));
  const [goalEnd, setGoalEnd] = useState('2028-12-01');

  const period = getCycleWindow(budgetPeriod.resetDay, nowMs);
  const manualActive =
    budgetPeriod.manualFromMs !== null && budgetPeriod.manualFromMs > period.startMs;

  const analytics = useMemo(
    () => computeAnalyticsOverview(summary, manualTransactions, analyticsScope, nowMs),
    [summary, manualTransactions, analyticsScope, nowMs]
  );

  // Merge bank/UPI SMS transactions + manual cash expenses for unified ledger inspection
  const unifiedLedger = useMemo<InspectableTransaction[]>(() => {
    const smsRows: InspectableTransaction[] = summary.transactions
      .filter((item) => (!onlyActiveCycle ? true : item.inActiveCycle))
      .map((item) => {
        const id = `sms-${item.smsId}`;
        const ov = txnOverrides[id];
        return {
          id,
          direction: item.txn.direction === 'Credit' ? 'Credit' : 'Debit',
          amount: item.txn.amount,
          currency: item.txn.currency,
          merchant: item.txn.merchantOrVpa || item.sender,
          category: ov?.category ?? item.budgetCategory,
          subcategory: ov?.subcategory ?? item.txn.paymentMethod,
          timestampMs: item.timestampMs,
          accountLabel: item.txn.accountLast4
            ? `${item.txn.paymentMethod} · ••${item.txn.accountLast4}`
            : item.txn.paymentMethod,
          sender: item.sender,
          originalSmsBody: item.body,
          threadId: item.threadId,
          isManual: false,
        };
      });

    const manualRows: InspectableTransaction[] = manualTransactions.map((m) => {
      const ov = txnOverrides[m.id];
      return {
        id: m.id,
        direction: m.direction,
        amount: m.amount,
        merchant: m.note,
        category: ov?.category ?? m.category,
        subcategory: ov?.subcategory ?? 'Manual Entry',
        timestampMs: m.timestampMs,
        accountLabel: 'Cash / Manual',
        isManual: true,
      };
    });

    return [...smsRows, ...manualRows].sort((a, b) => b.timestampMs - a.timestampMs);
  }, [summary.transactions, manualTransactions, onlyActiveCycle, txnOverrides]);

  const filteredLedger = useMemo(() => {
    const q = query.trim().toLowerCase();
    return unifiedLedger.filter((item) => {
      if (filter === 'debit' && item.direction !== 'Debit') return false;
      if (filter === 'credit' && item.direction !== 'Credit') return false;
      if (filter === 'manual' && !item.isManual) return false;
      if (
        (filter === 'UPI' || filter === 'Credit Card' || filter === 'Savings Account') &&
        !item.accountLabel.includes(filter)
      ) {
        return false;
      }
      if (q) {
        const hay = `${item.merchant} ${item.category} ${item.subcategory} ${item.accountLabel} ${item.originalSmsBody ?? ''}`.toLowerCase();
        return hay.includes(q);
      }
      return true;
    });
  }, [unifiedLedger, filter, query]);

  const manualSpendTotal = manualTransactions
    .filter((m) => m.direction === 'Debit')
    .reduce((s, m) => s + m.amount, 0);
  const manualIncomeTotal = manualTransactions
    .filter((m) => m.direction === 'Credit')
    .reduce((s, m) => s + m.amount, 0);

  const combinedSpent = budgetStats.totalSpent + manualSpendTotal;
  const combinedBudgetPct =
    budgetStats.totalBudget > 0 ? Math.min(100, (combinedSpent / budgetStats.totalBudget) * 100) : 0;
  const combinedRemaining = budgetStats.totalBudget - combinedSpent;
  const combinedNetBalance = summary.netCashflow + manualIncomeTotal - manualSpendTotal;

  const bankTotalBalance = summary.accountBalances
    .filter((a) => a.paymentMethod !== 'Credit Card')
    .reduce((s, a) => s + a.balance, 0);

  const totalDebitBase = Math.max(summary.totalDebits, 1);
  const upiPct = (summary.upiDebitTotal / totalDebitBase) * 100;
  const ccPct = (summary.creditCardDebitTotal / totalDebitBase) * 100;
  const savingsPct = (summary.savingsDebitTotal / totalDebitBase) * 100;

  const chartMax = Math.max(
    1,
    ...analytics.series.map((pt) => Math.max(pt.income, pt.spend))
  );

  const handleCreateGoal = (e: FormEvent) => {
    e.preventDefault();
    const targetAmt = Number(goalTarget);
    const savedAmt = Number(goalSaved) || 0;
    if (!goalTitle.trim() || !Number.isFinite(targetAmt) || targetAmt <= 0) return;
    onAddGoal({
      id: `goal-${Date.now()}`,
      title: goalTitle.trim(),
      nickname: goalNick.trim() || goalTitle.trim(),
      targetAmount: targetAmt,
      savedAmount: Math.max(0, savedAmt),
      startDateIso: goalStart,
      targetDateIso: goalEnd,
    });
    setGoalTitle('');
    setGoalNick('');
    setGoalTarget('');
    setGoalSaved('0');
    setGoalModalOpen(false);
  };

  return (
    <main className="flex-1 overflow-y-auto smooth-scroll-layer bg-slate-50 dark:bg-slate-950 glass:bg-transparent px-4 md:px-8 py-6 pb-24 md:pb-10">
      <div className="max-w-7xl mx-auto space-y-6">
        {/* Header + Sub-nav + Add Expense CTA */}
        <div className="flex flex-col lg:flex-row lg:items-end justify-between gap-4">
          <div>
            <h1 className="font-display text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
              Finance, Analytics &amp; Savings Goals
            </h1>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              Auto-parsed Indian UPI, Credit Card, Savings &amp; EPF tracker paired with manual cash logging and goal planning.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <div className="inline-flex items-center gap-1 p-1 bg-slate-200/70 dark:bg-slate-800 rounded-xl">
              {(
                [
                  { id: 'overview', label: 'Overview & Analytics' },
                  { id: 'budgets', label: 'Budgets & Cycles' },
                  { id: 'goals', label: 'Savings Goals' },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setSubTab(tab.id)}
                  className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-colors whitespace-nowrap ${
                    subTab === tab.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            <button
              type="button"
              onClick={() => setAddExpenseOpen(true)}
              className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid glass-cta rounded-xl transition-colors whitespace-nowrap"
            >
              <Plus className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Add Expense</span>
            </button>
          </div>
        </div>

        {/* ============================================================
            SUB-TAB 1: OVERVIEW & ANALYTICS (Inspired by uploaded screenshots)
           ============================================================ */}
        {subTab === 'overview' && (
          <>
            {/* Top 2-column Hero: Monthly Budget Progress + Accounts & Cards */}
            <section className="grid grid-cols-1 lg:grid-cols-12 gap-5 items-stretch">
              {/* Hero Budget Progress Card (Screenshot 2.jpg) */}
              <div className="lg:col-span-5 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-6 flex flex-col justify-between space-y-5">
                <div>
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-xs font-semibold text-slate-500 dark:text-slate-400">
                      Spent this period
                    </p>
                    <span className="font-mono text-xs text-slate-500 dark:text-slate-400">
                      {period.label}
                    </span>
                  </div>
                  <p className="mt-2 font-mono text-3xl sm:text-4xl font-bold tabular-nums text-slate-950 dark:text-white">
                    {formatInr(combinedSpent)}
                  </p>
                  <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
                    out of {formatInr(budgetStats.totalBudget)}
                  </p>

                  <div className="mt-4 h-2.5 rounded-full bg-slate-100 dark:bg-slate-800 overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-all ${
                        combinedSpent > budgetStats.totalBudget
                          ? 'bg-red-600'
                          : combinedBudgetPct >= 80
                            ? 'bg-amber-500'
                            : 'bg-emerald-500'
                      }`}
                      style={{ width: `${Math.max(3, combinedBudgetPct)}%` }}
                    />
                  </div>
                  <div className="mt-2 flex items-center justify-between text-xs font-medium">
                    <span className="text-slate-500 dark:text-slate-400">
                      {Math.round(combinedBudgetPct)}% of budget
                    </span>
                    <span className="font-mono font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                      {combinedRemaining >= 0
                        ? `${formatInr(combinedRemaining)} left`
                        : `${formatInr(Math.abs(combinedRemaining))} over`}
                    </span>
                  </div>
                </div>

                <div className="pt-4 border-t border-slate-100 dark:border-slate-800 flex items-center justify-between">
                  <span className="text-xs font-medium text-slate-600 dark:text-slate-400">
                    Net balance this month
                  </span>
                  <span
                    className={`font-mono text-base font-bold tabular-nums ${
                      combinedNetBalance >= 0
                        ? 'text-emerald-700 dark:text-emerald-400'
                        : 'text-red-700 dark:text-red-400'
                    }`}
                  >
                    {combinedNetBalance >= 0 ? '+' : ''}
                    {formatInr(combinedNetBalance)}
                  </span>
                </div>
              </div>

              {/* Accounts & Cards + Today / This Week Spend (Screenshot 2.jpg) */}
              <div className="lg:col-span-7 flex flex-col gap-4">
                <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl divide-y divide-slate-100 dark:divide-slate-800">
                  <div className="px-5 py-3.5 flex items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <span className="w-10 h-10 rounded-2xl bg-blue-50 dark:bg-blue-950/60 text-blue-700 dark:text-blue-300 flex items-center justify-center">
                        <Building2 className="w-5 h-5" aria-hidden="true" />
                      </span>
                      <div>
                        <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          Bank Accounts
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          Total parsed balance
                        </p>
                      </div>
                    </div>
                    <p className="font-mono text-base font-bold tabular-nums text-slate-900 dark:text-slate-100">
                      {formatInr(bankTotalBalance)}
                    </p>
                  </div>

                  <div className="px-5 py-3.5 flex items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <span className="w-10 h-10 rounded-2xl bg-indigo-50 dark:bg-indigo-950/60 text-indigo-700 dark:text-indigo-300 flex items-center justify-center">
                        <CreditCard className="w-5 h-5" aria-hidden="true" />
                      </span>
                      <div>
                        <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          Credit Cards
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          Active cycle spend
                        </p>
                      </div>
                    </div>
                    <p className="font-mono text-base font-bold tabular-nums text-slate-900 dark:text-slate-100">
                      {formatInr(summary.creditCardDebitTotal)}
                    </p>
                  </div>

                  <div className="px-5 py-3.5 flex items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <span className="w-10 h-10 rounded-2xl bg-sky-50 dark:bg-sky-950/60 text-sky-700 dark:text-sky-300 flex items-center justify-center">
                        <PiggyBank className="w-5 h-5" aria-hidden="true" />
                      </span>
                      <div>
                        <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                          EPF Balance
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          Provident Fund · tap eye to reveal
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {epfVisible ? (
                        <input
                          type="number"
                          aria-label="Edit EPF balance"
                          value={epfBalance}
                          onChange={(e) => onChangeEpfBalance(Number(e.target.value) || 0)}
                          className="w-28 text-right font-mono text-sm font-bold tabular-nums px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                        />
                      ) : (
                        <span className="font-mono text-base font-bold tracking-widest text-slate-900 dark:text-slate-100">
                          ••••••
                        </span>
                      )}
                      <button
                        type="button"
                        onClick={() => setEpfVisible((v) => !v)}
                        aria-label={epfVisible ? 'Hide EPF balance' : 'Reveal EPF balance'}
                        className="p-1.5 rounded-lg text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800"
                      >
                        {epfVisible ? (
                          <EyeOff className="w-4 h-4" aria-hidden="true" />
                        ) : (
                          <Eye className="w-4 h-4" aria-hidden="true" />
                        )}
                      </button>
                    </div>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4 flex-1">
                  <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-4 flex flex-col justify-between">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                        Today&apos;s Total Expense
                      </span>
                      <Calendar className="w-4 h-4 text-slate-400" aria-hidden="true" />
                    </div>
                    <p className="mt-2 font-mono text-2xl font-bold tabular-nums text-slate-900 dark:text-slate-100">
                      {formatInr(analytics.todaySpend)}
                    </p>
                  </div>
                  <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-4 flex flex-col justify-between">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                        This Week&apos;s Expense
                      </span>
                      <Calendar className="w-4 h-4 text-slate-400" aria-hidden="true" />
                    </div>
                    <p className="mt-2 font-mono text-2xl font-bold tabular-nums text-slate-900 dark:text-slate-100">
                      {formatInr(analytics.weekSpend)}
                    </p>
                  </div>
                </div>
              </div>
            </section>

            {/* Income vs Spend Analytics Chart (Screenshots 3.png / 4.png / unnamed.png) */}
            <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 sm:p-6 space-y-5">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
                    {analyticsScope === 'month'
                      ? 'Spent this month'
                      : analyticsScope === 'year'
                        ? `Spent in ${new Date(nowMs).getFullYear()}`
                        : 'Lifetime parsed spend'}
                  </p>
                  <p className="mt-1 font-mono text-3xl font-bold tabular-nums text-slate-950 dark:text-white">
                    {formatInr(analytics.totalSpend)}
                  </p>
                </div>

                <div className="inline-flex items-center gap-1 p-1 rounded-xl bg-slate-100 dark:bg-slate-800">
                  {(['month', 'year', 'lifetime'] as const).map((sc) => (
                    <button
                      key={sc}
                      type="button"
                      onClick={() => setAnalyticsScope(sc)}
                      className={`px-3.5 py-1.5 text-xs font-semibold rounded-lg capitalize transition-colors ${
                        analyticsScope === sc
                          ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                          : 'text-slate-500 dark:text-slate-400'
                      }`}
                    >
                      {sc}
                    </button>
                  ))}
                </div>
              </div>

              {/* Legend + Grouped Bar Chart */}
              <div className="space-y-3">
                <div className="flex items-center gap-5 text-xs text-slate-600 dark:text-slate-400">
                  <span className="inline-flex items-center gap-1.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-emerald-500" aria-hidden="true" />
                    Income ({formatInr(analytics.totalIncome)})
                  </span>
                  <span className="inline-flex items-center gap-1.5">
                    <span className="w-2.5 h-2.5 rounded-full bg-blue-600" aria-hidden="true" />
                    Spend ({formatInr(analytics.totalSpend)})
                  </span>
                  <span className="hidden sm:inline text-slate-400">·</span>
                  <span className="hidden sm:inline font-mono tabular-nums">
                    Avg/mo {formatInr(analytics.monthlyAverageSpend)}
                  </span>
                  <span className="hidden sm:inline text-slate-400">·</span>
                  <span className="hidden sm:inline font-mono tabular-nums">
                    Cards {formatInr(analytics.creditCardSpend)}
                  </span>
                </div>

                <div className="h-44 flex items-end justify-between gap-2 pt-4 px-1 border-b border-slate-100 dark:border-slate-800">
                  {analytics.series.map((pt, idx) => {
                    const incH = Math.max(pt.income > 0 ? 8 : 2, (pt.income / chartMax) * 100);
                    const spdH = Math.max(pt.spend > 0 ? 8 : 2, (pt.spend / chartMax) * 100);
                    return (
                      <div
                        key={`${pt.label}-${idx}`}
                        className="flex-1 flex flex-col items-center gap-2 h-full justify-end"
                      >
                        <div className="flex items-end gap-1 h-32 w-full justify-center">
                          <div
                            title={`Income: ${formatInr(pt.income)}`}
                            className="w-2.5 sm:w-3 rounded-t-full bg-emerald-500/90 transition-all"
                            style={{ height: `${incH}%` }}
                          />
                          <div
                            title={`Spend: ${formatInr(pt.spend)}`}
                            className="w-2.5 sm:w-3 rounded-t-full bg-blue-600/90 transition-all"
                            style={{ height: `${spdH}%` }}
                          />
                        </div>
                        <span className="font-mono text-[11px] text-slate-500 dark:text-slate-400 pb-1">
                          {pt.label}
                        </span>
                      </div>
                    );
                  })}
                </div>
              </div>
            </section>

            {/* Recent Expenses & Unified Ledger (Click any row for Transaction Detail Sheet!) */}
            <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl overflow-hidden">
              <div className="p-5 border-b border-slate-100 dark:border-slate-800 flex flex-col md:flex-row md:items-center justify-between gap-3">
                <div>
                  <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                    Recent Transactions
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    Tap any entry to edit category/note or view the original bank SMS.
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <div className="relative">
                    <Search
                      className="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2"
                      aria-hidden="true"
                    />
                    <input
                      type="search"
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      placeholder="Search merchant, UPI..."
                      className="pl-8 pr-3 py-1.5 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                    />
                  </div>
                  <div className="inline-flex items-center gap-1 p-0.5 rounded-xl bg-slate-100 dark:bg-slate-800">
                    {(
                      [
                        { id: 'all', label: 'All' },
                        { id: 'UPI', label: 'UPI' },
                        { id: 'Credit Card', label: 'Cards' },
                        { id: 'manual', label: 'Manual' },
                      ] as const
                    ).map((f) => (
                      <button
                        key={f.id}
                        type="button"
                        onClick={() => setFilter(f.id)}
                        className={`px-2.5 py-1 text-xs font-medium rounded-lg transition-colors ${
                          filter === f.id
                            ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-white shadow-xs'
                            : 'text-slate-500 dark:text-slate-400'
                        }`}
                      >
                        {f.label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>

              <div className="divide-y divide-slate-100 dark:divide-slate-800/80">
                {filteredLedger.length === 0 ? (
                  <div className="p-8 text-center text-sm text-slate-500 dark:text-slate-400">
                    No matching transactions in this view.
                  </div>
                ) : (
                  filteredLedger.slice(0, 40).map((item) => {
                    const ov = txnOverrides[item.id];
                    return (
                      <div
                        key={item.id}
                        className="px-5 py-3.5 flex items-center justify-between gap-3 hover:bg-slate-50/70 dark:hover:bg-slate-800/30 transition-colors"
                      >
                        <button
                          type="button"
                          onClick={() => setInspectItem(item)}
                          className="flex-1 min-w-0 text-left flex items-center justify-between gap-4"
                        >
                          <div className="min-w-0">
                            <p className="text-sm font-semibold text-slate-900 dark:text-slate-100 truncate">
                              {item.merchant}
                              {ov?.flagged && (
                                <span className="ml-2 text-[11px] font-medium text-amber-600 dark:text-amber-400">
                                  · Flagged
                                </span>
                              )}
                            </p>
                            <p className="text-xs text-slate-500 dark:text-slate-400 truncate">
                              {item.category} · {item.accountLabel} · {formatFullDateTime(item.timestampMs)}
                            </p>
                          </div>
                          <div className="text-right shrink-0">
                            <p
                              className={`font-mono text-sm font-semibold tabular-nums ${
                                item.direction === 'Debit'
                                  ? 'text-red-700 dark:text-red-400'
                                  : 'text-emerald-700 dark:text-emerald-400'
                              }`}
                            >
                              {item.direction === 'Debit' ? '-' : '+'}
                              {formatMoney(item.amount, item.currency)}
                            </p>
                            <p className="text-[11px] text-slate-400">{item.subcategory}</p>
                          </div>
                        </button>

                        {item.isManual ? (
                          <button
                            type="button"
                            onClick={() => onDeleteManualTransaction(item.id)}
                            aria-label={`Delete manual entry ${item.merchant}`}
                            className="p-2 text-slate-400 hover:text-red-600 dark:hover:text-red-400 rounded-lg shrink-0"
                          >
                            <Trash2 className="w-4 h-4" aria-hidden="true" />
                          </button>
                        ) : (
                          item.threadId !== undefined && (
                            <button
                              type="button"
                              onClick={() => onOpenThread(item.threadId!)}
                              aria-label={`Open SMS thread for ${item.merchant}`}
                              className="p-2 text-slate-400 hover:text-blue-600 dark:hover:text-blue-400 rounded-lg shrink-0"
                            >
                              <ArrowUpRight className="w-4 h-4" aria-hidden="true" />
                            </button>
                          )
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            </section>
          </>
        )}

        {/* ============================================================
            SUB-TAB 2: BUDGETS & PAYMENT CYCLES (All existing controls preserved!)
           ============================================================ */}
        {subTab === 'budgets' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between flex-wrap gap-3">
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Configure statement cycles for Indian UPI, Credit Cards, Savings Accounts, and monthly category limits.
              </p>
              <div className="inline-flex items-center gap-1 p-1 bg-slate-200/70 dark:bg-slate-800 rounded-lg">
                <button
                  type="button"
                  onClick={() => onToggleOnlyActiveCycle(true)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                    onlyActiveCycle
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  Active Cycles
                </button>
                <button
                  type="button"
                  onClick={() => onToggleOnlyActiveCycle(false)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                    !onlyActiveCycle
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  All History
                </button>
              </div>
            </div>

            {/* Budget period & manual reset */}
            <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                    Budget period &amp; reset
                  </h2>
                  <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">
                    {manualActive && budgetPeriod.manualFromMs !== null
                      ? `Counting since ${formatFullDateTime(budgetPeriod.manualFromMs)} (manual reset).`
                      : `Current budget month: ${period.label}. Resets automatically on day ${budgetPeriod.resetDay}.`}
                  </p>
                </div>
                <label className="inline-flex items-center gap-2 text-xs text-slate-600 dark:text-slate-300">
                  <span>Monthly reset day</span>
                  <select
                    value={budgetPeriod.resetDay}
                    onChange={(e) =>
                      onChangeBudgetPeriod({ ...budgetPeriod, resetDay: Number(e.target.value) })
                    }
                    className="px-2.5 py-1.5 text-xs font-mono bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-lg text-slate-900 dark:text-slate-100"
                  >
                    {Array.from({ length: 28 }, (_, i) => i + 1).map((d) => (
                      <option key={d} value={d}>
                        Day {d}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <div className="mt-4 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => {
                    if (confirm !== 'reset') {
                      setConfirm('reset');
                      return;
                    }
                    setConfirm(null);
                    onChangeBudgetPeriod({ ...budgetPeriod, manualFromMs: Date.now() });
                  }}
                  className={`inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold rounded-xl transition-colors min-h-[40px] ${
                    confirm === 'reset'
                      ? 'bg-red-600 text-white'
                      : 'bg-slate-100 text-slate-800 dark:bg-slate-800 dark:text-slate-100'
                  }`}
                >
                  <RotateCcw className="w-3.5 h-3.5" aria-hidden="true" />
                  {confirm === 'reset' ? 'Tap again to start fresh' : 'Start fresh now'}
                </button>
                {manualActive && (
                  <button
                    type="button"
                    onClick={() => onChangeBudgetPeriod({ ...budgetPeriod, manualFromMs: null })}
                    className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold rounded-xl bg-slate-100 text-slate-800 dark:bg-slate-800 dark:text-slate-100 min-h-[40px]"
                  >
                    <Undo2 className="w-3.5 h-3.5" aria-hidden="true" />
                    Undo reset
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => {
                    if (confirm !== 'limits') {
                      setConfirm('limits');
                      return;
                    }
                    setConfirm(null);
                    onRestoreDefaultLimits();
                  }}
                  className={`inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold rounded-xl transition-colors min-h-[40px] ${
                    confirm === 'limits'
                      ? 'bg-red-600 text-white'
                      : 'bg-slate-100 text-slate-800 dark:bg-slate-800 dark:text-slate-100'
                  }`}
                >
                  {confirm === 'limits' ? 'Tap again to restore' : 'Restore default limits'}
                </button>
              </div>
            </section>

            {/* Monthly Billing Cycle Configuration per Indian Payment Method */}
            <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5">
              <div className="flex items-center justify-between gap-2 mb-4">
                <div>
                  <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                    Monthly Billing Cycle by Payment Method
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                    Match your Indian credit card statement date and savings/UPI monthly reset day.
                  </p>
                </div>
                <Calendar className="w-4 h-4 text-slate-400" aria-hidden="true" />
              </div>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                {PAYMENT_METHODS.map((method) => {
                  const startDay = cycleConfig[method] ?? 1;
                  const cycle = getCycleWindow(startDay, nowMs);
                  const spent =
                    method === 'UPI'
                      ? summary.upiDebitTotal
                      : method === 'Credit Card'
                        ? summary.creditCardDebitTotal
                        : summary.savingsDebitTotal;
                  const pct =
                    method === 'UPI' ? upiPct : method === 'Credit Card' ? ccPct : savingsPct;

                  return (
                    <div
                      key={method}
                      className="p-4 rounded-2xl bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 space-y-3"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                          {method}
                        </span>
                        <span className="font-mono text-sm font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                          {formatInr(spent)}
                        </span>
                      </div>

                      <div className="flex items-center justify-between gap-2 text-xs">
                        <label
                          htmlFor={`cycle-${method}`}
                          className="text-slate-600 dark:text-slate-400"
                        >
                          Cycle starts day
                        </label>
                        <select
                          id={`cycle-${method}`}
                          value={startDay}
                          onChange={(e) =>
                            onChangeCycleDay(method, Number.parseInt(e.target.value, 10))
                          }
                          className="px-2 py-1 font-mono text-xs bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-md text-slate-900 dark:text-slate-100"
                        >
                          {Array.from({ length: 28 }, (_, i) => i + 1).map((day) => (
                            <option key={day} value={day}>
                              Day {day}
                            </option>
                          ))}
                        </select>
                      </div>

                      <div className="text-[11px] text-slate-500 dark:text-slate-400 flex items-center justify-between tabular-nums">
                        <span>Window: {cycle.label}</span>
                        <span className="font-mono">{pct.toFixed(0)}%</span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>

            {/* Built-in Category Budget Planner */}
            <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 space-y-5">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
                    Category Budget Planner
                  </h2>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    Auto-fed from SMS alerts. Adjust any monthly limit inline.
                  </p>
                </div>
                <span className="font-mono text-xs tabular-nums text-slate-600 dark:text-slate-300">
                  Remaining:{' '}
                  <strong className="text-emerald-700 dark:text-emerald-400">
                    {formatInr(Math.max(0, budgetStats.totalRemaining))}
                  </strong>
                </span>
              </div>

              <div className="space-y-4">
                {budgetStats.categories.map((cat) => {
                  const pct = Math.min(100, cat.percentUsed);
                  const barColor =
                    cat.status === 'Over budget'
                      ? 'bg-red-600'
                      : cat.status === 'Near limit'
                        ? 'bg-amber-500'
                        : 'bg-blue-600';
                  return (
                    <div key={cat.category} className="space-y-1.5">
                      <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <div>
                          <span className="font-semibold text-slate-900 dark:text-slate-100">
                            {cat.category}
                          </span>
                          <span className="ml-2 text-slate-500 dark:text-slate-400">
                            {cat.transactionCount} txns · {cat.status}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 font-mono tabular-nums">
                          <span className="font-semibold text-slate-900 dark:text-slate-100">
                            {formatInr(cat.spent)}
                          </span>
                          <span className="text-slate-400">/</span>
                          <input
                            type="number"
                            min={500}
                            step={500}
                            aria-label={`Monthly budget limit for ${cat.category}`}
                            value={cat.limit}
                            onChange={(e) => {
                              const v = Number(e.target.value);
                              if (Number.isFinite(v) && v >= 0) onChangeBudgetLimit(cat.category, v);
                            }}
                            className="w-24 text-right px-2 py-1 text-xs rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                          />
                        </div>
                      </div>
                      <div className="h-2 rounded-full bg-slate-100 dark:bg-slate-800 overflow-hidden">
                        <div className={`h-full rounded-full ${barColor}`} style={{ width: `${pct}%` }} />
                      </div>
                      <div className="flex items-center justify-between text-[11px] font-mono text-slate-500 dark:text-slate-400 tabular-nums">
                        <span>
                          UPI {formatInr(cat.byMethod.UPI)} · CC {formatInr(cat.byMethod['Credit Card'])} · Savings{' '}
                          {formatInr(cat.byMethod['Savings Account'])}
                        </span>
                        <span>
                          {cat.remaining >= 0
                            ? `${formatInr(cat.remaining)} left`
                            : `${formatInr(Math.abs(cat.remaining))} over`}
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          </div>
        )}

        {/* ============================================================
            SUB-TAB 3: SAVINGS GOALS (Inspired by screenshot 5654.png)
           ============================================================ */}
        {subTab === 'goals' && (
          <div className="space-y-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
                  Savings Goals &amp; Indian Investment Plan
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  Set a target amount and horizon to calculate your monthly contribution across NPS, PPF, Mutual Funds, or RD.
                </p>
              </div>
              <button
                type="button"
                onClick={() => setGoalModalOpen(true)}
                className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid rounded-xl"
              >
                <Target className="w-3.5 h-3.5" aria-hidden="true" />
                <span>Add Goal</span>
              </button>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              {goals.map((goal) => {
                const plan = computeGoalPlan(goal, nowMs);
                const radius = 32;
                const circ = 2 * Math.PI * radius;
                const dashOffset = circ - (plan.progressPct / 100) * circ;

                return (
                  <div
                    key={goal.id}
                    className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-3xl p-5 flex flex-col justify-between space-y-4"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div>
                        <p className="font-mono text-[11px] uppercase tracking-wider text-slate-500 dark:text-slate-400">
                          {goal.startDateIso} → {goal.targetDateIso}
                        </p>
                        <h3 className="mt-1 font-display text-lg font-bold text-slate-900 dark:text-slate-100">
                          {goal.nickname}
                        </h3>
                        <p className="text-xs text-slate-500 dark:text-slate-400">{goal.title}</p>
                      </div>
                      <button
                        type="button"
                        onClick={() => onDeleteGoal(goal.id)}
                        aria-label={`Delete goal ${goal.nickname}`}
                        className="p-2 text-slate-400 hover:text-red-600 dark:hover:text-red-400 rounded-lg"
                      >
                        <Trash2 className="w-4 h-4" aria-hidden="true" />
                      </button>
                    </div>

                    <div className="flex items-center gap-5">
                      <div className="relative w-20 h-20 shrink-0">
                        <svg className="w-20 h-20 -rotate-90" viewBox="0 0 80 80">
                          <circle
                            cx="40"
                            cy="40"
                            r={radius}
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="7"
                            className="text-slate-100 dark:text-slate-800"
                          />
                          <circle
                            cx="40"
                            cy="40"
                            r={radius}
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="7"
                            strokeLinecap="round"
                            strokeDasharray={circ}
                            strokeDashoffset={dashOffset}
                            className="text-emerald-600 dark:text-emerald-400"
                          />
                        </svg>
                        <span className="absolute inset-0 flex items-center justify-center font-mono text-xs font-bold tabular-nums text-slate-900 dark:text-white">
                          {plan.progressPct}%
                        </span>
                      </div>

                      <div className="flex-1 space-y-1.5 text-xs">
                        <div className="flex items-center justify-between">
                          <span className="text-slate-500 dark:text-slate-400">Target</span>
                          <span className="font-mono font-bold tabular-nums text-slate-900 dark:text-slate-100">
                            {formatInr(goal.targetAmount)}
                          </span>
                        </div>
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-slate-500 dark:text-slate-400">Saved so far</span>
                          <input
                            type="number"
                            min={0}
                            step={1000}
                            aria-label={`Saved amount for ${goal.nickname}`}
                            value={goal.savedAmount}
                            onChange={(e) =>
                              onUpdateGoalSaved(goal.id, Math.max(0, Number(e.target.value) || 0))
                            }
                            className="w-28 text-right font-mono font-semibold tabular-nums px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                          />
                        </div>
                        <div className="flex items-center justify-between">
                          <span className="text-slate-500 dark:text-slate-400">
                            Planned monthly ({plan.monthsRemaining} mo)
                          </span>
                          <span className="font-mono font-bold tabular-nums snipt-accent-text">
                            {formatInr(plan.monthlyContribution)}/mo
                          </span>
                        </div>
                      </div>
                    </div>

                    {/* Suggested Indian Investment Strategy Box (Screenshot 5654.png) */}
                    <div className="rounded-2xl bg-slate-900 dark:bg-slate-950 text-white p-4 space-y-1.5">
                      <p className="text-xs font-semibold text-blue-300">{plan.strategyLabel}</p>
                      <div className="space-y-1 text-xs">
                        {plan.breakdown.map((b) => (
                          <div key={b.instrument} className="flex items-center justify-between">
                            <span className="text-slate-300">{b.instrument}</span>
                            <span className="font-mono font-semibold tabular-nums">
                              {formatInr(b.monthlyAmount)}/mo
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>

      {/* Add Expense / Income Modal */}
      <AddExpenseModal
        isOpen={addExpenseOpen}
        onClose={() => setAddExpenseOpen(false)}
        onSave={onAddManualTransaction}
      />

      {/* Transaction Detail Inspection Sheet */}
      <TransactionDetailModal
        item={inspectItem}
        override={inspectItem ? txnOverrides[inspectItem.id] : undefined}
        onClose={() => setInspectItem(null)}
        onSaveOverride={onSaveTxnOverride}
        onOpenThread={onOpenThread}
      />

      {/* Add Savings Goal Modal */}
      {goalModalOpen && (
        <div
          className="fixed inset-0 z-[60] flex items-end md:items-center justify-center p-0 md:p-4"
          role="presentation"
        >
          <button
            type="button"
            aria-label="Close add goal modal"
            onClick={() => setGoalModalOpen(false)}
            className="absolute inset-0 bg-black/50"
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Add Savings Goal"
            className="sheet-enter relative w-full md:max-w-md rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl p-5 space-y-4"
          >
            <div className="flex items-center justify-between">
              <h2 className="font-display text-lg font-bold text-slate-900 dark:text-slate-100">
                Add Savings Goal
              </h2>
              <button
                type="button"
                onClick={() => setGoalModalOpen(false)}
                aria-label="Close"
                className="p-2 rounded-full text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800"
              >
                <X className="w-5 h-5" aria-hidden="true" />
              </button>
            </div>
            <form onSubmit={handleCreateGoal} className="space-y-3">
              <div>
                <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                  Savings goal
                </label>
                <input
                  type="text"
                  required
                  value={goalTitle}
                  onChange={(e) => setGoalTitle(e.target.value)}
                  placeholder="e.g. Emergency Fund or Home Downpayment"
                  className="w-full px-3 py-2 text-sm rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                />
              </div>
              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Nickname
                  </label>
                  <input
                    type="text"
                    value={goalNick}
                    onChange={(e) => setGoalNick(e.target.value)}
                    placeholder="e.g. Home Fund"
                    className="w-full px-3 py-2 text-sm rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Target Amount (₹)
                  </label>
                  <input
                    type="number"
                    required
                    min={1000}
                    value={goalTarget}
                    onChange={(e) => setGoalTarget(e.target.value)}
                    placeholder="500000"
                    className="w-full px-3 py-2 text-sm font-mono rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
              </div>
              <div className="grid grid-cols-3 gap-2">
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Saved (₹)
                  </label>
                  <input
                    type="number"
                    min={0}
                    value={goalSaved}
                    onChange={(e) => setGoalSaved(e.target.value)}
                    className="w-full px-2.5 py-2 text-xs font-mono rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Start Date
                  </label>
                  <input
                    type="date"
                    value={goalStart}
                    onChange={(e) => setGoalStart(e.target.value)}
                    className="w-full px-2 py-2 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
                <div>
                  <label className="block text-xs font-semibold text-slate-600 dark:text-slate-400 mb-1">
                    Target Date
                  </label>
                  <input
                    type="date"
                    value={goalEnd}
                    onChange={(e) => setGoalEnd(e.target.value)}
                    className="w-full px-2 py-2 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100"
                  />
                </div>
              </div>
              <button
                type="submit"
                className="w-full py-3 rounded-2xl text-sm font-semibold text-white snipt-accent-bg snipt-accent-on-solid"
              >
                Create Goal &amp; Calculate Plan
              </button>
            </form>
          </div>
        </div>
      )}
    </main>
  );
}
