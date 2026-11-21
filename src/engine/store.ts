import {
  categorizeMerchantExpense,
  DEFAULT_CYCLE_CONFIG,
  isTimestampInMethodCycle,
} from './budget';
import { defaultEngine, type SmsEngine } from './index';
import type {
  Category,
  ExtractedOtp,
  ExtractedReminder,
  FinanceSummary,
  FinanceTransactionItem,
  MessageProtocol,
  ParsedSms,
  PaymentCycleConfig,
  PaymentMethod,
  RawSms,
  ThreadSummary,
} from './types';

export interface StoreState {
  messages: RawSms[];
  archivedThreadIds: number[];
  blockedSenders: string[];
  completedReminderSmsIds: number[];
}

export interface ActiveOtpItem {
  smsId: number;
  threadId: number;
  sender: string;
  body: string;
  timestampMs: number;
  simSlot: 0 | 1;
  otp: ExtractedOtp;
  expiresAtMs: number;
  isExpired: boolean;
}

export interface ReminderItem {
  smsId: number;
  threadId: number;
  sender: string;
  body: string;
  timestampMs: number;
  reminder: ExtractedReminder;
  isDone: boolean;
}

export function buildThreads(
  state: StoreState,
  engine: SmsEngine = defaultEngine
): ThreadSummary[] {
  const archivedSet = new Set(state.archivedThreadIds);
  const blockedSet = new Set(state.blockedSenders.map((s) => s.toLowerCase()));
  const byThread = new Map<number, ParsedSms[]>();

  for (const raw of state.messages) {
    const parsed = engine.analyzeMessage(raw);
    const list = byThread.get(raw.threadId);
    if (list) {
      list.push(parsed);
    } else {
      byThread.set(raw.threadId, [parsed]);
    }
  }

  const summaries: ThreadSummary[] = [];

  for (const [threadId, msgs] of byThread.entries()) {
    msgs.sort((a, b) => a.sms.timestampMs - b.sms.timestampMs);
    const latest = msgs[msgs.length - 1];
    const sender = latest.sms.sender;
    const isBlocked = blockedSet.has(sender.toLowerCase());
    const baseClassification = engine.classifyThread(msgs);
    const classification = isBlocked
      ? {
          ...baseClassification,
          category: 'Spam' as Category,
          confidence: 1.0,
        }
      : baseClassification;

    const contactName = msgs.find((m) => m.sms.contactName)?.sms.contactName;
    const displayName = contactName
      ? contactName
      : latest.senderKind.type === 'DltHeader'
        ? latest.senderKind.entity
        : sender;

    const unreadCount = msgs.filter((m) => !m.sms.isOutgoing && !m.sms.isRead).length;
    const hasStarred = msgs.some((m) => Boolean(m.sms.isStarred));
    const isRcsCapable = msgs.some((m) => m.sms.protocol === 'RCS');

    summaries.push({
      threadId,
      sender,
      displayName,
      senderKind: latest.senderKind,
      messages: msgs,
      latestMessage: latest,
      classification,
      unreadCount,
      hasStarred,
      isArchived: archivedSet.has(threadId),
      isBlocked,
      simSlot: latest.sms.simSlot,
      isRcsCapable,
    });
  }

  summaries.sort(
    (a, b) => b.latestMessage.sms.timestampMs - a.latestMessage.sms.timestampMs
  );

  return summaries;
}

export type InboxFilter =
  | 'All'
  | Category
  | 'RCS'
  | 'Starred'
  | 'Archived';

export function filterThreads(
  threads: ThreadSummary[],
  filter: InboxFilter,
  simFilter: 'all' | 0 | 1,
  searchQuery: string
): ThreadSummary[] {
  const q = searchQuery.trim().toLowerCase();

  return threads.filter((thread) => {
    if (filter === 'Archived') {
      if (!thread.isArchived) return false;
    } else {
      if (thread.isArchived) return false;
      if (filter === 'Starred') {
        if (!thread.hasStarred) return false;
      } else if (filter === 'RCS') {
        if (!thread.isRcsCapable) return false;
      } else if (filter === 'All') {
        if (thread.classification.category === 'Spam') return false;
      } else if (thread.classification.category !== filter) {
        return false;
      }
    }

    if (simFilter !== 'all' && thread.simSlot !== simFilter) {
      return false;
    }

    if (q.length > 0) {
      const tokens = q.split(/\s+/).filter(Boolean);
      const recentWindow =
        thread.messages.length > 60
          ? thread.messages.slice(thread.messages.length - 60)
          : thread.messages;
      const haystack = [
        thread.displayName,
        thread.sender,
        thread.isRcsCapable ? 'rcs' : 'sms',
        ...recentWindow.map((m) =>
          [
            m.sms.body,
            m.otp?.code ?? '',
            m.transaction?.merchantOrVpa ?? '',
            m.transaction?.paymentMethod ?? '',
            m.transaction?.referenceId ?? '',
            m.reminder?.reference ?? '',
          ].join(' ')
        ),
      ]
        .join(' ')
        .toLowerCase();

      return tokens.every((tok) => haystack.includes(tok));
    }

    return true;
  });
}

export function computeFinanceSummary(
  threads: ThreadSummary[],
  cycleConfig: PaymentCycleConfig = DEFAULT_CYCLE_CONFIG,
  nowMs: number = Date.now(),
  onlyActiveCycle: boolean = true
): FinanceSummary {
  const transactions: FinanceTransactionItem[] = [];
  const latestBalByAcct = new Map<
    string,
    {
      accountLast4: string;
      bankLabel: string;
      paymentMethod: PaymentMethod;
      balance: number;
      updatedAtMs: number;
    }
  >();
  const merchantTotals = new Map<
    string,
    { total: number; count: number; paymentMethod: PaymentMethod }
  >();

  let totalDebits = 0;
  let totalCredits = 0;
  let upiDebitTotal = 0;
  let creditCardDebitTotal = 0;
  let savingsDebitTotal = 0;

  for (const thread of threads) {
    if (thread.isBlocked) continue;
    for (const msg of thread.messages) {
      if (!msg.transaction) continue;
      const txn = msg.transaction;
      const inActiveCycle = isTimestampInMethodCycle(
        msg.sms.timestampMs,
        txn.paymentMethod,
        cycleConfig,
        nowMs
      );
      const budgetCategory = categorizeMerchantExpense(
        txn.merchantOrVpa,
        msg.sms.body
      );

      transactions.push({
        smsId: msg.sms.id,
        threadId: thread.threadId,
        sender: thread.displayName,
        timestampMs: msg.sms.timestampMs,
        simSlot: msg.sms.simSlot,
        body: msg.sms.body,
        txn,
        budgetCategory,
        inActiveCycle,
      });

      if (!onlyActiveCycle || inActiveCycle) {
        if (txn.direction === 'Debit') {
          totalDebits += txn.amount;
          if (txn.paymentMethod === 'UPI') upiDebitTotal += txn.amount;
          else if (txn.paymentMethod === 'Credit Card') creditCardDebitTotal += txn.amount;
          else savingsDebitTotal += txn.amount;

          const merchantKey = txn.merchantOrVpa || thread.displayName;
          const prev = merchantTotals.get(merchantKey) ?? {
            total: 0,
            count: 0,
            paymentMethod: txn.paymentMethod,
          };
          merchantTotals.set(merchantKey, {
            total: prev.total + txn.amount,
            count: prev.count + 1,
            paymentMethod: txn.paymentMethod,
          });
        } else if (txn.direction === 'Credit') {
          totalCredits += txn.amount;
        }
      }

      if (txn.availableBalance !== null) {
        const acctKey = `${txn.paymentMethod === 'Credit Card' ? 'CC' : 'AC'}-${
          txn.accountLast4 ?? thread.displayName
        }`;
        const existing = latestBalByAcct.get(acctKey);
        if (!existing || msg.sms.timestampMs > existing.updatedAtMs) {
          latestBalByAcct.set(acctKey, {
            accountLast4: txn.accountLast4 ?? 'Main',
            bankLabel: thread.displayName,
            paymentMethod: txn.paymentMethod,
            balance: txn.availableBalance,
            updatedAtMs: msg.sms.timestampMs,
          });
        }
      }
    }
  }

  transactions.sort((a, b) => b.timestampMs - a.timestampMs);

  const visibleTransactions = onlyActiveCycle
    ? transactions.filter((t) => t.inActiveCycle)
    : transactions;

  const topMerchants = Array.from(merchantTotals.entries())
    .map(([merchant, stats]) => ({ merchant, ...stats }))
    .sort((a, b) => b.total - a.total)
    .slice(0, 6);

  const accountBalances = Array.from(latestBalByAcct.values()).sort(
    (a, b) => b.balance - a.balance
  );

  return {
    totalDebits,
    totalCredits,
    netCashflow: totalCredits - totalDebits,
    upiDebitTotal,
    creditCardDebitTotal,
    savingsDebitTotal,
    cardOrBankDebitTotal: creditCardDebitTotal + savingsDebitTotal,
    transactionCount: visibleTransactions.length,
    accountBalances,
    topMerchants,
    transactions,
  };
}

export function collectOtpItems(
  threads: ThreadSummary[],
  nowMs: number
): ActiveOtpItem[] {
  const items: ActiveOtpItem[] = [];
  for (const thread of threads) {
    if (thread.isBlocked || thread.isArchived) continue;
    for (const msg of thread.messages) {
      if (!msg.otp) continue;
      const expiresAtMs = msg.sms.timestampMs + msg.otp.ttlSeconds * 1000;
      items.push({
        smsId: msg.sms.id,
        threadId: thread.threadId,
        sender: thread.displayName,
        body: msg.sms.body,
        timestampMs: msg.sms.timestampMs,
        simSlot: msg.sms.simSlot,
        otp: msg.otp,
        expiresAtMs,
        isExpired: nowMs >= expiresAtMs,
      });
    }
  }
  items.sort((a, b) => b.timestampMs - a.timestampMs);
  return items;
}

export function collectReminders(
  threads: ThreadSummary[],
  completedIds: number[]
): ReminderItem[] {
  const doneSet = new Set(completedIds);
  const items: ReminderItem[] = [];
  for (const thread of threads) {
    if (thread.isBlocked) continue;
    for (const msg of thread.messages) {
      if (!msg.reminder) continue;
      items.push({
        smsId: msg.sms.id,
        threadId: thread.threadId,
        sender: thread.displayName,
        body: msg.sms.body,
        timestampMs: msg.sms.timestampMs,
        reminder: msg.reminder,
        isDone: doneSet.has(msg.sms.id),
      });
    }
  }
  items.sort((a, b) => Number(a.isDone) - Number(b.isDone) || b.timestampMs - a.timestampMs);
  return items;
}

export function purgeExpiredOtpMessages(
  state: StoreState,
  nowMs: number,
  engine: SmsEngine = defaultEngine
): { nextState: StoreState; purgedCount: number } {
  const nextMessages: RawSms[] = [];
  let purgedCount = 0;

  for (const raw of state.messages) {
    const parsed = engine.analyzeMessage(raw);
    if (parsed.otp && !parsed.transaction && !parsed.reminder && !raw.isStarred) {
      const expiresAtMs = raw.timestampMs + parsed.otp.ttlSeconds * 1000;
      if (nowMs >= expiresAtMs) {
        purgedCount += 1;
        continue;
      }
    }
    nextMessages.push(raw);
  }

  return {
    nextState: {
      ...state,
      messages: nextMessages,
    },
    purgedCount,
  };
}

export function deleteMessagesByIds(
  state: StoreState,
  idsToDelete: number[]
): { nextState: StoreState; deletedCount: number } {
  const deleteSet = new Set(idsToDelete);
  const nextMessages = state.messages.filter((m) => !deleteSet.has(m.id));
  return {
    nextState: {
      ...state,
      messages: nextMessages,
    },
    deletedCount: state.messages.length - nextMessages.length,
  };
}

export function markSmsRead(state: StoreState, smsId: number): StoreState {
  return {
    ...state,
    messages: state.messages.map((m) =>
      m.id === smsId ? { ...m, isRead: true } : m
    ),
  };
}

export function ingestIncomingSms(
  state: StoreState,
  input: {
    sender: string;
    contactName?: string;
    body: string;
    isKnownContact: boolean;
    simSlot: 0 | 1;
    protocol?: MessageProtocol;
    timestampMs?: number;
  }
): { nextState: StoreState; createdSms: RawSms } {
  const normalizedSender = input.sender.trim();
  const existingThread = state.messages.find(
    (m) => m.sender.toLowerCase() === normalizedSender.toLowerCase()
  );
  const nextId =
    state.messages.reduce((max, m) => Math.max(max, m.id), 1000) + 1;
  const threadId = existingThread
    ? existingThread.threadId
    : state.messages.reduce((max, m) => Math.max(max, m.threadId), 100) + 1;

  const createdSms: RawSms = {
    id: nextId,
    threadId,
    sender: normalizedSender,
    contactName: input.contactName?.trim() || existingThread?.contactName,
    body: input.body.trim(),
    timestampMs: input.timestampMs ?? Date.now(),
    isKnownContact: input.isKnownContact,
    isOutgoing: false,
    simSlot: input.simSlot,
    isRead: false,
    isStarred: false,
    protocol: input.protocol ?? 'SMS',
  };

  return {
    createdSms,
    nextState: {
      ...state,
      messages: [...state.messages, createdSms],
      archivedThreadIds: state.archivedThreadIds.filter((id) => id !== threadId),
    },
  };
}

export function appendOutgoingReply(
  state: StoreState,
  threadId: number,
  body: string,
  simSlot: 0 | 1,
  protocol: MessageProtocol = 'SMS'
): StoreState {
  const existing = state.messages.find((m) => m.threadId === threadId);
  if (!existing || !body.trim()) return state;

  const nextId =
    state.messages.reduce((max, m) => Math.max(max, m.id), 1000) + 1;

  const outgoing: RawSms = {
    id: nextId,
    threadId,
    sender: existing.sender,
    contactName: existing.contactName,
    body: body.trim(),
    timestampMs: Date.now(),
    isKnownContact: existing.isKnownContact,
    isOutgoing: true,
    simSlot,
    isRead: true,
    isStarred: false,
    protocol,
    rcsStatus: protocol === 'RCS' ? 'delivered' : undefined,
  };

  return {
    ...state,
    messages: [...state.messages, outgoing],
  };
}
