import { call } from './core';
import type { ReminderItem } from './types';

/** store.ts `collectReminders` order: pending first, then newest. */
export const reminders = (): Promise<ReminderItem[]> => call('reminders');

export const setReminderDone = (smsId: number, done: boolean): Promise<void> =>
  call('set_reminder_done', { smsId, done });
