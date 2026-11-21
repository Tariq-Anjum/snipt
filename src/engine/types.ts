export type Category =
  | 'Personal'
  | 'Transactions'
  | 'Promotions'
  | 'Priority'
  | 'Spam';

export const CATEGORY_LIST: readonly Category[] = [
  'Personal',
  'Transactions',
  'Promotions',
  'Priority',
  'Spam',
] as const;

export const CATEGORY_COUNT = 5;

export type MessageProtocol = 'SMS' | 'RCS';

export type RcsDeliveryStatus = 'sent' | 'delivered' | 'read';

export interface RcsRichCard {
  title: string;
  subtitle: string;
  actionLabel: string;
  actionMeta?: string;
}

export type DltSuffix = 'Service' | 'Transactional' | 'Promotional' | 'Government';

export type SenderKind =
  | { type: 'Contact' }
  | { type: 'PhoneNumber' }
  | { type: 'Shortcode' }
  | {
      type: 'DltHeader';
      prefix: string;
      entity: string;
      suffix: DltSuffix | null;
    }
  | { type: 'Alphanumeric'; value: string };

export interface RawSms {
  id: number;
  threadId: number;
  sender: string;
  contactName?: string;
  body: string;
  timestampMs: number;
  isKnownContact: boolean;
  isOutgoing: boolean;
  simSlot: 0 | 1;
  isRead?: boolean;
  isStarred?: boolean;
  protocol?: MessageProtocol;
  rcsStatus?: RcsDeliveryStatus;
  rcsRichCard?: RcsRichCard;
}

export interface ExtractedOtp {
  code: string;
  serviceHint: string | null;
  ttlSeconds: number;
}

export type TxnDirection = 'Debit' | 'Credit' | 'Unknown';

export type PaymentMethod = 'UPI' | 'Credit Card' | 'Savings Account';

export const PAYMENT_METHODS: readonly PaymentMethod[] = [
  'UPI',
  'Credit Card',
  'Savings Account',
] as const;

export interface ExtractedTransaction {
  amount: number;
  currency: string;
  direction: TxnDirection;
  paymentMethod: PaymentMethod;
  accountLast4: string | null;
  merchantOrVpa: string | null;
  referenceId: string | null;
  availableBalance: number | null;
  isUpi: boolean;
}

export type ReminderKind = 'BillDue' | 'TravelPnr' | 'Delivery';

export interface ExtractedReminder {
  kind: ReminderKind;
  reference: string | null;
  dateText: string | null;
  summary: string;
}

export type ClassificationReason =
  | 'contact_or_outgoing'
  | 'txn_extractor'
  | 'otp_or_reminder'
  | 'dlt_promo_suffix'
  | 'ml_linear'
  | 'thread_consensus'
  | 'empty_thread';

export interface Classification {
  category: Category;
  confidence: number;
  scores: [number, number, number, number, number];
  reason: ClassificationReason;
}

export interface ParsedSms {
  sms: RawSms;
  senderKind: SenderKind;
  classification: Classification;
  otp: ExtractedOtp | null;
  transaction: ExtractedTransaction | null;
  reminder: ExtractedReminder | null;
}

export interface ThreadSummary {
  threadId: number;
  sender: string;
  displayName: string;
  senderKind: SenderKind;
  messages: ParsedSms[];
  latestMessage: ParsedSms;
  classification: Classification;
  unreadCount: number;
  hasStarred: boolean;
  isArchived: boolean;
  isBlocked: boolean;
  simSlot: 0 | 1;
  isRcsCapable: boolean;
}

export type PaymentCycleConfig = Record<PaymentMethod, number>;

export type BudgetCategoryId =
  | 'Food & Dining'
  | 'Transport & Travel'
  | 'Bills & Utilities'
  | 'Shopping & Retail'
  | 'General & Other';

export const BUDGET_CATEGORIES: readonly BudgetCategoryId[] = [
  'Food & Dining',
  'Transport & Travel',
  'Bills & Utilities',
  'Shopping & Retail',
  'General & Other',
] as const;

export type BudgetLimits = Record<BudgetCategoryId, number>;

export interface FinanceTransactionItem {
  smsId: number;
  threadId: number;
  sender: string;
  timestampMs: number;
  simSlot: 0 | 1;
  body: string;
  txn: ExtractedTransaction;
  budgetCategory: BudgetCategoryId;
  inActiveCycle: boolean;
}

export interface FinanceSummary {
  totalDebits: number;
  totalCredits: number;
  netCashflow: number;
  upiDebitTotal: number;
  creditCardDebitTotal: number;
  savingsDebitTotal: number;
  cardOrBankDebitTotal: number;
  transactionCount: number;
  accountBalances: Array<{
    accountLast4: string;
    bankLabel: string;
    paymentMethod: PaymentMethod;
    balance: number;
    updatedAtMs: number;
  }>;
  topMerchants: Array<{
    merchant: string;
    total: number;
    count: number;
    paymentMethod: PaymentMethod;
  }>;
  transactions: FinanceTransactionItem[];
}
