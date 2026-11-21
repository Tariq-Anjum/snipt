import { LazyMotion, MotionConfig, domAnimation, m } from 'motion/react';
import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import {
  applyCleanup,
  budgetStats as fetchBudgetStats,
  capabilities,
  deleteMessages,
  financeSummary as fetchFinanceSummary,
  getSettings,
  getThread,
  inboxCounts as fetchInboxCounts,
  ingestIncoming,
  isTauri,
  markRead,
  otpItems as fetchOtpItems,
  powerSaveActive,
  purgeExpiredOtps,
  queueOutgoing,
  reminders as fetchReminders,
  setBackHandler,
  setBudget,
  setCycle,
  setReminderDone,
  threadsPage,
  weeklyScan as fetchWeeklyScan,
  DEFAULT_CYCLE_CONFIG,
  READ_ONLY_CAPABILITIES,
  type BudgetCategoryId,
  type BudgetLimits,
  type BudgetPlannerSummary,
  type Capabilities,
  type CleanupBucketId,
  type CloudBackupProvider,
  type FinanceSummary,
  type InboxCounts,
  type InboxFilter,
  type IncomingSms,
  type NotificationPayload,
  type OtpItemView,
  type PaymentCycleConfig,
  type PaymentMethod,
  type ReminderItem,
  type SimFilter,
  type ThreadRow,
  type WeeklyScanReport,
} from './api';
import { devImportSeed } from './api/dev';
import { FinanceView } from './components/FinanceView';
import { InboxView } from './components/InboxView';
import { NotificationBanner } from './components/NotificationBanner';
import { RemindersView } from './components/RemindersView';
import { TopBar, type ActiveTab, type DesignStyle } from './components/TopBar';
import { useDebounced } from './hooks/data';
import { popBack, subscribeBack, useBackLayer } from './utils/backStack';
import {
  glassSupported,
  installScrollFlags,
  prefersReducedTransparency,
  startFrameGuard,
} from './utils/perf';

const SimulateSmsModal = import.meta.env.DEV
  ? lazy(() =>
      import('./components/SimulateSmsModal').then((mod) => ({
        default: mod.SimulateSmsModal,
      }))
    )
  : null;

const THEME_KEY = 'snipt-theme-v1';
const STYLE_KEY = 'snipt-design-style-v1';
const PREFS_KEY = 'snipt-prefs-v1';

// UI-only preferences. Data state (reminder completion, billing cycles, budget)
// lives in the Rust core and is read through commands, never mirrored here.
interface Prefs {
  scanScheduleDay: string;
  autoTranslateToEnglish: boolean;
  preferredCloudProvider: CloudBackupProvider;
  lastBackupAtMs: number | null;
}

const DEFAULT_PREFS: Prefs = {
  scanScheduleDay: 'Every Sunday · 9:00 AM',
  autoTranslateToEnglish: true,
  preferredCloudProvider: 'device_picker',
  lastBackupAtMs: null,
};

function loadPrefs(): Prefs {
  try {
    const raw = window.localStorage.getItem(PREFS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<Prefs>;
      // Pick known keys only: drops data mirrors written by earlier builds.
      return {
        scanScheduleDay: p.scanScheduleDay ?? DEFAULT_PREFS.scanScheduleDay,
        autoTranslateToEnglish: p.autoTranslateToEnglish ?? DEFAULT_PREFS.autoTranslateToEnglish,
        preferredCloudProvider: p.preferredCloudProvider ?? DEFAULT_PREFS.preferredCloudProvider,
        lastBackupAtMs: p.lastBackupAtMs ?? DEFAULT_PREFS.lastBackupAtMs,
      };
    }
  } catch {
    // ignore malformed storage
  }
  return DEFAULT_PREFS;
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
  const [prefs, setPrefs] = useState<Prefs>(() => loadPrefs());
  const [cycleConfig, setCycleConfig] = useState<PaymentCycleConfig>(() => ({
    ...DEFAULT_CYCLE_CONFIG,
  }));
  const [onlyActiveCycle, setOnlyActiveCycle] = useState(true);
  const [theme, setTheme] = useState<'light' | 'dark'>(() => getInitialTheme());
  const [designStyle, setDesignStyle] = useState<DesignStyle>(() => getInitialDesignStyle());
  const [degraded, setDegraded] = useState(
    () => !glassSupported() || prefersReducedTransparency()
  );
  const [notice, setNotice] = useState<string | null>(null);
  const [caps, setCaps] = useState<Capabilities>(READ_ONLY_CAPABILITIES);
  const platform = caps.platform;

  const [version, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  const [activeTab, setActiveTab] = useState<ActiveTab>('inbox');
  const [inboxFilter, setInboxFilter] = useState<InboxFilter>('All');
  const [simFilter, setSimFilter] = useState<SimFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const debouncedQuery = useDebounced(searchQuery, 150);
  const [selectedThread, setSelectedThread] = useState<ThreadRow | null>(null);
  const [simulateOpen, setSimulateOpen] = useState(false);
  const [latestNotification, setLatestNotification] = useState<NotificationPayload | null>(
    null
  );
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [isLoadingSample, setIsLoadingSample] = useState(false);
  const [clipboardClearSecondsLeft, setClipboardClearSecondsLeft] = useState<number | null>(null);

  const [finance, setFinance] = useState<FinanceSummary | null>(null);
  const [budget, setBudgetState] = useState<BudgetPlannerSummary | null>(null);
  const [otps, setOtps] = useState<OtpItemView[]>([]);
  const [scan, setScan] = useState<WeeklyScanReport | null>(null);
  const [reminders, setReminders] = useState<ReminderItem[]>([]);
  const [remindersLoading, setRemindersLoading] = useState(true);
  const [counts, setCounts] = useState<InboxCounts | null>(null);

  const glassActive = designStyle === 'glass' && !degraded;

  const patchPrefs = (patch: Partial<Prefs>) =>
    setPrefs((prev) => {
      const next = { ...prev, ...patch };
      try {
        window.localStorage.setItem(PREFS_KEY, JSON.stringify(next));
      } catch {
        // ignore quota/private-mode errors
      }
      return next;
    });

  // ---- platform + back-stack --------------------------------------------
  useEffect(() => {
    capabilities()
      .then(setCaps)
      .catch(() => {});
  }, []);

  useEffect(() => subscribeBack((depth) => setBackHandler(depth > 0, platform)), [platform]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && popBack()) e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    let unlisten: (() => void) | undefined;
    if (isTauri()) {
      import('@tauri-apps/api/event')
        .then(({ listen }) => listen('back', () => popBack()))
        .then((u) => {
          unlisten = u;
        })
        .catch(() => {});
    }
    return () => {
      window.removeEventListener('keydown', onKey);
      unlisten?.();
    };
  }, []);

  useBackLayer(selectedThread !== null, () => setSelectedThread(null));
  useBackLayer(simulateOpen, () => setSimulateOpen(false));
  useBackLayer(activeTab !== 'inbox', () => setActiveTab('inbox'));

  // ---- perf guards --------------------------------------------------------
  useEffect(() => installScrollFlags(), []);

  const degrade = useCallback((message: string) => {
    setDegraded((was) => {
      if (!was) setNotice(message);
      return true;
    });
  }, []);

  useEffect(() => {
    if (!glassActive) return;
    return startFrameGuard(() =>
      degrade('Scrolling was dropping frames, so Snipt switched to Crisp Solid for this session.')
    );
  }, [glassActive, degrade]);

  useEffect(() => {
    powerSaveActive(platform).then((on) => {
      if (on) degrade('Power saver is on, so Snipt switched to Crisp Solid.');
    });
  }, [platform, degrade]);

  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 6000);
    return () => window.clearTimeout(id);
  }, [notice]);

  // ---- appearance ---------------------------------------------------------
  useEffect(() => {
    document.documentElement.classList.toggle('dark', theme === 'dark');
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // ignore
    }
  }, [theme]);

  useEffect(() => {
    try {
      window.localStorage.setItem(STYLE_KEY, designStyle);
    } catch {
      // ignore
    }
  }, [designStyle]);

  useEffect(() => {
    document.documentElement.classList.toggle('liquid-glass', glassActive);
  }, [glassActive]);

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  // 30-second OTP clipboard auto-clear countdown
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

  // ---- data -----------------------------------------------------------------
  // Billing cycles are owned by the core.
  useEffect(() => {
    getSettings()
      .then((s) => setCycleConfig(s.cycleConfig))
      .catch(() => {});
  }, []);

  // Dev-seed builds only (VITE_DEV_SEED=1): fill an empty database once.
  useEffect(() => {
    if (import.meta.env.VITE_DEV_SEED !== '1') return;
    threadsPage('All', 'all', null, 1)
      .then((p) => (p.threads.length === 0 ? devImportSeed().then(bump) : undefined))
      .catch(() => {});
  }, [bump]);

  // Unread badge, sidebar counts and the 10,000 capacity meter (one query).
  useEffect(() => {
    fetchInboxCounts()
      .then(setCounts)
      .catch(() => {});
  }, [version]);

  useEffect(() => {
    if (activeTab !== 'finance') return;
    let live = true;
    Promise.all([fetchFinanceSummary(onlyActiveCycle), fetchBudgetStats(onlyActiveCycle)])
      .then(([f, b]) => {
        if (!live) return;
        setFinance(f);
        setBudgetState(b);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [activeTab, onlyActiveCycle, version]);

  useEffect(() => {
    if (activeTab !== 'reminders') return;
    let live = true;
    Promise.all([fetchOtpItems(), fetchWeeklyScan()])
      .then(([o, s]) => {
        if (!live) return;
        setOtps(o);
        setScan(s);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [activeTab, version]);

  // Reminders feed the tab badge, so load them at startup and on changes.
  useEffect(() => {
    let live = true;
    fetchReminders()
      .then((r) => live && setReminders(r))
      .catch(() => {})
      .finally(() => live && setRemindersLoading(false));
    return () => {
      live = false;
    };
  }, [version]);

  const pendingRemindersCount = reminders.filter((r) => !r.isDone).length;
  const completedReminderIds = reminders.filter((r) => r.isDone).map((r) => r.smsId);

  // ---- handlers ---------------------------------------------------------------
  const handleJumpToThread = useCallback(async (threadId: number) => {
    setActiveTab('inbox');
    setInboxFilter('All');
    setSearchQuery('');
    const row = await getThread(threadId).catch(() => null);
    if (row) setSelectedThread(row);
  }, []);

  const handleToggleReminderDone = (smsId: number) => {
    const done = !reminders.find((r) => r.smsId === smsId)?.isDone;
    setReminders((prev) =>
      prev
        .map((r) => (r.smsId === smsId ? { ...r, isDone: done } : r))
        .sort((a, b) => Number(a.isDone) - Number(b.isDone) || b.timestampMs - a.timestampMs)
    );
    setReminderDone(smsId, done)
      .then(bump)
      .catch(bump);
  };

  const handlePurgeExpiredOtps = async (): Promise<number> => {
    const removed = await purgeExpiredOtps();
    bump();
    return removed;
  };

  const handleQuickReply = async (threadId: number, body: string, simSlot: 0 | 1) => {
    await queueOutgoing({ threadId, body, simSlot });
    bump();
  };

  const handleCopyOtp = (code: string) => {
    navigator.clipboard?.writeText(code).catch(() => {});
    setClipboardClearSecondsLeft(30);
  };

  const handleApplyCleanup = async (buckets: CleanupBucketId[]): Promise<number> => {
    const removed = await applyCleanup(buckets);
    bump();
    return removed;
  };

  const handleChangeCycleDay = (method: PaymentMethod, day: number) => {
    const next = { ...cycleConfig, [method]: day };
    setCycleConfig(next);
    setCycle(next)
      .then(bump)
      .catch(() => {
        getSettings()
          .then((s) => setCycleConfig(s.cycleConfig))
          .catch(() => {});
      });
  };

  const budgetRef = useRef<BudgetPlannerSummary | null>(null);
  budgetRef.current = budget;
  const budgetTimer = useRef<number | undefined>(undefined);
  const handleChangeBudgetLimit = (category: BudgetCategoryId, limit: number) => {
    setBudgetState((prev) =>
      prev
        ? {
            ...prev,
            categories: prev.categories.map((c) =>
              c.category === category ? { ...c, limit } : c
            ),
          }
        : prev
    );
    window.clearTimeout(budgetTimer.current);
    budgetTimer.current = window.setTimeout(() => {
      const cats = budgetRef.current?.categories;
      if (!cats) return;
      const limits = Object.fromEntries(cats.map((c) => [c.category, c.limit])) as BudgetLimits;
      setBudget(limits)
        .then(bump)
        .catch(() => {});
    }, 350);
  };

  const handleLoadSample = () => {
    setIsLoadingSample(true);
    devImportSeed()
      .then(bump)
      .catch(() => {})
      .finally(() => setIsLoadingSample(false));
  };

  // Dev simulator: goes through the same ingest command as the Android receiver.
  const handleDeliverSimulatedSms = (payload: IncomingSms) => {
    ingestIncoming(payload)
      .then((r) => {
        setNowMs(Date.now());
        if (r.notification.notify) setLatestNotification(r.notification);
        setActiveTab('inbox');
        setInboxFilter(r.thread.classification.category === 'Spam' ? 'Spam' : 'All');
        bump();
        setSelectedThread(r.thread);
      })
      .catch(() => {});
  };

  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <div className="app-shell relative flex flex-col overflow-hidden bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100 glass:bg-transparent">
          {/* Luminous ambient canvas (Liquid Glass only; transform-only animation) */}
          {glassActive && (
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
            onOpenSimulate={import.meta.env.DEV ? () => setSimulateOpen(true) : undefined}
            unreadCount={counts?.unread ?? 0}
            pendingReminderCount={pendingRemindersCount}
            designStyle={glassActive ? 'glass' : 'solid'}
          />

          <m.div
            key={activeTab}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.16, ease: [0.16, 1, 0.3, 1] }}
            className="flex-1 flex min-h-0 overflow-hidden"
          >
            {activeTab === 'inbox' && (
              <InboxView
                version={version}
                activeFilter={inboxFilter}
                onChangeFilter={setInboxFilter}
                simFilter={simFilter}
                onChangeSimFilter={setSimFilter}
                searchQuery={searchQuery}
                debouncedQuery={debouncedQuery}
                onChangeSearch={setSearchQuery}
                selectedThread={selectedThread}
                onSelectThread={setSelectedThread}
                onThreadPatched={(patch) =>
                  setSelectedThread((prev) => (prev ? { ...prev, ...patch } : prev))
                }
                onChanged={bump}
                counts={counts}
                caps={caps}
                onCopyOtp={handleCopyOtp}
                completedReminderIds={completedReminderIds}
                onToggleReminderDone={handleToggleReminderDone}
                theme={theme}
                onToggleTheme={() => setTheme((t) => (t === 'dark' ? 'light' : 'dark'))}
                designStyle={designStyle}
                glassActive={glassActive}
                onChangeDesignStyle={(s) => {
                  setDesignStyle(s);
                  if (s === 'glass') setDegraded(!glassSupported() || prefersReducedTransparency());
                }}
                autoTranslateToEnglish={prefs.autoTranslateToEnglish}
                onToggleAutoTranslate={() =>
                  patchPrefs({ autoTranslateToEnglish: !prefs.autoTranslateToEnglish })
                }
                onLoadSample={import.meta.env.DEV ? handleLoadSample : undefined}
                isLoadingSample={isLoadingSample}
                onGoToBackup={() => setActiveTab('reminders')}
                nowMs={nowMs}
              />
            )}

            {activeTab === 'finance' &&
              (finance && budget ? (
                <FinanceView
                  summary={finance}
                  budgetStats={budget}
                  cycleConfig={cycleConfig}
                  onChangeCycleDay={handleChangeCycleDay}
                  onlyActiveCycle={onlyActiveCycle}
                  onToggleOnlyActiveCycle={setOnlyActiveCycle}
                  onChangeBudgetLimit={handleChangeBudgetLimit}
                  onOpenThread={handleJumpToThread}
                  nowMs={nowMs}
                />
              ) : (
                <main className="flex-1 flex items-center justify-center text-sm text-slate-500 dark:text-slate-400">
                  Loading finance...
                </main>
              ))}

            {activeTab === 'reminders' && (
              <RemindersView
                preferredCloudProvider={prefs.preferredCloudProvider}
                onChangeCloudProvider={(p) => patchPrefs({ preferredCloudProvider: p })}
                lastBackupAtMs={prefs.lastBackupAtMs}
                onBackupSuccess={(t) => patchPrefs({ lastBackupAtMs: t })}
                onRestored={() => {
                  setSelectedThread(null);
                  getSettings()
                    .then((s) => setCycleConfig(s.cycleConfig))
                    .catch(() => {});
                  bump();
                }}
                readOnly={caps.readOnly}
                otps={otps}
                reminders={reminders}
                remindersLoading={remindersLoading}
                weeklyScan={scan}
                scanScheduleDay={prefs.scanScheduleDay}
                onChangeScanScheduleDay={(d) => patchPrefs({ scanScheduleDay: d })}
                onApplyCleanup={handleApplyCleanup}
                onPurgeExpiredOtps={handlePurgeExpiredOtps}
                onCopyOtp={handleCopyOtp}
                clipboardClearSecondsLeft={clipboardClearSecondsLeft}
                onToggleReminderDone={handleToggleReminderDone}
                onOpenThread={handleJumpToThread}
                nowMs={nowMs}
              />
            )}
          </m.div>

          {import.meta.env.DEV && SimulateSmsModal && simulateOpen && (
            <Suspense fallback={null}>
              <SimulateSmsModal
                isOpen={simulateOpen}
                onClose={() => setSimulateOpen(false)}
                onDeliver={handleDeliverSimulatedSms}
              />
            </Suspense>
          )}

          <NotificationBanner
            notification={latestNotification}
            onDismiss={() => setLatestNotification(null)}
            onOpenThread={handleJumpToThread}
            onCopyOtp={handleCopyOtp}
            onMarkRead={(id) => markRead(id).then(bump).catch(() => {})}
            onDeleteSms={(id) => deleteMessages([id]).then(bump).catch(() => {})}
            onQuickReply={caps.canSend ? handleQuickReply : undefined}
          />

          {notice && (
            <div
              role="status"
              className="fixed bottom-24 md:bottom-6 left-4 right-4 md:left-auto md:right-6 md:w-96 z-50 px-4 py-3 text-xs font-medium rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 shadow-lg"
            >
              {notice}
            </div>
          )}
        </div>
      </MotionConfig>
    </LazyMotion>
  );
}
