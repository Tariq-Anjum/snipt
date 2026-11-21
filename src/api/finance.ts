import { call } from './core';
import type {
  AppSettings,
  BudgetLimits,
  BudgetPlannerSummary,
  CleanupBucketId,
  FinanceSummary,
  PaymentCycleConfig,
  WeeklyScanReport,
} from './types';

export const financeSummary = (onlyActiveCycle: boolean): Promise<FinanceSummary> =>
  call('finance_summary', { onlyActiveCycle });

export const budgetStats = (onlyActiveCycle: boolean): Promise<BudgetPlannerSummary> =>
  call('budget_stats', { onlyActiveCycle });

export const getSettings = (): Promise<AppSettings> => call('get_settings');

export const setCycle = (config: PaymentCycleConfig): Promise<PaymentCycleConfig> =>
  call('set_cycle', { config });

export const setBudget = (limits: BudgetLimits): Promise<BudgetLimits> =>
  call('set_budget', { limits });

export const weeklyScan = (): Promise<WeeklyScanReport> => call('weekly_scan');

export const applyCleanup = (buckets: CleanupBucketId[]): Promise<number> =>
  call('apply_cleanup', { buckets });
