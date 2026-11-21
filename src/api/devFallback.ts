// DEV-ONLY adapter: implements the Tauri command surface in memory on top of the
// TypeScript engine so the UI can be checked in a plain browser (`npm run dev`).
// It is imported dynamically behind `import.meta.env.DEV` and is absent from
// production bundles.
import { defaultEngine } from '../engine';
import {
  computeBudgetStats,
  DEFAULT_BUDGET_LIMITS,
  DEFAULT_CYCLE_CONFIG,
} from '../engine/budget';
import { runWeeklyUselessScan } from '../engine/cleanup';
import { decryptOtpCode, encryptOtpCode } from '../engine/cryptoVault';
import { generateBulkHistoryMessages } from '../engine/indexedDb';
import { createEncryptedBackup, restoreEncryptedBackup, buildBackupFilename } from '../engine/backup';
import { createSeedMessages } from '../engine/seedData';
import {
  buildThreads,
  collectOtpItems,
  computeFinanceSummary,
  filterThreads,
  ingestIncomingSms,
  type StoreState,
} from '../engine/store';
import { translateSmsToEnglish } from '../engine/translate';
import type { BudgetLimits, PaymentCycleConfig, ThreadSummary } from '../engine/types';
import type {
  InboxFilter,
  MessageCursor,
  ThreadCursor,
  ThreadRow,
} from './types';

type Args = Record<string, any>;

let state: StoreState = {
  messages: createSeedMessages(Date.now()),
  archivedThreadIds: [],
  blockedSenders: [],
  completedReminderSmsIds: [],
};
let cycle: PaymentCycleConfig = { ...DEFAULT_CYCLE_CONFIG };
let limits: BudgetLimits = { ...DEFAULT_BUDGET_LIMITS };
let threadCache: ThreadSummary[] | null = null;

const touch = () => {
  threadCache = null;
};
const threads = (): ThreadSummary[] => (threadCache ??= buildThreads(state));

function toRow(t: ThreadSummary): ThreadRow {
  return {
    threadId: t.threadId,
    sender: t.sender,
    displayName: t.displayName,
    senderKind: t.senderKind,
    latestTs: t.latestMessage.sms.timestampMs,
    latestMessageId: t.latestMessage.sms.id,
    classification: t.classification,
    unreadCount: t.unreadCount,
    hasStarred: t.hasStarred,
    isArchived: t.isArchived,
    isBlocked: t.isBlocked,
    simSlot: t.simSlot,
    isRcsCapable: t.isRcsCapable,
    messageCount: t.messages.length,
  };
}

const ordered = (list: ThreadSummary[]) =>
  [...list].sort(
    (a, b) =>
      b.latestMessage.sms.timestampMs - a.latestMessage.sms.timestampMs ||
      b.threadId - a.threadId
  );

const sim = (v: unknown): 'all' | 0 | 1 => (v === 0 || v === 1 ? v : 'all');

export async function dispatch(cmd: string, a: Args): Promise<unknown> {
  switch (cmd) {
    case 'threads_page': {
      const list = ordered(filterThreads(threads(), a.filter as InboxFilter, sim(a.sim), ''));
      const after = a.after as ThreadCursor | null;
      const start = after
        ? list.findIndex(
            (t) =>
              t.latestMessage.sms.timestampMs < after.latestTs ||
              (t.latestMessage.sms.timestampMs === after.latestTs && t.threadId < after.threadId)
          )
        : 0;
      const from = start < 0 ? list.length : start;
      const limit = a.limit ?? 50;
      const page = list.slice(from, from + limit);
      const last = page[page.length - 1];
      return {
        threads: page.map(toRow),
        next:
          from + limit < list.length && last
            ? { latestTs: last.latestMessage.sms.timestampMs, threadId: last.threadId }
            : null,
      };
    }
    case 'messages_page': {
      const t = threads().find((x) => x.threadId === a.threadId);
      const all = t ? [...t.messages].reverse() : [];
      const before = a.before as MessageCursor | null;
      const from = before
        ? all.findIndex(
            (m) =>
              m.sms.timestampMs < before.tsMs ||
              (m.sms.timestampMs === before.tsMs && m.sms.id < before.id)
          )
        : 0;
      const start = from < 0 ? all.length : from;
      const limit = a.limit ?? 50;
      const page = all.slice(start, start + limit);
      const last = page[page.length - 1];
      return {
        messages: page,
        next:
          start + limit < all.length && last
            ? { tsMs: last.sms.timestampMs, id: last.sms.id }
            : null,
      };
    }
    case 'search':
      return ordered(filterThreads(threads(), a.filter, sim(a.sim), a.query))
        .slice(0, a.limit ?? 100)
        .map(toRow);
    case 'mark_read':
      state = {
        ...state,
        messages: state.messages.map((m) => (m.id === a.smsId ? { ...m, isRead: true } : m)),
      };
      touch();
      return true;
    case 'star':
      state = {
        ...state,
        messages: state.messages.map((m) =>
          m.id === a.smsId ? { ...m, isStarred: a.starred } : m
        ),
      };
      touch();
      return true;
    case 'archive': {
      const rest = state.archivedThreadIds.filter((id) => id !== a.threadId);
      state = { ...state, archivedThreadIds: a.archived ? [...rest, a.threadId] : rest };
      touch();
      return null;
    }
    case 'block': {
      const key = String(a.sender).toLowerCase();
      const rest = state.blockedSenders.filter((s) => s.toLowerCase() !== key);
      state = { ...state, blockedSenders: a.blocked ? [...rest, a.sender] : rest };
      touch();
      return null;
    }
    case 'delete_messages': {
      const del = new Set<number>(a.ids);
      const before = state.messages.length;
      state = { ...state, messages: state.messages.filter((m) => !del.has(m.id)) };
      touch();
      return before - state.messages.length;
    }
    case 'finance_summary':
      return computeFinanceSummary(threads(), cycle, Date.now(), a.onlyActiveCycle);
    case 'budget_stats':
      return computeBudgetStats(
        computeFinanceSummary(threads(), cycle, Date.now(), a.onlyActiveCycle).transactions,
        limits,
        a.onlyActiveCycle
      );
    case 'set_cycle':
      cycle = { ...cycle, ...a.config };
      return cycle;
    case 'set_budget':
      limits = { ...limits, ...a.limits };
      return limits;
    case 'weekly_scan':
      return runWeeklyUselessScan(state.messages, state.blockedSenders, Date.now());
    case 'apply_cleanup': {
      const scan = runWeeklyUselessScan(state.messages, state.blockedSenders, Date.now());
      const del = new Set<number>(
        scan.buckets.filter((b) => a.buckets.includes(b.id)).flatMap((b) => b.messageIds)
      );
      state = { ...state, messages: state.messages.filter((m) => !del.has(m.id)) };
      touch();
      return del.size;
    }
    case 'otp_items':
      return Promise.all(
        collectOtpItems(threads(), Date.now()).map(async (item) => ({
          vault: await encryptOtpCode(item.otp.code),
          item: { ...item, otp: { ...item.otp, code: '•'.repeat(item.otp.code.length) } },
        }))
      );
    case 'vault_reveal':
      return decryptOtpCode(a.payload);
    case 'translate':
      return translateSmsToEnglish(a.text);
    case 'backup_create': {
      const now = Date.now();
      const { envelope, serialized } = await createEncryptedBackup(
        {
          schemaVersion: 1,
          app: 'mySMS',
          createdAtMs: now,
          provider: 'device_picker',
          messages: state.messages,
          archivedThreadIds: state.archivedThreadIds,
          blockedSenders: state.blockedSenders,
          completedReminderSmsIds: state.completedReminderSmsIds,
          cycleConfig: cycle,
          budgetLimits: limits,
          scanScheduleDay: 'Manual scan only',
          autoTranslateToEnglish: true,
        },
        a.passphrase
      );
      return {
        filename: buildBackupFilename(now),
        content: serialized,
        messageCount: envelope.messageCount,
      };
    }
    case 'backup_restore': {
      const p = await restoreEncryptedBackup(a.content, a.passphrase);
      state = {
        messages: p.messages,
        archivedThreadIds: p.archivedThreadIds ?? [],
        blockedSenders: p.blockedSenders ?? [],
        completedReminderSmsIds: p.completedReminderSmsIds ?? [],
      };
      touch();
      return {
        ingest: { inserted: p.messages.length, skippedExisting: 0, threadsTouched: threads().length },
        archivedThreads: state.archivedThreadIds.length,
        blockedSenders: state.blockedSenders.length,
      };
    }
    case 'capabilities':
      return { platform: 'web' };
    case 'import_seed': {
      const before = state.messages.length;
      state = { ...state, messages: generateBulkHistoryMessages(10_000, state.messages, Date.now()) };
      touch();
      return {
        inserted: state.messages.length - before,
        skippedExisting: 0,
        threadsTouched: threads().length,
      };
    }
    case 'dev_ingest': {
      const { nextState, createdSms } = ingestIncomingSms(state, {
        ...(a as Parameters<typeof ingestIncomingSms>[1]),
        timestampMs: Date.now(),
      });
      state = nextState;
      touch();
      return defaultEngine.analyzeMessage(createdSms);
    }
    default:
      throw new Error(`devFallback: unknown command ${cmd}`);
  }
}
