import { MotionConfig, motion } from 'motion/react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { FinanceView } from './components/FinanceView';
import { InboxView } from './components/InboxView';
import { NotificationBanner } from './components/NotificationBanner';
import { RemindersView } from './components/RemindersView';
import { RustCoreView } from './components/RustCoreView';
import { SimulateSmsModal } from './components/SimulateSmsModal';
import { TopBar, type ActiveTab, type DesignStyle } from './components/TopBar';
import {
  DEFAULT_BUDGET_LIMITS,
  DEFAULT_CYCLE_CONFIG,
  defaultEngine,
  encryptOtpCode,
  generateBulkHistoryMessages,
  loadMessagesFromIndexedDb,
  MIN_HISTORY_CAPACITY,
  runWeeklyUselessScan,
  saveMessagesToIndexedDb,
  type BudgetCategoryId,
  type BudgetLimits,
  type CloudBackupProvider,
  type MessageProtocol,
  type MySmsBackupPayload,
  type ParsedSms,
  type PaymentCycleConfig,
  type PaymentMethod,
} from './engine';
import { createSeedMessages } from './engine/seedData';
import {
  appendOutgoingReply,
  buildThreads,
  collectOtpItems,
  collectReminders,
  computeFinanceSummary,
  deleteMessagesByIds,
  filterThreads,
  ingestIncomingSms,
  markSmsRead,
  purgeExpiredOtpMessages,
  type InboxFilter,
  type StoreState,
} from './engine/store';

const META_STORAGE_KEY = 'mysms-meta-v2';
const LEGACY_MESSAGES_KEY = 'mysms-messages-small-v2';
const THEME_KEY = 'mysms-theme-v2';
const STYLE_KEY = 'mysms-design-style-v1';

interface PersistedMeta {
  archivedThreadIds: number[];
  blockedSenders: string[];
  completedReminderSmsIds: number[];
  cycleConfig: PaymentCycleConfig;
  budgetLimits: BudgetLimits;
  scanScheduleDay: string;
  autoTranslateToEnglish: boolean;
  preferredCloudProvider: CloudBackupProvider;
  lastBackupAtMs: number | null;
}

function loadInitialMeta(): PersistedMeta {
  try {
    const raw = window.localStorage.getItem(META_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PersistedMeta>;
      return {
        archivedThreadIds: Array.isArray(parsed.archivedThreadIds)
          ? parsed.archivedThreadIds
          : [],
        blockedSenders: Array.isArray(parsed.blockedSenders)
          ? parsed.blockedSenders
          : [],
        completedReminderSmsIds: Array.isArray(parsed.completedReminderSmsIds)
          ? parsed.completedReminderSmsIds
          : [],
        cycleConfig: {
          ...DEFAULT_CYCLE_CONFIG,
          ...(parsed.cycleConfig ?? {}),
        },
        budgetLimits: {
          ...DEFAULT_BUDGET_LIMITS,
          ...(parsed.budgetLimits ?? {}),
        },
        scanScheduleDay:
          typeof parsed.scanScheduleDay === 'string'
            ? parsed.scanScheduleDay
            : 'Every Sunday · 9:00 AM',
        autoTranslateToEnglish:
          typeof parsed.autoTranslateToEnglish === 'boolean'
            ? parsed.autoTranslateToEnglish
            : true,
        preferredCloudProvider:
          parsed.preferredCloudProvider === 'google_drive' ||
          parsed.preferredCloudProvider === 'onedrive' ||
          parsed.preferredCloudProvider === 'dropbox' ||
          parsed.preferredCloudProvider === 'device_picker'
            ? parsed.preferredCloudProvider
            : 'device_picker',
        lastBackupAtMs:
          typeof parsed.lastBackupAtMs === 'number'
            ? parsed.lastBackupAtMs
            : null,
      };
    }
  } catch {
    // ignore malformed storage
  }
  return {
    archivedThreadIds: [],
    blockedSenders: [],
    completedReminderSmsIds: [],
    cycleConfig: { ...DEFAULT_CYCLE_CONFIG },
    budgetLimits: { ...DEFAULT_BUDGET_LIMITS },
    scanScheduleDay: 'Every Sunday · 9:00 AM',
    autoTranslateToEnglish: true,
    preferredCloudProvider: 'device_picker',
    lastBackupAtMs: null,
  };
}

function loadSyncMessages(): StoreState['messages'] {
  try {
    const raw = window.localStorage.getItem(LEGACY_MESSAGES_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.length > 0) {
        return parsed;
      }
    }
  } catch {
    // ignore
  }
  return createSeedMessages(Date.now());
}

function getInitialTheme(): 'light' | 'dark' {
  try {
    const saved = window.localStorage.getItem(THEME_KEY);
    if (saved === 'dark' || saved === 'light') return saved;
  } catch {
    // ignore
  }
  return 'light';
}

function getInitialDesignStyle(): DesignStyle {
  try {
    const saved = window.localStorage.getItem(STYLE_KEY);
    if (saved === 'glass' || saved === 'solid') return saved;
  } catch {
    // ignore
  }
  return 'glass';
}

export default function App() {
  const initialMeta = useMemo(() => loadInitialMeta(), []);
  const [store, setStore] = useState<StoreState>(() => ({
    messages: loadSyncMessages(),
    archivedThreadIds: initialMeta.archivedThreadIds,
    blockedSenders: initialMeta.blockedSenders,
    completedReminderSmsIds: initialMeta.completedReminderSmsIds,
  }));
  const [cycleConfig, setCycleConfig] = useState<PaymentCycleConfig>(
    initialMeta.cycleConfig
  );
  const [budgetLimits, setBudgetLimits] = useState<BudgetLimits>(
    initialMeta.budgetLimits
  );
  const [onlyActiveCycle, setOnlyActiveCycle] = useState(true);
  const [scanScheduleDay, setScanScheduleDay] = useState(
    initialMeta.scanScheduleDay
  );
  const [autoTranslateToEnglish, setAutoTranslateToEnglish] = useState(
    initialMeta.autoTranslateToEnglish
  );
  const [preferredCloudProvider, setPreferredCloudProvider] =
    useState<CloudBackupProvider>(initialMeta.preferredCloudProvider);
  const [lastBackupAtMs, setLastBackupAtMs] = useState<number | null>(
    initialMeta.lastBackupAtMs
  );
  const [theme, setTheme] = useState<'light' | 'dark'>(() => getInitialTheme());
  const [designStyle, setDesignStyle] = useState<DesignStyle>(() =>
    getInitialDesignStyle()
  );
  const [activeTab, setActiveTab] = useState<ActiveTab>('inbox');
  const [inboxFilter, setInboxFilter] = useState<InboxFilter>('All');
  const [simFilter, setSimFilter] = useState<'all' | 0 | 1>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedThreadId, setSelectedThreadId] = useState<number | null>(101);
  const [simulateOpen, setSimulateOpen] = useState(false);
  const [latestNotification, setLatestNotification] = useState<ParsedSms | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [isLoadingBulk, setIsLoadingBulk] = useState(false);
  const [clipboardClearSecondsLeft, setClipboardClearSecondsLeft] = useState<
    number | null
  >(null);

  const hydratedRef = useRef(false);

  // Hydrate 10,000+ SMS history from IndexedDB on startup
  useEffect(() => {
    let active = true;
    loadMessagesFromIndexedDb().then((saved) => {
      hydratedRef.current = true;
      if (active && saved && saved.length > 0) {
        setStore((prev) => ({ ...prev, messages: saved }));
      }
    });
    return () => {
      active = false;
    };
  }, []);

  // Persist messages to IndexedDB (supports 10,000+ SMS easily without localStorage quota error)
  useEffect(() => {
    if (!hydratedRef.current && store.messages.length <= 30) {
      // Wait for initial check, or save lightweight copy to localStorage when small
    }
    void saveMessagesToIndexedDb(store.messages);
    if (store.messages.length <= 200) {
      try {
        window.localStorage.setItem(
          LEGACY_MESSAGES_KEY,
          JSON.stringify(store.messages)
        );
      } catch {
        // ignore quota errors
      }
    } else {
      try {
        window.localStorage.removeItem(LEGACY_MESSAGES_KEY);
      } catch {
        // ignore
      }
    }
  }, [store.messages]);

  // Persist settings metadata
  useEffect(() => {
    try {
      const meta: PersistedMeta = {
        archivedThreadIds: store.archivedThreadIds,
        blockedSenders: store.blockedSenders,
        completedReminderSmsIds: store.completedReminderSmsIds,
        cycleConfig,
        budgetLimits,
        scanScheduleDay,
        autoTranslateToEnglish,
        preferredCloudProvider,
        lastBackupAtMs,
      };
      window.localStorage.setItem(META_STORAGE_KEY, JSON.stringify(meta));
    } catch {
      // ignore
    }
  }, [
    store.archivedThreadIds,
    store.blockedSenders,
    store.completedReminderSmsIds,
    cycleConfig,
    budgetLimits,
    scanScheduleDay,
    autoTranslateToEnglish,
    preferredCloudProvider,
    lastBackupAtMs,
  ]);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'dark') root.classList.add('dark');
    else root.classList.remove('dark');
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // ignore
    }
  }, [theme]);

  useEffect(() => {
    const root = document.documentElement;
    if (designStyle === 'glass') root.classList.add('liquid-glass');
    else root.classList.remove('liquid-glass');
    try {
      window.localStorage.setItem(STYLE_KEY, designStyle);
    } catch {
      // ignore
    }
  }, [designStyle]);

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  // 30-second encrypted OTP clipboard auto-clear countdown
  useEffect(() => {
    if (clipboardClearSecondsLeft === null) return;
    if (clipboardClearSecondsLeft <= 0) {
      navigator.clipboard?.writeText('').catch(() => {});
      setClipboardClearSecondsLeft(null);
      return;
    }
    const id = window.setTimeout(
      () => setClipboardClearSecondsLeft((s) => (s !== null ? s - 1 : null)),
      1000
    );
    return () => window.clearTimeout(id);
  }, [clipboardClearSecondsLeft]);

  const allThreads = useMemo(() => buildThreads(store), [store]);

  const filteredThreads = useMemo(
    () => filterThreads(allThreads, inboxFilter, simFilter, searchQuery),
    [allThreads, inboxFilter, simFilter, searchQuery]
  );

  const financeSummary = useMemo(
    () => computeFinanceSummary(allThreads, cycleConfig, nowMs, onlyActiveCycle),
    [allThreads, cycleConfig, nowMs, onlyActiveCycle]
  );

  const otpItems = useMemo(
    () => collectOtpItems(allThreads, nowMs),
    [allThreads, nowMs]
  );

  const reminderItems = useMemo(
    () => collectReminders(allThreads, store.completedReminderSmsIds),
    [allThreads, store.completedReminderSmsIds]
  );

  const weeklyScan = useMemo(
    () => runWeeklyUselessScan(store.messages, store.blockedSenders, nowMs),
    [store.messages, store.blockedSenders, nowMs]
  );

  const totalUnread = useMemo(
    () =>
      allThreads
        .filter((t) => !t.isArchived && t.classification.category !== 'Spam')
        .reduce((sum, t) => sum + t.unreadCount, 0),
    [allThreads]
  );

  const pendingRemindersCount = useMemo(
    () => reminderItems.filter((r) => !r.isDone).length,
    [reminderItems]
  );

  const handleSelectThread = (threadId: number | null) => {
    setSelectedThreadId(threadId);
    if (threadId === null) return;
    setStore((prev) => {
      const hasUnread = prev.messages.some(
        (m) => m.threadId === threadId && !m.isOutgoing && !m.isRead
      );
      if (!hasUnread) return prev;
      return {
        ...prev,
        messages: prev.messages.map((m) =>
          m.threadId === threadId ? { ...m, isRead: true } : m
        ),
      };
    });
  };

  const handleDeliverSimulatedSms = (payload: {
    sender: string;
    contactName?: string;
    body: string;
    isKnownContact: boolean;
    simSlot: 0 | 1;
    protocol: MessageProtocol;
  }) => {
    const now = Date.now();
    setNowMs(now);
    const { nextState, createdSms } = ingestIncomingSms(store, {
      ...payload,
      timestampMs: now,
    });
    const parsed = defaultEngine.analyzeMessage(createdSms);
    if (parsed.otp) {
      // Seal OTP into AES-256-GCM non-extractable vault context
      void encryptOtpCode(parsed.otp.code).catch(() => {});
    }
    setStore(nextState);
    setLatestNotification(parsed);
    setActiveTab('inbox');
    if (parsed.classification.category === 'Spam') {
      setInboxFilter('Spam');
    } else if (inboxFilter === 'Archived' || inboxFilter === 'Starred') {
      setInboxFilter('All');
    }
    setSelectedThreadId(createdSms.threadId);
  };

  const handleSendReply = (
    threadId: number,
    body: string,
    simSlot: 0 | 1,
    protocol: MessageProtocol
  ) => {
    setStore((prev) => appendOutgoingReply(prev, threadId, body, simSlot, protocol));
  };

  const handleMarkSmsRead = (smsId: number) => {
    setStore((prev) => markSmsRead(prev, smsId));
  };

  const handleToggleStar = (smsId: number) => {
    setStore((prev) => ({
      ...prev,
      messages: prev.messages.map((m) =>
        m.id === smsId ? { ...m, isStarred: !m.isStarred } : m
      ),
    }));
  };

  const handleDeleteMessage = (smsId: number) => {
    setStore((prev) => ({
      ...prev,
      messages: prev.messages.filter((m) => m.id !== smsId),
    }));
  };

  const handleToggleArchive = (threadId: number) => {
    setStore((prev) => {
      const exists = prev.archivedThreadIds.includes(threadId);
      return {
        ...prev,
        archivedThreadIds: exists
          ? prev.archivedThreadIds.filter((id) => id !== threadId)
          : [...prev.archivedThreadIds, threadId],
      };
    });
  };

  const handleToggleBlock = (sender: string) => {
    const key = sender.toLowerCase();
    setStore((prev) => {
      const exists = prev.blockedSenders.some((s) => s.toLowerCase() === key);
      return {
        ...prev,
        blockedSenders: exists
          ? prev.blockedSenders.filter((s) => s.toLowerCase() !== key)
          : [...prev.blockedSenders, sender],
      };
    });
  };

  const handleToggleReminderDone = (smsId: number) => {
    setStore((prev) => {
      const exists = prev.completedReminderSmsIds.includes(smsId);
      return {
        ...prev,
        completedReminderSmsIds: exists
          ? prev.completedReminderSmsIds.filter((id) => id !== smsId)
          : [...prev.completedReminderSmsIds, smsId],
      };
    });
  };

  const handlePurgeExpiredOtps = (): number => {
    const now = Date.now();
    setNowMs(now);
    const { nextState, purgedCount } = purgeExpiredOtpMessages(store, now);
    setStore(nextState);
    return purgedCount;
  };

  const handleDeleteSelectedUseless = (ids: number[]): number => {
    const { nextState, deletedCount } = deleteMessagesByIds(store, ids);
    setStore(nextState);
    return deletedCount;
  };

  const handleLoadTenThousand = () => {
    setIsLoadingBulk(true);
    window.setTimeout(() => {
      const now = Date.now();
      setNowMs(now);
      setStore((prev) => ({
        ...prev,
        messages: generateBulkHistoryMessages(
          MIN_HISTORY_CAPACITY,
          prev.messages,
          now
        ),
      }));
      setIsLoadingBulk(false);
    }, 30);
  };

  const handleCopyOtp = (code: string) => {
    void encryptOtpCode(code).catch(() => {});
    navigator.clipboard?.writeText(code).catch(() => {});
    setClipboardClearSecondsLeft(30);
  };

  const handleChangeCycleDay = (method: PaymentMethod, day: number) => {
    setCycleConfig((prev) => ({ ...prev, [method]: day }));
  };

  const handleChangeBudgetLimit = (category: BudgetCategoryId, limit: number) => {
    setBudgetLimits((prev) => ({ ...prev, [category]: limit }));
  };

  const handleJumpToThread = (threadId: number) => {
    setActiveTab('inbox');
    setInboxFilter('All');
    setSearchQuery('');
    handleSelectThread(threadId);
  };

  const handleRestoreBackup = (payload: MySmsBackupPayload) => {
    setStore({
      messages: payload.messages,
      archivedThreadIds: payload.archivedThreadIds ?? [],
      blockedSenders: payload.blockedSenders ?? [],
      completedReminderSmsIds: payload.completedReminderSmsIds ?? [],
    });
    if (payload.cycleConfig) {
      setCycleConfig({ ...DEFAULT_CYCLE_CONFIG, ...payload.cycleConfig });
    }
    if (payload.budgetLimits) {
      setBudgetLimits({ ...DEFAULT_BUDGET_LIMITS, ...payload.budgetLimits });
    }
    if (payload.scanScheduleDay) {
      setScanScheduleDay(payload.scanScheduleDay);
    }
    if (typeof payload.autoTranslateToEnglish === 'boolean') {
      setAutoTranslateToEnglish(payload.autoTranslateToEnglish);
    }
    if (payload.provider) {
      setPreferredCloudProvider(payload.provider);
    }
    setNowMs(Date.now());
  };

  const handleResetDemo = () => {
    const now = Date.now();
    setNowMs(now);
    setStore({
      messages: createSeedMessages(now),
      archivedThreadIds: [],
      blockedSenders: [],
      completedReminderSmsIds: [],
    });
    setCycleConfig({ ...DEFAULT_CYCLE_CONFIG });
    setBudgetLimits({ ...DEFAULT_BUDGET_LIMITS });
    setAutoTranslateToEnglish(true);
    setInboxFilter('All');
    setSimFilter('all');
    setSearchQuery('');
    setSelectedThreadId(101);
  };

  return (
    <MotionConfig reducedMotion="user">
      <div className="relative h-screen flex flex-col overflow-hidden bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100 glass:bg-transparent">
        {/* Luminous Ambient Refraction Canvas (visible in Liquid Glass mode only) */}
        {designStyle === 'glass' && (
          <div
            aria-hidden="true"
            className="pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-[#edf3fb] dark:bg-[#070d19] transition-colors"
          >
            <div
              className="liquid-ambient-a absolute -top-28 -left-24 w-[58vw] h-[58vw] max-w-[780px] max-h-[780px] rounded-full opacity-85 dark:opacity-55 blur-3xl"
              style={{
                background:
                  theme === 'dark'
                    ? 'radial-gradient(circle, rgba(37,99,235,0.42) 0%, rgba(14,116,144,0.18) 52%, transparent 74%)'
                    : 'radial-gradient(circle, rgba(125,211,252,0.62) 0%, rgba(191,219,254,0.42) 52%, transparent 74%)',
              }}
            />
            <div
              className="liquid-ambient-b absolute -bottom-32 -right-24 w-[62vw] h-[62vw] max-w-[840px] max-h-[840px] rounded-full opacity-80 dark:opacity-50 blur-3xl"
              style={{
                background:
                  theme === 'dark'
                    ? 'radial-gradient(circle, rgba(56,189,248,0.28) 0%, rgba(30,58,138,0.34) 48%, transparent 74%)'
                    : 'radial-gradient(circle, rgba(165,243,252,0.52) 0%, rgba(199,210,254,0.42) 50%, transparent 74%)',
              }}
            />
          </div>
        )}

        <TopBar
          activeTab={activeTab}
          onSelectTab={setActiveTab}
          onOpenSimulate={() => setSimulateOpen(true)}
          unreadCount={totalUnread}
          pendingReminderCount={pendingRemindersCount}
          designStyle={designStyle}
        />

        <motion.div
          key={activeTab}
          initial={{ opacity: 0, y: 5 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.16, ease: [0.16, 1, 0.3, 1] }}
          className="flex-1 flex min-h-0 overflow-hidden"
        >
          {activeTab === 'inbox' && (
            <InboxView
              allThreads={allThreads}
              filteredThreads={filteredThreads}
              totalMessageCount={store.messages.length}
              isLoadingBulk={isLoadingBulk}
              onLoadTenThousand={handleLoadTenThousand}
              activeFilter={inboxFilter}
              onChangeFilter={setInboxFilter}
              simFilter={simFilter}
              onChangeSimFilter={setSimFilter}
              searchQuery={searchQuery}
              onChangeSearch={setSearchQuery}
              selectedThreadId={selectedThreadId}
              onSelectThread={handleSelectThread}
              onSendReply={handleSendReply}
              onToggleStar={handleToggleStar}
              onDeleteMessage={handleDeleteMessage}
              onToggleArchive={handleToggleArchive}
              onToggleBlock={handleToggleBlock}
              onCopyOtp={handleCopyOtp}
              completedReminderIds={store.completedReminderSmsIds}
              onToggleReminderDone={handleToggleReminderDone}
              theme={theme}
              onToggleTheme={() => setTheme((t) => (t === 'dark' ? 'light' : 'dark'))}
              designStyle={designStyle}
              onChangeDesignStyle={setDesignStyle}
              autoTranslateToEnglish={autoTranslateToEnglish}
              onToggleAutoTranslate={() => setAutoTranslateToEnglish((v) => !v)}
              onResetDemo={handleResetDemo}
              nowMs={nowMs}
            />
          )}

          {activeTab === 'finance' && (
            <FinanceView
              summary={financeSummary}
              cycleConfig={cycleConfig}
              onChangeCycleDay={handleChangeCycleDay}
              onlyActiveCycle={onlyActiveCycle}
              onToggleOnlyActiveCycle={setOnlyActiveCycle}
              budgetLimits={budgetLimits}
              onChangeBudgetLimit={handleChangeBudgetLimit}
              onOpenThread={handleJumpToThread}
              nowMs={nowMs}
            />
          )}

          {activeTab === 'reminders' && (
            <RemindersView
              messages={store.messages}
              archivedThreadIds={store.archivedThreadIds}
              blockedSenders={store.blockedSenders}
              completedReminderSmsIds={store.completedReminderSmsIds}
              cycleConfig={cycleConfig}
              budgetLimits={budgetLimits}
              autoTranslateToEnglish={autoTranslateToEnglish}
              preferredCloudProvider={preferredCloudProvider}
              onChangeCloudProvider={setPreferredCloudProvider}
              lastBackupAtMs={lastBackupAtMs}
              onBackupSuccess={setLastBackupAtMs}
              onRestoreBackup={handleRestoreBackup}
              otps={otpItems}
              reminders={reminderItems}
              weeklyScan={weeklyScan}
              scanScheduleDay={scanScheduleDay}
              onChangeScanScheduleDay={setScanScheduleDay}
              onDeleteSelectedUseless={handleDeleteSelectedUseless}
              onCopyOtp={handleCopyOtp}
              clipboardClearSecondsLeft={clipboardClearSecondsLeft}
              onPurgeExpiredOtps={handlePurgeExpiredOtps}
              onToggleReminderDone={handleToggleReminderDone}
              onOpenThread={handleJumpToThread}
              nowMs={nowMs}
            />
          )}

          {activeTab === 'rust' && (
            <RustCoreView
              designStyle={designStyle}
              onChangeDesignStyle={setDesignStyle}
            />
          )}
        </motion.div>

        <SimulateSmsModal
          isOpen={simulateOpen}
          onClose={() => setSimulateOpen(false)}
          onDeliver={handleDeliverSimulatedSms}
        />

        <NotificationBanner
          parsed={latestNotification}
          onDismiss={() => setLatestNotification(null)}
          onOpenThread={handleJumpToThread}
          onCopyOtp={handleCopyOtp}
          onMarkRead={handleMarkSmsRead}
          onDeleteSms={handleDeleteMessage}
          onQuickReply={handleSendReply}
        />
      </div>
    </MotionConfig>
  );
}
