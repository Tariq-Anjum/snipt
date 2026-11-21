import {
  BUDGET_CATEGORIES,
  type BudgetCategoryId,
  type BudgetLimits,
  type FinanceTransactionItem,
  type PaymentCycleConfig,
  type PaymentMethod,
} from './types';

export const DEFAULT_CYCLE_CONFIG: PaymentCycleConfig = {
  UPI: 1,
  'Credit Card': 15,
  'Savings Account': 1,
};

export const DEFAULT_BUDGET_LIMITS: BudgetLimits = {
  'Food & Dining': 8000,
  'Transport & Travel': 5000,
  'Bills & Utilities': 6000,
  'Shopping & Retail': 7000,
  'General & Other': 4000,
};

export interface CycleWindow {
  startMs: number;
  endMs: number;
  label: string;
}

/**
 * Computes the current active monthly billing cycle window for a given start day (1-28).
 */
export function getCycleWindow(cycleStartDay: number, nowMs: number): CycleWindow {
  const safeDay = Math.min(28, Math.max(1, Math.floor(cycleStartDay)));
  const now = new Date(nowMs);
  const year = now.getFullYear();
  const month = now.getMonth();
  const currentDay = now.getDate();

  let startDate: Date;
  let endDate: Date;

  if (currentDay >= safeDay) {
    startDate = new Date(year, month, safeDay, 0, 0, 0, 0);
    endDate = new Date(year, month + 1, safeDay - 1, 23, 59, 59, 999);
    if (safeDay === 1) {
      endDate = new Date(year, month + 1, 0, 23, 59, 59, 999);
    }
  } else {
    startDate = new Date(year, month - 1, safeDay, 0, 0, 0, 0);
    endDate = new Date(year, month, safeDay - 1, 23, 59, 59, 999);
  }

  const fmt = (d: Date) =>
    d.toLocaleDateString('en-IN', { day: 'numeric', month: 'short' });

  return {
    startMs: startDate.getTime(),
    endMs: endDate.getTime(),
    label: `${fmt(startDate)} – ${fmt(endDate)}`,
  };
}

export function isTimestampInMethodCycle(
  timestampMs: number,
  method: PaymentMethod,
  cycleConfig: PaymentCycleConfig,
  nowMs: number
): boolean {
  const window = getCycleWindow(cycleConfig[method] ?? 1, nowMs);
  return timestampMs >= window.startMs && timestampMs <= window.endMs;
}

const CATEGORY_RULES: Array<{ category: BudgetCategoryId; keywords: string[] }> = [
  {
    category: 'Food & Dining',
    keywords: [
      'swiggy',
      'zomato',
      'zepto',
      'blinkit',
      'bigbasket',
      'blue tokai',
      'starbucks',
      'cafe',
      'coffee',
      'restaurant',
      'dominos',
      'bakery',
      'instamart',
      'eat',
      'food',
    ],
  },
  {
    category: 'Transport & Travel',
    keywords: [
      'uber',
      'ola',
      'rapido',
      'irctc',
      'indigo',
      'air india',
      'vistara',
      'makemytrip',
      'metro',
      'fuel',
      'petrol',
      'hpcl',
      'bpcl',
      'fastag',
    ],
  },
  {
    category: 'Bills & Utilities',
    keywords: [
      'act fibernet',
      'fibernet',
      'airtel',
      'jio',
      'vi ',
      'electricity',
      'bescom',
      'tata power',
      'broadband',
      'gas',
      'water',
      'recharge',
      'insurance',
      'lic',
      'rent',
    ],
  },
  {
    category: 'Shopping & Retail',
    keywords: [
      'amazon',
      'flipkart',
      'myntra',
      'ajio',
      'nykaa',
      'croma',
      'reliance',
      'decathlon',
      'ikea',
      'apple',
      'store',
      'mall',
    ],
  },
];

export function categorizeMerchantExpense(
  merchantOrVpa: string | null,
  body: string
): BudgetCategoryId {
  const text = `${merchantOrVpa ?? ''} ${body}`.toLowerCase();
  for (const rule of CATEGORY_RULES) {
    if (rule.keywords.some((kw) => text.includes(kw))) {
      return rule.category;
    }
  }
  return 'General & Other';
}

export interface BudgetCategoryStat {
  category: BudgetCategoryId;
  limit: number;
  spent: number;
  remaining: number;
  percentUsed: number;
  transactionCount: number;
  status: 'On track' | 'Near limit' | 'Over budget';
  byMethod: Record<PaymentMethod, number>;
}

export interface BudgetPlannerSummary {
  totalBudget: number;
  totalSpent: number;
  totalRemaining: number;
  overallPercentUsed: number;
  overallStatus: 'On track' | 'Near limit' | 'Over budget';
  categories: BudgetCategoryStat[];
}

export function computeBudgetStats(
  transactions: FinanceTransactionItem[],
  limits: BudgetLimits,
  onlyActiveCycle: boolean = true
): BudgetPlannerSummary {
  const spentMap = new Map<
    BudgetCategoryId,
    {
      spent: number;
      count: number;
      byMethod: Record<PaymentMethod, number>;
    }
  >();

  for (const cat of BUDGET_CATEGORIES) {
    spentMap.set(cat, {
      spent: 0,
      count: 0,
      byMethod: { UPI: 0, 'Credit Card': 0, 'Savings Account': 0 },
    });
  }

  for (const item of transactions) {
    if (item.txn.direction !== 'Debit') continue;
    if (onlyActiveCycle && !item.inActiveCycle) continue;
    const bucket = spentMap.get(item.budgetCategory);
    if (!bucket) continue;
    bucket.spent += item.txn.amount;
    bucket.count += 1;
    bucket.byMethod[item.txn.paymentMethod] += item.txn.amount;
  }

  let totalBudget = 0;
  let totalSpent = 0;

  const categories: BudgetCategoryStat[] = BUDGET_CATEGORIES.map((category) => {
    const limit = Math.max(0, limits[category] ?? 0);
    const data = spentMap.get(category)!;
    const spent = data.spent;
    const remaining = limit - spent;
    const percentUsed = limit > 0 ? (spent / limit) * 100 : spent > 0 ? 100 : 0;

    let status: BudgetCategoryStat['status'] = 'On track';
    if (spent > limit) status = 'Over budget';
    else if (percentUsed >= 80) status = 'Near limit';

    totalBudget += limit;
    totalSpent += spent;

    return {
      category,
      limit,
      spent,
      remaining,
      percentUsed,
      transactionCount: data.count,
      status,
      byMethod: data.byMethod,
    };
  });

  const overallPercentUsed =
    totalBudget > 0 ? (totalSpent / totalBudget) * 100 : 0;
  let overallStatus: BudgetPlannerSummary['overallStatus'] = 'On track';
  if (totalSpent > totalBudget) overallStatus = 'Over budget';
  else if (overallPercentUsed >= 80) overallStatus = 'Near limit';

  return {
    totalBudget,
    totalSpent,
    totalRemaining: totalBudget - totalSpent,
    overallPercentUsed,
    overallStatus,
    categories,
  };
}
