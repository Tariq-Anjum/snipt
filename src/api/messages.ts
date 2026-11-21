import { call } from './core';
import type {
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

export function markRead(smsId: number): Promise<boolean> {
  return call('mark_read', { smsId });
}

export function starMessage(smsId: number, starred: boolean): Promise<boolean> {
  return call('star', { smsId, starred });
}

export function deleteMessages(ids: number[]): Promise<number> {
  return call('delete_messages', { ids });
}

/**
 * Store an outgoing SMS as `queued`; the Android telephony plugin sends it and
 * reports `set_send_status`. Rejected where `capabilities().canSend` is false.
 */
export function queueOutgoing(message: OutgoingSms): Promise<OutgoingResult> {
  return call('queue_outgoing', { message });
}

/** Platform receive path (Android receiver, or the dev simulator). */
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
