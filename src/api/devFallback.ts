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
  appendOutgoingReply,
  buildThreads,
  collectOtpItems,
  collectReminders,
  computeFinanceSummary,
  filterThreads,
  ingestIncomingSms,
  purgeExpiredOtpMessages,
  type StoreState,
} from '../engine/store';
import { translateSmsToEnglish } from '../engine/translate';
import type { BudgetLimits, ParsedSms, PaymentCycleConfig, ThreadSummary } from '../engine/types';
import type {
  Capabilities,
  InboxCounts,
  InboxFilter,
  MessageCursor,
  MessageView,
  SendStatus,
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
// Dev fallback only: stored but not applied to the TS budget math.
let period: { resetDay: number; manualFromMs: number | null } = { resetDay: 1, manualFromMs: null };
let threadCache: ThreadSummary[] | null = null;
const sendStatus = new Map<number, SendStatus>();

// Browser preview behaves like a phone so the compose bar can be exercised.
const DEV_CAPS: Capabilities = {
  platform: 'web',
  canSend: true,
  readOnly: false,
  defaultSmsRole: false,
  rcs: false,
  mms: false,
  simSlots: 2,
  nativePicker: false,
};

/** Mirror of sms-engine `redact_otp_code` (whole digit runs only). */
function redact(body: string, code: string): string {
  if (!code) return body;
  const mask = '•'.repeat(code.length);
  return body.replace(new RegExp(`(?<![0-9])${code}(?![0-9])`, 'g'), mask);
}

function redactParsed(p: ParsedSms): ParsedSms {
  if (!p.otp) return p;
  return {
    ...p,
    sms: { ...p.sms, body: redact(p.sms.body, p.otp.code) },
    otp: { ...p.otp, code: '•'.repeat(p.otp.code.length) },
  };
}

function preview(p: ParsedSms): string {
  const text = (p.otp ? redact(p.sms.body, p.otp.code) : p.sms.body)
    .split(/\s+/)
    .filter(Boolean)
    .join(' ');
  return [...text].length <= 140 ? text : `${[...text].slice(0, 139).join('')}…`;
}

const view = async (m: ParsedSms): Promise<MessageView> => {
  const st = sendStatus.get(m.sms.id);
  const otpVault = m.otp ? await encryptOtpCode(m.otp.code) : undefined;
  const v: MessageView = { ...redactParsed(m), ...(otpVault ? { otpVault } : {}) };
  return st ? { ...v, sendStatus: st } : v;
};

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
    preview: preview(t.latestMessage),
    previewIsOutgoing: t.latestMessage.sms.isOutgoing,
    latestHasOtp: t.latestMessage.otp !== null,
    latestTransaction: t.latestMessage.transaction,
    latestReminder: t.latestMessage.reminder,
  };
}

function rowFor(threadId: number): ThreadRow | null {
  const t = threads().find((x) => x.threadId === threadId);
  return t ? toRow(t) : null;
}

function counts(): InboxCounts {
  const ts = threads();
  const live = ts.filter((t) => !t.isArchived);
  const cat = (c: string) => live.filter((t) => t.classification.category === c).length;
  return {
    totalMessages: state.messages.length,
    unread: live
      .filter((t) => t.classification.category !== 'Spam')
      .reduce((n, t) => n + t.unreadCount, 0),
    filters: {
      All: live.filter((t) => t.classification.category !== 'Spam').length,
      Personal: cat('Personal'),
      Transactions: cat('Transactions'),
      Promotions: cat('Promotions'),
      Priority: cat('Priority'),
      Spam: cat('Spam'),
      RCS: live.filter((t) => t.isRcsCapable).length,
      Starred: live.filter((t) => t.hasStarred).length,
      Archived: ts.length - live.length,
    },
    capacity: 10_000,
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
        messages: await Promise.all(page.map(view)),
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
      return { deleted: before - state.messages.length, failedIds: [] };
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
    case 'weekly_scan': {
      const scan = runWeeklyUselessScan(state.messages, state.blockedSenders, Date.now());
      const byId = new Map(state.messages.map((m) => [m.id, m]));
      for (const b of scan.buckets) {
        b.samples = b.samples.map((sm) => {
          const raw = byId.get(sm.smsId);
          const otp = raw ? defaultEngine.analyzeMessage(raw).otp : null;
          return otp ? { ...sm, body: redact(sm.body, otp.code) } : sm;
        });
      }
      return scan;
    }
    case 'apply_cleanup': {
      const scan = runWeeklyUselessScan(state.messages, state.blockedSenders, Date.now());
      const del = new Set<number>(
        scan.buckets.filter((b) => a.buckets.includes(b.id)).flatMap((b) => b.messageIds)
      );
      state = { ...state, messages: state.messages.filter((m) => !del.has(m.id)) };
      touch();
      return del.size;
    }
    case 'get_thread':
      return rowFor(a.threadId);
    case 'inbox_counts':
      return counts();
    case 'reminders':
      return collectReminders(threads(), state.completedReminderSmsIds);
    case 'set_reminder_done': {
      const rest = state.completedReminderSmsIds.filter((id) => id !== a.smsId);
      state = { ...state, completedReminderSmsIds: a.done ? [...rest, a.smsId] : rest };
      return null;
    }
    case 'get_settings':
      return { cycleConfig: cycle, budgetLimits: limits, budgetPeriod: period };
    case 'set_budget_period':
      period = { ...a.period };
      return period;
    case 'purge_expired_otps': {
      const { nextState, purgedCount } = purgeExpiredOtpMessages(state, Date.now());
      state = nextState;
      touch();
      return purgedCount;
    }
    case 'ingest_incoming': {
      const { nextState, createdSms } = ingestIncomingSms(state, {
        ...(a.message as Parameters<typeof ingestIncomingSms>[1]),
        timestampMs: a.message.timestampMs ?? Date.now(),
      });
      state = nextState;
      touch();
      const parsed = defaultEngine.analyzeMessage(createdSms);
      const thread = rowFor(createdSms.threadId)!;
      return {
        inserted: true,
        thread,
        notification: {
          notify: !thread.isBlocked && thread.classification.category !== 'Spam',
          title: thread.displayName,
          message: redactParsed(parsed),
          otpVault: parsed.otp ? await encryptOtpCode(parsed.otp.code) : null,
          otpExpiresAtMs: parsed.otp
            ? createdSms.timestampMs + parsed.otp.ttlSeconds * 1000
            : null,
        },
      };
    }
    case 'queue_outgoing': {
      const m = a.message as { threadId?: number; body: string; simSlot: 0 | 1 };
      if (m.threadId === undefined || !m.body.trim()) throw new Error('invalid input');
      const before = state.messages.length;
      state = appendOutgoingReply(state, m.threadId, m.body, m.simSlot, 'SMS');
      if (state.messages.length === before) throw new Error('invalid input: unknown thread');
      touch();
      const raw = state.messages[state.messages.length - 1];
      sendStatus.set(raw.id, 'queued');
      return {
        message: await view(defaultEngine.analyzeMessage(raw)),
        thread: rowFor(raw.threadId),
      };
    }
    case 'otp_items':
      return Promise.all(
        collectOtpItems(threads(), Date.now()).map(async (item) => ({
          vault: await encryptOtpCode(item.otp.code),
          item: {
            ...item,
            body: redact(item.body, item.otp.code),
            otp: { ...item.otp, code: '•'.repeat(item.otp.code.length) },
          },
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
        remappedMessages: 0,
      };
    }
    case 'capabilities':
      return DEV_CAPS;
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
    default:
      throw new Error(`devFallback: unknown command ${cmd}`);
  }
}
