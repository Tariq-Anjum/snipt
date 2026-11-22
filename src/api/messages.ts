import { call } from './core';
import type {
  DeleteOutcome,
  IncomingResult,
  IncomingSms,
  MessageCursor,
  MessagesPage,
  OutgoingResult,
  OutgoingSms,
  TranslationResult,
} from './types';

export function messagesPage(
  threadId: number,
  before: MessageCursor | null,
  limit = 50
): Promise<MessagesPage> {
  return call('messages_page', { threadId, before, limit });
}

/** Rejects (message stays unread) when the system SMS store refused the change. */
export function markRead(smsId: number): Promise<boolean> {
  return call('mark_read', { smsId });
}

export function starMessage(smsId: number, starred: boolean): Promise<boolean> {
  return call('star', { smsId, starred });
}

/** Per-row result: `failedIds` stay visible (provider delete failed; retry). */
export function deleteMessages(ids: number[]): Promise<DeleteOutcome> {
  return call('delete_messages', { ids });
}

/**
 * Store an outgoing SMS and hand it to the radio (Android); the result carries
 * the state persisted after the hand-off. Rejected where `canSend` is false.
 */
export function queueOutgoing(message: OutgoingSms): Promise<OutgoingResult> {
  return call('queue_outgoing', { message });
}

/** Dev simulator only (debug builds); Android receipt is native. */
export function ingestIncoming(
  message: IncomingSms,
  providerId: number | null = null
): Promise<IncomingResult> {
  return call('ingest_incoming', { message, providerId });
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
