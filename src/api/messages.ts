import { call } from './core';
import type { MessageCursor, MessagesPage, ParsedSms, ThreadRow, TranslationResult } from './types';

export function messagesPage(
  threadId: number,
  before: MessageCursor | null,
  limit = 50
): Promise<MessagesPage> {
  return call('messages_page', { threadId, before, limit });
}

export function markRead(smsId: number): Promise<boolean> {
  return call('mark_read', { smsId });
}

export function starMessage(smsId: number, starred: boolean): Promise<boolean> {
  return call('star', { smsId, starred });
}

export function deleteMessages(ids: number[]): Promise<number> {
  return call('delete_messages', { ids });
}

// ---- translation (cached: the same text is shown many times while scrolling)
const translations = new Map<string, Promise<TranslationResult>>();

export function translateText(text: string): Promise<TranslationResult> {
  let p = translations.get(text);
  if (!p) {
    p = call<TranslationResult>('translate', { text });
    p.catch(() => translations.delete(text));
    if (translations.size > 2000) translations.clear();
    translations.set(text, p);
  }
  return p;
}

// ---- latest message per thread (ThreadRow carries no preview; see report)
const previews = new Map<string, Promise<ParsedSms | null>>();

export function threadPreview(row: ThreadRow): Promise<ParsedSms | null> {
  const key = `${row.threadId}:${row.latestMessageId}`;
  let p = previews.get(key);
  if (!p) {
    p = messagesPage(row.threadId, null, 1).then((r) => r.messages[0] ?? null);
    p.catch(() => previews.delete(key));
    if (previews.size > 1000) previews.clear();
    previews.set(key, p);
  }
  return p;
}

/**
 * Minimal ThreadRow for a thread id (no `get_thread` command yet). Archive and
 * star state are unknown from a single message and default to false.
 */
export async function threadFromMessages(threadId: number): Promise<ThreadRow | null> {
  const { messages } = await messagesPage(threadId, null, 1);
  const m = messages[0];
  if (!m) return null;
  return {
    threadId,
    sender: m.sms.sender,
    displayName: m.sms.contactName || (m.senderKind.type === 'DltHeader' ? m.senderKind.entity : m.sms.sender),
    senderKind: m.senderKind,
    latestTs: m.sms.timestampMs,
    latestMessageId: m.sms.id,
    classification: m.classification,
    unreadCount: 0,
    hasStarred: Boolean(m.sms.isStarred),
    isArchived: false,
    isBlocked: false,
    simSlot: m.sms.simSlot,
    isRcsCapable: m.sms.protocol === 'RCS',
    messageCount: 0,
  };
}
