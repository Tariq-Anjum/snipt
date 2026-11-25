/**
 * Personal Finance extensions inspired by Expense Keeper & Goals trackers:
 * - Manual Cash / UPI / Income entries (8-category icon grid)
 * - Editable Transaction Detail metadata (category, subcategory, note, flagged)
 * - Savings Goals with automatic monthly contribution & Indian asset allocation suggestions
 * - EPF balance tracking (privacy-masked by default)
 * - Month / Year / Lifetime analytics aggregation
 */

import type { BudgetCategoryId, FinanceSummary } from './types';

export type ManualCategoryId =
  | 'Cash'
  | 'Apps'
  | 'Education'
  | 'Fun'
  | 'Food'
  | 'Grocery'
  | 'Health'
  | 'Insurance';

export const MANUAL_CATEGORIES: Array<{
  id: ManualCategoryId;
  budgetBucket: BudgetCategoryId;
  tone: string;
}> = [
  { id: 'Cash', budgetBucket: 'General & Other', tone: 'bg-lime-100/80 text-lime-800 dark:bg-lime-950/60 dark:text-lime-300' },
  { id: 'Apps', budgetBucket: 'Bills & Utilities', tone: 'bg-indigo-100/80 text-indigo-800 dark:bg-indigo-950/60 dark:text-indigo-300' },
  { id: 'Education', budgetBucket: 'General & Other', tone: 'bg-teal-100/80 text-teal-800 dark:bg-teal-950/60 dark:text-teal-300' },
  { id: 'Fun', budgetBucket: 'General & Other', tone: 'bg-emerald-100/80 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-300' },
  { id: 'Food', budgetBucket: 'Food & Dining', tone: 'bg-orange-100/80 text-orange-800 dark:bg-orange-950/60 dark:text-orange-300' },
  { id: 'Grocery', budgetBucket: 'Food & Dining', tone: 'bg-green-100/80 text-green-800 dark:bg-green-950/60 dark:text-green-300' },
  { id: 'Health', budgetBucket: 'General & Other', tone: 'bg-rose-100/80 text-rose-800 dark:bg-rose-950/60 dark:text-rose-300' },
  { id: 'Insurance', budgetBucket: 'Bills & Utilities', tone: 'bg-violet-100/80 text-violet-800 dark:bg-violet-950/60 dark:text-violet-300' },
];

export interface ManualTransaction {
  id: string;
  direction: 'Debit' | 'Credit';
  amount: number;
  category: ManualCategoryId;
  note: string;
  timestampMs: number;
}

export interface TransactionOverride {
  category?: string;
  subcategory?: string;
  note?: string;
  flagged?: boolean;
}

export interface SavingsGoal {
  id: string;
  title: string;
  nickname: string;
  targetAmount: number;
  savedAmount: number;
  startDateIso: string; // YYYY-MM-DD
  targetDateIso: string; // YYYY-MM-DD
}

export interface GoalAllocationPlan {
  monthsRemaining: number;
  monthlyContribution: number;
  progressPct: number;
  strategyLabel: string;
  breakdown: Array<{ instrument: string; monthlyAmount: number }>;
}

const MANUAL_KEY = 'snipt-manual-txns-v1';
const OVERRIDE_KEY = 'snipt-txn-overrides-v1';
const GOALS_KEY = 'snipt-savings-goals-v1';
const EPF_KEY = 'snipt-epf-balance-v1';

export function computeGoalPlan(goal: SavingsGoal, nowMs: number = Date.now()): GoalAllocationPlan {
  const start = Date.parse(goal.startDateIso) || nowMs;
  const target = Date.parse(goal.targetDateIso) || nowMs + 365 * 86_400_000;
  const remainingAmt = Math.max(0, goal.targetAmount - goal.savedAmount);
  const refStart = Math.max(nowMs, start);
  const diffMonths = Math.max(1, Math.round((target - refStart) / (30.4375 * 86_400_000)));
  const monthlyContribution = Math.ceil(remainingAmt / diffMonths);
  const progressPct =
    goal.targetAmount > 0 ? Math.min(100, Math.round((goal.savedAmount / goal.targetAmount) * 100)) : 0;

  // Horizon-appropriate Indian asset allocation suggestion (inspired by Indian Goals planners)
  if (diffMonths <= 18) {
    return {
      monthsRemaining: diffMonths,
      monthlyContribution,
      progressPct,
      strategyLabel: 'Savings Bank or Recurring Deposit (Short Horizon)',
      breakdown: [{ instrument: 'Recurring Deposit / Liquid Fund', monthlyAmount: monthlyContribution }],
    };
  }
  if (diffMonths <= 60) {
    const rd = Math.round(monthlyContribution * 0.4);
    const mf = monthlyContribution - rd;
    return {
      monthsRemaining: diffMonths,
      monthlyContribution,
      progressPct,
      strategyLabel: 'Balanced Debt + Index Mutual Funds (Medium Horizon)',
      breakdown: [
        { instrument: 'Recurring Deposit / Short-Term Debt', monthlyAmount: rd },
        { instrument: 'Nifty 50 Index / Hybrid Mutual Funds', monthlyAmount: mf },
      ],
    };
  }
  const nps = Math.round(monthlyContribution * 0.18);
  const ppf = Math.min(12_500, Math.round(monthlyContribution * 0.54));
  const mf = Math.max(0, monthlyContribution - nps - ppf);
  return {
    monthsRemaining: diffMonths,
    monthlyContribution,
    progressPct,
    strategyLabel: 'Long-Term Indian Wealth Plan (NPS + PPF + Equity MF)',
    breakdown: [
      { instrument: 'NPS (Tier 1)', monthlyAmount: nps },
      { instrument: 'PPF', monthlyAmount: ppf },
      { instrument: 'Equity Mutual Funds / Index SIP', monthlyAmount: mf },
    ].filter((b) => b.monthlyAmount > 0),
  };
}

export function defaultSavingsGoals(): SavingsGoal[] {
  return [
    {
      id: 'goal-retirement',
      title: 'Save for Retirement',
      nickname: 'Retirement',
      targetAmount: 1_00_00_000,
      savedAmount: 14_50_000,
      startDateIso: '2024-10-01',
      targetDateIso: '2040-10-01',
    },
    {
      id: 'goal-vacation',
      title: 'Family Holiday',
      nickname: 'Vacation',
      targetAmount: 1_00_000,
      savedAmount: 28_000,
      startDateIso: '2026-09-01',
      targetDateIso: '2027-04-01',
    },
  ];
}

export function loadManualTransactions(): ManualTransaction[] {
  try {
    const raw = window.localStorage.getItem(MANUAL_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as ManualTransaction[];
      if (Array.isArray(parsed)) return parsed;
    }
  } catch {
    // ignore
  }
  return [];
}

export function saveManualTransactions(items: ManualTransaction[]): void {
  try {
    window.localStorage.setItem(MANUAL_KEY, JSON.stringify(items));
  } catch {
    // ignore
  }
}

export function loadTransactionOverrides(): Record<string, TransactionOverride> {
  try {
    const raw = window.localStorage.getItem(OVERRIDE_KEY);
    if (raw) return JSON.parse(raw) as Record<string, TransactionOverride>;
  } catch {
    // ignore
  }
  return {};
}

export function saveTransactionOverrides(map: Record<string, TransactionOverride>): void {
  try {
    window.localStorage.setItem(OVERRIDE_KEY, JSON.stringify(map));
  } catch {
    // ignore
  }
}

export function loadSavingsGoals(): SavingsGoal[] {
  try {
    const raw = window.localStorage.getItem(GOALS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as SavingsGoal[];
      if (Array.isArray(parsed) && parsed.length > 0) return parsed;
    }
  } catch {
    // ignore
  }
  return defaultSavingsGoals();
}

export function saveSavingsGoals(goals: SavingsGoal[]): void {
  try {
    window.localStorage.setItem(GOALS_KEY, JSON.stringify(goals));
  } catch {
    // ignore
  }
}

export function loadEpfBalance(): number {
  try {
    const raw = window.localStorage.getItem(EPF_KEY);
    if (raw) {
      const n = Number(raw);
      if (Number.isFinite(n) && n >= 0) return n;
    }
  } catch {
    // ignore
  }
  return 4_82_640;
}

export function saveEpfBalance(val: number): void {
  try {
    window.localStorage.setItem(EPF_KEY, String(Math.max(0, Math.round(val))));
  } catch {
    // ignore
  }
}

export interface MonthlyBarPoint {
  label: string;
  monthIndex: number;
  income: number;
  spend: number;
}

export interface AnalyticsOverview {
  todaySpend: number;
  weekSpend: number;
  totalIncome: number;
  totalSpend: number;
  netBalance: number;
  monthlyAverageSpend: number;
  creditCardSpend: number;
  series: MonthlyBarPoint[];
}

const MONTH_LABELS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export function computeAnalyticsOverview(
  summary: FinanceSummary,
  manualTxns: ManualTransaction[],
  scope: 'month' | 'year' | 'lifetime',
  nowMs: number
): AnalyticsOverview {
  const now = new Date(nowMs);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const weekAgo = nowMs - 7 * 86_400_000;
  const currentYear = now.getFullYear();
  const currentMonth = now.getMonth();

  const unified = [
    ...summary.transactions.map((t) => ({
      amount: t.txn.amount,
      direction: t.txn.direction,
      paymentMethod: t.txn.paymentMethod,
      timestampMs: t.timestampMs,
    })),
    ...manualTxns.map((m) => ({
      amount: m.amount,
      direction: m.direction,
      paymentMethod: 'Cash / Manual' as const,
      timestampMs: m.timestampMs,
    })),
  ];

  let todaySpend = 0;
  let weekSpend = 0;
  for (const item of unified) {
    if (item.direction === 'Debit') {
      if (item.timestampMs >= startOfToday) todaySpend += item.amount;
      if (item.timestampMs >= weekAgo) weekSpend += item.amount;
    }
  }

  const scoped = unified.filter((item) => {
    const d = new Date(item.timestampMs);
    if (scope === 'month') {
      return d.getFullYear() === currentYear && d.getMonth() === currentMonth;
    }
    if (scope === 'year') {
      return d.getFullYear() === currentYear;
    }
    return true;
  });

  const totalIncome = scoped
    .filter((i) => i.direction === 'Credit')
    .reduce((s, i) => s + i.amount, 0);
  const totalSpend = scoped
    .filter((i) => i.direction === 'Debit')
    .reduce((s, i) => s + i.amount, 0);
  const creditCardSpend = scoped
    .filter((i) => i.direction === 'Debit' && i.paymentMethod === 'Credit Card')
    .reduce((s, i) => s + i.amount, 0);

  // Build 6-month (month view) or 12-month (year/lifetime view) income vs spend bars
  const points: MonthlyBarPoint[] = [];
  if (scope === 'month') {
    for (let offset = 5; offset >= 0; offset--) {
      const d = new Date(currentYear, currentMonth - offset, 1);
      const y = d.getFullYear();
      const m = d.getMonth();
      let inc = 0;
      let spd = 0;
      for (const item of unified) {
        const id = new Date(item.timestampMs);
        if (id.getFullYear() === y && id.getMonth() === m) {
          if (item.direction === 'Credit') inc += item.amount;
          else spd += item.amount;
        }
      }
      points.push({ label: MONTH_LABELS[m], monthIndex: m, income: inc, spend: spd });
    }
  } else {
    for (let m = 0; m < 12; m++) {
      let inc = 0;
      let spd = 0;
      for (const item of unified) {
        const id = new Date(item.timestampMs);
        if (scope === 'lifetime' || id.getFullYear() === currentYear) {
          if (id.getMonth() === m) {
            if (item.direction === 'Credit') inc += item.amount;
            else spd += item.amount;
          }
        }
      }
      points.push({ label: MONTH_LABELS[m].slice(0, 1), monthIndex: m, income: inc, spend: spd });
    }
  }

  const activeMonths = Math.max(1, points.filter((p) => p.spend > 0 || p.income > 0).length);

  return {
    todaySpend,
    weekSpend,
    totalIncome,
    totalSpend,
    netBalance: totalIncome - totalSpend,
    monthlyAverageSpend: Math.round(totalSpend / activeMonths),
    creditCardSpend,
    series: points,
  };
}
