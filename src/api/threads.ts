import { call } from './core';
import type {
  InboxFilter,
  SimFilter,
  ThreadCursor,
  ThreadPage,
  ThreadRow,
} from './types';

const simArg = (sim: SimFilter): number | null => (sim === 'all' ? null : sim);

export function threadsPage(
  filter: InboxFilter,
  sim: SimFilter,
  after: ThreadCursor | null,
  limit = 50
): Promise<ThreadPage> {
  return call('threads_page', { filter, sim: simArg(sim), after, limit });
}

export function searchThreads(
  filter: InboxFilter,
  sim: SimFilter,
  query: string,
  limit = 100
): Promise<ThreadRow[]> {
  return call('search', { filter, sim: simArg(sim), query, limit });
}

export function archiveThread(threadId: number, archived: boolean): Promise<void> {
  return call('archive', { threadId, archived });
}

export function blockSender(sender: string, blocked: boolean): Promise<void> {
  return call('block', { sender, blocked });
}
