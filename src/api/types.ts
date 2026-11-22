// Wire types of the Tauri command surface (src-tauri/src/commands/mod.rs, core.rs).
// Everything here is type-only except the small constants at the bottom, so the
// in-page TS engine is never pulled into the bundle through this module.
import type {
  BudgetLimits,
  Category,
  Classification,
  ExtractedReminder,
  ExtractedTransaction,
  FinanceSummary,
  MessageProtocol,
  ParsedSms,
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
  /** Latest message body, OTP masked, whitespace collapsed, <= 140 chars. */
  preview: string;
  previewIsOutgoing: boolean;
  latestHasOtp: boolean;
  latestTransaction: ExtractedTransaction | null;
  latestReminder: ExtractedReminder | null;
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

export type SendStatus =
  | 'queued'
  | 'sending'
  | 'sent'
  | 'unconfirmed'
  | 'delivered'
  | 'failed'
  | 'indeterminate';

/**
 * A stored message; outgoing messages queued by Snipt carry a send state.
 * The OTP is masked in `sms.body` and `otp.code`; `otpVault` holds the code
 * encrypted (reveal with `vault_reveal`).
 */
export type MessageView = ParsedSms & {
  sendStatus?: SendStatus;
  otpVault?: EncryptedOtpPayload;
  /** MMS attachments (S15): metadata only; bytes via `mms_image`. */
  attachments?: AttachmentMeta[];
};

/** A non-text MMS part. `image` parts are displayable (type checked by magic bytes). */
export interface AttachmentMeta {
  id: number;
  kind: 'image' | 'unsupported';
  contentType: string;
  name?: string | null;
  size: number;
  hasThumb: boolean;
}

/** `mms_image`: allow-listed image bytes, base64 (turned into a blob: URL). */
export interface MmsImage {
  contentType: string;
  dataB64: string;
}

/** `delete_messages`: rows whose provider row could not be deleted are kept. */
export interface DeleteOutcome {
  deleted: number;
  failedIds: number[];
}

export interface MessagesPage {
  messages: MessageView[];
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
  /** Incoming messages kept under a new id (their id was taken locally). */
  remappedMessages: number;
}

export interface Capabilities {
  /** `pending` until the native platform answered (fail closed meanwhile). */
  platform: 'android' | 'linux' | 'web' | 'pending';
  /** False on Linux (read-only backup viewer): no compose, no quick reply. */
  canSend: boolean;
  readOnly: boolean;
  defaultSmsRole: boolean;
  rcs: boolean;
  mms: boolean;
  simSlots: number;
  /** Native backup save/open picker (SAF / file dialog). False in the dev browser. */
  nativePicker: boolean;
}

export const READ_ONLY_CAPABILITIES: Capabilities = {
  platform: 'linux',
  canSend: false,
  readOnly: true,
  defaultSmsRole: false,
  rcs: false,
  mms: false,
  simSlots: 2,
  nativePicker: false,
};

/**
 * Startup placeholder until `capabilities()` resolves. Nothing that depends
 * on the platform may act on it: vault Reveal and sensitive Copy stay
 * disabled and backup controls wait (R4 #6), so an Android WebView can never
 * take the Linux / browser path before the native answer arrives.
 */
export const PENDING_CAPABILITIES: Capabilities = {
  ...READ_ONLY_CAPABILITIES,
  platform: 'pending',
};

export interface ReminderItem {
  smsId: number;
  threadId: number;
  sender: string;
  body: string;
  timestampMs: number;
  reminder: ExtractedReminder;
  isDone: boolean;
}

export interface InboxCounts {
  totalMessages: number;
  unread: number;
  filters: Record<InboxFilter, number>;
  capacity: number;
}

/** Budget month (starts on `resetDay`, 1-28) and the manual "start fresh" instant. */
export interface BudgetPeriod {
  resetDay: number;
  manualFromMs: number | null;
}

export const DEFAULT_BUDGET_PERIOD: BudgetPeriod = { resetDay: 1, manualFromMs: null };

export interface AppSettings {
  cycleConfig: PaymentCycleConfig;
  budgetLimits: BudgetLimits;
  budgetPeriod: BudgetPeriod;
}

/** Input of `ingest_incoming` (store.ts `ingestIncomingSms`). */
export interface IncomingSms {
  sender: string;
  contactName?: string;
  body: string;
  isKnownContact: boolean;
  simSlot: 0 | 1;
  protocol?: MessageProtocol;
  timestampMs?: number;
}

/** Redacted: body has the OTP masked, `message.otp.code` is bullets. */
export interface NotificationPayload {
  notify: boolean;
  title: string;
  message: ParsedSms;
  otpVault: EncryptedOtpPayload | null;
  otpExpiresAtMs: number | null;
}

export interface IncomingResult {
  inserted: boolean;
  thread: ThreadRow;
  notification: NotificationPayload;
}

export interface OutgoingSms {
  threadId?: number;
  /** With threadId: the expected recipient (mismatch is rejected). */
  address?: string;
  body: string;
  simSlot: 0 | 1;
  /** Reply to this message; it must still exist in threadId. */
  replyTo?: number;
}

export interface OutgoingResult {
  message: MessageView;
  thread: ThreadRow;
}

