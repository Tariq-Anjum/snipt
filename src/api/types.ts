// Wire types of the Tauri command surface (src-tauri/src/commands/mod.rs, core.rs).
// Everything here is type-only except the small constants at the bottom, so the
// in-page TS engine is never pulled into the bundle through this module.
import type {
  BudgetLimits,
  Category,
  Classification,
  ExtractedReminder,
  FinanceSummary,
  PaymentCycleConfig,
  SenderKind,
} from '../engine/types';
import type { BudgetPlannerSummary } from '../engine/budget';
import type { CleanupBucketId, WeeklyScanReport } from '../engine/cleanup';
import type { EncryptedOtpPayload } from '../engine/cryptoVault';
import type { CloudBackupProvider } from '../engine/backup';
import type { ActiveOtpItem } from '../engine/store';
import type { TranslationResult } from '../engine/translate';

export type {
  BudgetCategoryId,
  BudgetLimits,
  Category,
  Classification,
  ExtractedReminder,
  FinanceSummary,
  MessageProtocol,
  ParsedSms,
  PaymentCycleConfig,
  PaymentMethod,
  RawSms,
  SenderKind,
} from '../engine/types';
export type {
  ActiveOtpItem,
  BudgetPlannerSummary,
  CleanupBucketId,
  CloudBackupProvider,
  EncryptedOtpPayload,
  TranslationResult,
  WeeklyScanReport,
};

export type InboxFilter = 'All' | Category | 'RCS' | 'Starred' | 'Archived';
export type SimFilter = 'all' | 0 | 1;

export interface ThreadRow {
  threadId: number;
  sender: string;
  displayName: string;
  senderKind: SenderKind;
  latestTs: number;
  latestMessageId: number;
  classification: Classification;
  unreadCount: number;
  hasStarred: boolean;
  isArchived: boolean;
  isBlocked: boolean;
  simSlot: 0 | 1;
  isRcsCapable: boolean;
  messageCount: number;
}

export interface ThreadCursor {
  latestTs: number;
  threadId: number;
}

export interface MessageCursor {
  tsMs: number;
  id: number;
}

export interface ThreadPage {
  threads: ThreadRow[];
  next: ThreadCursor | null;
}

export interface MessagesPage {
  messages: import('../engine/types').ParsedSms[];
  next: MessageCursor | null;
}

export interface OtpItemView {
  item: ActiveOtpItem;
  vault: EncryptedOtpPayload;
}

export interface BackupFile {
  filename: string;
  content: string;
  messageCount: number;
}

export interface IngestReport {
  inserted: number;
  skippedExisting: number;
  threadsTouched: number;
}

export interface RestoreReport {
  ingest: IngestReport;
  archivedThreads: number;
  blockedSenders: number;
}

export interface Capabilities {
  platform: 'android' | 'linux' | 'web';
}

/** Derived (no Rust command yet): a reminder-bearing message. */
export interface ReminderItem {
  smsId: number;
  threadId: number;
  sender: string;
  body: string;
  timestampMs: number;
  reminder: ExtractedReminder;
}

