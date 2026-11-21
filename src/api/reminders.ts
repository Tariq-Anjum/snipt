import { messagesPage } from './messages';
import { threadsPage } from './threads';
import type { ReminderItem, ThreadRow } from './types';

/**
 * Reminders have no Rust command yet (the store has `reminders()` but it is not
 * exposed). Interim: scan the newest Priority + All threads and read reminder
 * fields off their latest messages. Bounded: 8 concurrent calls, <=150 threads.
 */
export async function scanReminders(): Promise<ReminderItem[]> {
  const [pri, all] = await Promise.all([
    threadsPage('Priority', 'all', null, 100),
    threadsPage('All', 'all', null, 100),
  ]);
  const seen = new Set<number>();
  const rows: ThreadRow[] = [];
  for (const t of [...pri.threads, ...all.threads]) {
    if (!seen.has(t.threadId) && !t.isBlocked) {
      seen.add(t.threadId);
      rows.push(t);
    }
  }
  const out: ReminderItem[] = [];
  let i = 0;
  const worker = async () => {
    while (i < rows.length) {
      const row = rows[i++];
      const { messages } = await messagesPage(row.threadId, null, 30);
      for (const m of messages) {
        if (m.reminder) {
          out.push({
            smsId: m.sms.id,
            threadId: row.threadId,
            sender: row.displayName,
            body: m.sms.body,
            timestampMs: m.sms.timestampMs,
            reminder: m.reminder,
          });
        }
      }
    }
  };
  await Promise.all(Array.from({ length: 8 }, worker));
  out.sort((a, b) => b.timestampMs - a.timestampMs);
  return out;
}
