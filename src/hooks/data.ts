import { useCallback, useEffect, useRef, useState } from 'react';
import {
  messagesPage,
  searchThreads,
  threadPreview,
  threadsPage,
  translateText,
  type InboxFilter,
  type MessageCursor,
  type ParsedSms,
  type SimFilter,
  type ThreadCursor,
  type ThreadRow,
  type TranslationResult,
} from '../api';

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

/** Keyset-paged thread list. `version` bumps refetch in place (same depth). */
export function useThreadList(
  filter: InboxFilter,
  sim: SimFilter,
  query: string,
  version: number
) {
  const [threads, setThreads] = useState<ThreadRow[]>([]);
  const [next, setNext] = useState<ThreadCursor | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const req = useRef(0);
  const lenRef = useRef(0);
  const keyRef = useRef('');
  const moreBusy = useRef(false);
  const q = query.trim();

  useEffect(() => {
    const key = `${filter}|${sim}|${q}`;
    const sameView = keyRef.current === key;
    keyRef.current = key;
    const id = ++req.current;
    const limit = sameView ? clamp(lenRef.current, 50, 500) : 50;
    setLoading(true);
    const p = q
      ? searchThreads(filter, sim, q, 100).then((rows) => ({ threads: rows, next: null }))
      : threadsPage(filter, sim, null, limit);
    p.then((page) => {
      if (id !== req.current) return;
      lenRef.current = page.threads.length;
      setThreads(page.threads);
      setNext(page.next);
      setError(null);
    })
      .catch((e: Error) => id === req.current && setError(e.message))
      .finally(() => id === req.current && setLoading(false));
  }, [filter, sim, q, version]);

  const loadMore = useCallback(() => {
    if (!next || moreBusy.current || q) return;
    moreBusy.current = true;
    const id = req.current;
    threadsPage(filter, sim, next, 50)
      .then((page) => {
        if (id !== req.current) return;
        setThreads((prev) => {
          const seen = new Set(prev.map((t) => t.threadId));
          const merged = [...prev, ...page.threads.filter((t) => !seen.has(t.threadId))];
          lenRef.current = merged.length;
          return merged;
        });
        setNext(page.next);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => {
        moreBusy.current = false;
      });
  }, [next, filter, sim, q]);

  return { threads, hasMore: next !== null, loading, error, loadMore };
}

/** Messages of one thread, ascending (oldest first) for chat display. */
export function useThreadMessages(threadId: number | null, version: number) {
  const [messages, setMessages] = useState<ParsedSms[]>([]);
  const [next, setNext] = useState<MessageCursor | null>(null);
  const [loading, setLoading] = useState(false);
  const [openedAt, setOpenedAt] = useState(0);
  const req = useRef(0);
  const lenRef = useRef(0);
  const tidRef = useRef<number | null>(null);
  const olderBusy = useRef(false);
  const prepended = useRef(0);

  useEffect(() => {
    const same = tidRef.current === threadId;
    tidRef.current = threadId;
    const id = ++req.current;
    if (threadId === null) {
      setMessages([]);
      setNext(null);
      lenRef.current = 0;
      return;
    }
    if (!same) {
      setMessages([]);
      setNext(null);
      lenRef.current = 0;
    }
    setLoading(true);
    messagesPage(threadId, null, same ? clamp(lenRef.current, 50, 500) : 50)
      .then((page) => {
        if (id !== req.current) return;
        const asc = [...page.messages].reverse();
        lenRef.current = asc.length;
        setMessages(asc);
        setNext(page.next);
        if (!same) setOpenedAt((n) => n + 1);
      })
      .catch(() => {})
      .finally(() => id === req.current && setLoading(false));
  }, [threadId, version]);

  const loadOlder = useCallback(() => {
    if (threadId === null || !next || olderBusy.current) return;
    olderBusy.current = true;
    const id = req.current;
    messagesPage(threadId, next, 50)
      .then((page) => {
        if (id !== req.current) return;
        const older = [...page.messages].reverse();
        prepended.current = older.length;
        setMessages((prev) => {
          const merged = [...older, ...prev];
          lenRef.current = merged.length;
          return merged;
        });
        setNext(page.next);
      })
      .catch(() => {})
      .finally(() => {
        olderBusy.current = false;
      });
  }, [threadId, next]);

  const patch = useCallback((smsId: number, fn: (m: ParsedSms) => ParsedSms | null) => {
    setMessages((prev) =>
      prev.flatMap((m) => {
        if (m.sms.id !== smsId) return [m];
        const r = fn(m);
        return r ? [r] : [];
      })
    );
  }, []);

  return { messages, hasOlder: next !== null, loading, loadOlder, patch, openedAt, prepended };
}

export function useThreadPreview(row: ThreadRow): ParsedSms | null {
  const [p, setP] = useState<ParsedSms | null>(null);
  useEffect(() => {
    let live = true;
    threadPreview(row)
      .then((m) => live && setP(m))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [row.threadId, row.latestMessageId]);
  return p;
}

export function useTranslation(text: string | null): TranslationResult | null {
  const [t, setT] = useState<TranslationResult | null>(null);
  useEffect(() => {
    let live = true;
    setT(null);
    if (text) {
      translateText(text)
        .then((r) => live && setT(r))
        .catch(() => {});
    }
    return () => {
      live = false;
    };
  }, [text]);
  return t;
}

export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setV(value), ms);
    return () => window.clearTimeout(id);
  }, [value, ms]);
  return v;
}
