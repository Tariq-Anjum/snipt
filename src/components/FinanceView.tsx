import { ArrowUpRight, Calendar, Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { getCycleWindow } from '../engine/budget';
import { PAYMENT_METHODS } from '../engine/types';
import type {
  BudgetCategoryId,
  BudgetPlannerSummary,
  FinanceSummary,
  PaymentCycleConfig,
  PaymentMethod,
} from '../api';
import { formatFullDateTime, formatInr } from '../utils/format';

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
}

type LedgerFilter =
  | 'all'
  | 'UPI'
  | 'Credit Card'
  | 'Savings Account'
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
}: FinanceViewProps) {
  const [filter, setFilter] = useState<LedgerFilter>('all');
  const [query, setQuery] = useState('');

  const filteredTxns = useMemo(() => {
    const q = query.trim().toLowerCase();
    return summary.transactions.filter((item) => {
      if (onlyActiveCycle && !item.inActiveCycle) return false;
      if (filter === 'debit' && item.txn.direction !== 'Debit') return false;
      if (filter === 'credit' && item.txn.direction !== 'Credit') return false;
      if (
        (filter === 'UPI' || filter === 'Credit Card' || filter === 'Savings Account') &&
        item.txn.paymentMethod !== filter
      ) {
        return false;
      }
      if (q) {
        const hay = [
          item.sender,
          item.txn.merchantOrVpa ?? '',
          item.txn.paymentMethod,
          item.budgetCategory,
          item.txn.accountLast4 ?? '',
          item.txn.referenceId ?? '',
          item.body,
        ]
          .join(' ')
          .toLowerCase();
        return hay.includes(q);
      }
      return true;
    });
  }, [summary.transactions, filter, query, onlyActiveCycle]);

  const totalDebitBase = Math.max(summary.totalDebits, 1);
  const upiPct = (summary.upiDebitTotal / totalDebitBase) * 100;
  const ccPct = (summary.creditCardDebitTotal / totalDebitBase) * 100;
  const savingsPct = (summary.savingsDebitTotal / totalDebitBase) * 100;

  return (
    <main className="flex-1 overflow-y-auto smooth-scroll-layer bg-slate-50 dark:bg-slate-950 glass:bg-transparent px-4 md:px-8 py-6 pb-24 md:pb-10">
      <div className="max-w-7xl mx-auto space-y-6">
        <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-4">
          <div>
            <h1 className="font-display text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
              Finance, Billing Cycles &amp; Budget Planner
            </h1>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              Automatic Indian UPI, Credit Card, and Savings Account tracking with per-method monthly billing cycles.
            </p>
          </div>

          <div className="inline-flex items-center gap-1 p-1 bg-slate-200/70 dark:bg-slate-800 rounded-lg self-start sm:self-auto">
            <button
              type="button"
              onClick={() => onToggleOnlyActiveCycle(true)}
              className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
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
              className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                !onlyActiveCycle
                  ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                  : 'text-slate-600 dark:text-slate-400'
              }`}
            >
              All History
            </button>
          </div>
        </div>

        {/* 4-column KPI strip */}
        <section
          aria-label="Financial summary"
          className="grid grid-cols-2 lg:grid-cols-4 gap-4"
        >
          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-4">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Total Debits
            </p>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-red-700 dark:text-red-400">
              {formatInr(summary.totalDebits)}
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
              UPI · Credit Card · Savings
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-4">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Total Credits
            </p>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-emerald-700 dark:text-emerald-400">
              {formatInr(summary.totalCredits)}
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
              Salary, NEFT &amp; refunds
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-4">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Monthly Budget Used
            </p>
            <p
              className={`mt-1.5 font-mono text-2xl font-semibold tabular-nums ${
                budgetStats.overallStatus === 'Over budget'
                  ? 'text-red-700 dark:text-red-400'
                  : budgetStats.overallStatus === 'Near limit'
                    ? 'text-amber-700 dark:text-amber-400'
                    : 'text-blue-700 dark:text-blue-400'
              }`}
            >
              {budgetStats.overallPercentUsed.toFixed(0)}%
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 tabular-nums">
              {formatInr(budgetStats.totalSpent)} of {formatInr(budgetStats.totalBudget)} ·{' '}
              {budgetStats.overallStatus}
            </p>
          </div>

          <div className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-4">
            <p className="text-xs font-medium text-slate-500 dark:text-slate-400">
              Net Cashflow
            </p>
            <p className="mt-1.5 font-mono text-2xl font-semibold tabular-nums text-slate-900 dark:text-slate-100">
              {summary.netCashflow >= 0 ? '+' : ''}
              {formatInr(summary.netCashflow)}
            </p>
            <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 tabular-nums">
              {summary.transactionCount} alerts in view
            </p>
          </div>
        </section>

        {/* Monthly Billing Cycle Configuration per Indian Payment Method */}
        <section
          aria-label="Payment method monthly billing cycles"
          className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-5"
        >
          <div className="flex flex-wrap items-center justify-between gap-2 mb-4">
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
                method === 'UPI'
                  ? upiPct
                  : method === 'Credit Card'
                    ? ccPct
                    : savingsPct;

              return (
                <div
                  key={method}
                  className="p-4 rounded-xl bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 space-y-3"
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
                      className="px-2 py-1 font-mono text-xs bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 rounded-md text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
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
                    <span className="font-mono">{pct.toFixed(0)}% of debits</span>
                  </div>
                </div>
              );
            })}
          </div>
        </section>

        {/* Built-in Auto-Populated Budget Planner + Account Balances */}
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 items-stretch">
          <section
            aria-label="Automatic budget planner"
            className="lg:col-span-2 bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-5 space-y-4"
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                  Built-in Budget Planner (Auto-Fed from SMS)
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                  Automatically categorizes parsed UPI, Credit Card, and Savings debits. Adjust any monthly limit inline.
                </p>
              </div>
              <div className="text-right text-xs font-mono tabular-nums text-slate-600 dark:text-slate-300">
                Remaining:{' '}
                <span
                  className={
                    budgetStats.totalRemaining < 0
                      ? 'font-semibold text-red-700 dark:text-red-400'
                      : 'font-semibold text-emerald-700 dark:text-emerald-400'
                  }
                >
                  {formatInr(budgetStats.totalRemaining)}
                </span>
              </div>
            </div>

            <div className="divide-y divide-slate-100 dark:divide-slate-800">
              {budgetStats.categories.map((cat) => {
                const barWidth = Math.min(100, Math.max(cat.spent > 0 ? 4 : 0, cat.percentUsed));
                const barColor =
                  cat.status === 'Over budget'
                    ? 'bg-red-600 dark:bg-red-500'
                    : cat.status === 'Near limit'
                      ? 'bg-amber-500'
                      : 'bg-blue-700 dark:bg-blue-500';
                const statusClass =
                  cat.status === 'Over budget'
                    ? 'text-red-700 dark:text-red-400'
                    : cat.status === 'Near limit'
                      ? 'text-amber-700 dark:text-amber-400'
                      : 'text-emerald-700 dark:text-emerald-400';

                return (
                  <div key={cat.category} className="py-3.5 space-y-2">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div>
                        <span className="text-sm font-medium text-slate-900 dark:text-slate-100">
                          {cat.category}
                        </span>
                        <span className="ml-2 text-xs text-slate-500 dark:text-slate-400">
                          {cat.transactionCount} txn{cat.transactionCount === 1 ? '' : 's'} ·{' '}
                          <span className={`font-medium ${statusClass}`}>{cat.status}</span>
                        </span>
                      </div>

                      <div className="flex items-center gap-2 text-xs">
                        <span className="font-mono font-semibold tabular-nums text-slate-900 dark:text-slate-100">
                          {formatInr(cat.spent)}
                        </span>
                        <span className="text-slate-400">/</span>
                        <label htmlFor={`budget-${cat.category}`} className="sr-only">
                          Monthly budget limit for {cat.category}
                        </label>
                        <input
                          id={`budget-${cat.category}`}
                          type="number"
                          min={0}
                          step={500}
                          value={cat.limit}
                          onChange={(e) => {
                            const val = Number.parseInt(e.target.value, 10);
                            onChangeBudgetLimit(
                              cat.category,
                              Number.isFinite(val) ? Math.max(0, val) : 0
                            );
                          }}
                          className="w-24 px-2 py-1 text-right font-mono text-xs tabular-nums bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-md text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
                        />
                      </div>
                    </div>

                    <div className="h-2 bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full ${barColor}`}
                        style={{ width: `${barWidth}%` }}
                      />
                    </div>

                    <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-slate-500 dark:text-slate-400 font-mono tabular-nums">
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

          <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl p-5 flex flex-col justify-between space-y-5">
            <div>
              <h2 className="font-display text-sm font-semibold text-slate-900 dark:text-slate-100">
                Latest Balances &amp; Card Limits
              </h2>
              <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
                Latest parsed balance or available credit card limit per account.
              </p>

              <div className="mt-4 divide-y divide-slate-100 dark:divide-slate-800">
                {summary.accountBalances.length === 0 ? (
                  <p className="py-4 text-xs text-slate-500 dark:text-slate-400">
                    No balance alerts detected yet.
                  </p>
                ) : (
                  summary.accountBalances.map((acct) => (
                    <div
                      key={`${acct.bankLabel}-${acct.paymentMethod}-${acct.accountLast4}`}
                      className="py-3 flex items-center justify-between gap-3"
                    >
                      <div className="min-w-0">
                        <p className="text-sm font-medium text-slate-900 dark:text-slate-100 truncate">
                          {acct.bankLabel} · ••{acct.accountLast4}
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          {acct.paymentMethod} ·{' '}
                          <span className="font-mono tabular-nums">
                            {formatFullDateTime(acct.updatedAtMs)}
                          </span>
                        </p>
                      </div>
                      <span className="font-mono text-sm font-semibold tabular-nums text-slate-900 dark:text-slate-100 shrink-0">
                        {formatInr(acct.balance)}
                      </span>
                    </div>
                  ))
                )}
              </div>
            </div>

            <div className="pt-4 border-t border-slate-100 dark:border-slate-800 space-y-2">
              <div className="flex items-center justify-between text-xs">
                <span className="font-medium text-slate-700 dark:text-slate-300">
                  3-Method Debit Split
                </span>
              </div>
              <div className="h-2.5 w-full bg-slate-100 dark:bg-slate-800 rounded-full overflow-hidden flex">
                <div
                  className="bg-blue-700 dark:bg-blue-500 h-full"
                  style={{ width: `${upiPct}%` }}
                  title={`UPI: ${upiPct.toFixed(0)}%`}
                />
                <div
                  className="bg-amber-500 h-full"
                  style={{ width: `${ccPct}%` }}
                  title={`Credit Card: ${ccPct.toFixed(0)}%`}
                />
                <div
                  className="bg-emerald-600 h-full"
                  style={{ width: `${savingsPct}%` }}
                  title={`Savings Account: ${savingsPct.toFixed(0)}%`}
                />
              </div>
              <div className="flex flex-wrap gap-x-3 gap-y-1 text-[11px] font-mono tabular-nums text-slate-500 dark:text-slate-400">
                <span>UPI {upiPct.toFixed(0)}%</span>
                <span>·</span>
                <span>Credit Card {ccPct.toFixed(0)}%</span>
                <span>·</span>
                <span>Savings {savingsPct.toFixed(0)}%</span>
              </div>
            </div>
          </section>
        </div>

        {/* High-density Transaction Table */}
        <section className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden">
          <div className="p-4 md:p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col lg:flex-row lg:items-center justify-between gap-3">
            <div className="flex flex-wrap items-center gap-1 p-1 bg-slate-100 dark:bg-slate-800 rounded-lg self-start">
              {(
                [
                  { id: 'all', label: 'All' },
                  { id: 'UPI', label: 'UPI' },
                  { id: 'Credit Card', label: 'Credit Card' },
                  { id: 'Savings Account', label: 'Savings Account' },
                  { id: 'debit', label: 'Debits' },
                  { id: 'credit', label: 'Credits' },
                ] as const
              ).map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setFilter(tab.id)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors whitespace-nowrap ${
                    filter === tab.id
                      ? 'bg-white dark:bg-slate-900 text-slate-900 dark:text-slate-100 shadow-xs'
                      : 'text-slate-600 dark:text-slate-400'
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            <div className="relative w-full lg:w-64">
              <Search
                className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"
                aria-hidden="true"
              />
              <input
                type="search"
                aria-label="Filter transactions by merchant, payment method, or category"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter merchant, method, ref..."
                className="w-full pl-9 pr-3 py-1.5 text-xs bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-800 rounded-lg text-slate-900 dark:text-slate-100 focus:outline-none focus:ring-2 focus:ring-blue-600"
              />
            </div>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="border-b border-slate-200 dark:border-slate-800 text-xs font-medium text-slate-500 dark:text-slate-400 bg-slate-50/70 dark:bg-slate-950/40">
                  <th className="py-3 px-4">Date</th>
                  <th className="py-3 px-4">Merchant &amp; Category</th>
                  <th className="py-3 px-4">Payment Method</th>
                  <th className="py-3 px-4 text-right">Amount</th>
                  <th className="py-3 px-4 text-right">Balance / Limit</th>
                  <th className="py-3 px-4 text-right">Source</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100 dark:divide-slate-800 text-sm">
                {filteredTxns.length === 0 ? (
                  <tr>
                    <td
                      colSpan={6}
                      className="py-8 px-4 text-center text-xs text-slate-500 dark:text-slate-400"
                    >
                      No transactions match your current cycle or filter.
                    </td>
                  </tr>
                ) : (
                  filteredTxns.slice(0, 60).map((row) => (
                    <tr
                      key={row.smsId}
                      className="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition-colors"
                    >
                      <td className="py-3 px-4 font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400 whitespace-nowrap">
                        {formatFullDateTime(row.timestampMs)}
                      </td>
                      <td className="py-3 px-4">
                        <div className="font-medium text-slate-900 dark:text-slate-100">
                          {row.txn.merchantOrVpa || row.sender}
                        </div>
                        <div className="text-xs text-slate-500 dark:text-slate-400">
                          {row.budgetCategory} · {row.sender}
                        </div>
                      </td>
                      <td className="py-3 px-4 text-xs text-slate-600 dark:text-slate-400">
                        <div className="font-medium text-slate-800 dark:text-slate-200">
                          {row.txn.paymentMethod}
                          {row.txn.accountLast4 ? ` · ••${row.txn.accountLast4}` : ''}
                        </div>
                        {row.txn.referenceId && (
                          <div className="font-mono text-[11px] text-slate-400">
                            Ref {row.txn.referenceId}
                          </div>
                        )}
                      </td>
                      <td
                        className={`py-3 px-4 text-right font-mono font-semibold tabular-nums whitespace-nowrap ${
                          row.txn.direction === 'Credit'
                            ? 'text-emerald-700 dark:text-emerald-400'
                            : 'text-red-700 dark:text-red-400'
                        }`}
                      >
                        {row.txn.direction === 'Credit' ? '+' : '-'}
                        {formatInr(row.txn.amount)}
                      </td>
                      <td className="py-3 px-4 text-right font-mono text-xs tabular-nums text-slate-600 dark:text-slate-300 whitespace-nowrap">
                        {row.txn.availableBalance !== null
                          ? formatInr(row.txn.availableBalance)
                          : '—'}
                      </td>
                      <td className="py-3 px-4 text-right whitespace-nowrap">
                        <button
                          type="button"
                          onClick={() => onOpenThread(row.threadId)}
                          className="inline-flex items-center gap-1 text-xs font-medium text-blue-700 hover:text-blue-800 dark:text-blue-400 dark:hover:text-blue-300"
                        >
                          <span>SMS</span>
                          <ArrowUpRight className="w-3.5 h-3.5" aria-hidden="true" />
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </main>
  );
}
