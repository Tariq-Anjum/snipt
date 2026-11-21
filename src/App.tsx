import { LazyMotion, MotionConfig, domAnimation, m } from 'motion/react';
import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import {
  applyCleanup,
  budgetStats as fetchBudgetStats,
  capabilities,
  deleteMessages,
  financeSummary as fetchFinanceSummary,
  isTauri,
  markRead,
  otpItems as fetchOtpItems,
  powerSaveActive,
  scanReminders,
  setBackHandler,
  setBudget,
  setCycle,
  threadFromMessages,
  threadsPage,
  weeklyScan as fetchWeeklyScan,
  DEFAULT_CYCLE_CONFIG,
  type BudgetCategoryId,
  type BudgetLimits,
  type BudgetPlannerSummary,
  type Capabilities,
  type CleanupBucketId,
  type CloudBackupProvider,
  type FinanceSummary,
  type InboxFilter,
  type OtpItemView,
  type ParsedSms,
  type PaymentCycleConfig,
  type PaymentMethod,
  type ReminderItem,
  type SimFilter,
  type ThreadRow,
  type WeeklyScanReport,
} from './api';
import { devImportSeed, devIngest } from './api/dev';
import { FinanceView } from './components/FinanceView';
import { InboxView } from './components/InboxView';
import { NotificationBanner } from './components/NotificationBanner';
import { RemindersView, type ReminderRow } from './components/RemindersView';
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

interface Prefs {
  completedReminderSmsIds: number[];
  cycleConfig: PaymentCycleConfig | null;
  scanScheduleDay: string;
  autoTranslateToEnglish: boolean;
  preferredCloudProvider: CloudBackupProvider;
  lastBackupAtMs: number | null;
}

const DEFAULT_PREFS: Prefs = {
  completedReminderSmsIds: [],
  cycleConfig: null,
  scanScheduleDay: 'Every Sunday · 9:00 AM',
  autoTranslateToEnglish: true,
  preferredCloudProvider: 'device_picker',
  lastBackupAtMs: null,
};

function loadPrefs(): Prefs {
  try {
    const raw = window.localStorage.getItem(PREFS_KEY);
    if (raw) return { ...DEFAULT_PREFS, ...(JSON.parse(raw) as Partial<Prefs>) };
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
  const [cycleConfig, setCycleConfig] = useState<PaymentCycleConfig>(
    () => prefs.cycleConfig ?? { ...DEFAULT_CYCLE_CONFIG }
  );
  const [onlyActiveCycle, setOnlyActiveCycle] = useState(true);
  const [theme, setTheme] = useState<'light' | 'dark'>(() => getInitialTheme());
  const [designStyle, setDesignStyle] = useState<DesignStyle>(() => getInitialDesignStyle());
  const [degraded, setDegraded] = useState(
    () => !glassSupported() || prefersReducedTransparency()
  );
  const [notice, setNotice] = useState<string | null>(null);
  const [platform, setPlatform] = useState<Capabilities['platform']>('linux');

  const [version, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  const [activeTab, setActiveTab] = useState<ActiveTab>('inbox');
  const [inboxFilter, setInboxFilter] = useState<InboxFilter>('All');
  const [simFilter, setSimFilter] = useState<SimFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const debouncedQuery = useDebounced(searchQuery, 150);
  const [selectedThread, setSelectedThread] = useState<ThreadRow | null>(null);
  const [simulateOpen, setSimulateOpen] = useState(false);
  const [latestNotification, setLatestNotification] = useState<ParsedSms | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [isLoadingSample, setIsLoadingSample] = useState(false);
  const [clipboardClearSecondsLeft, setClipboardClearSecondsLeft] = useState<number | null>(null);

  const [finance, setFinance] = useState<FinanceSummary | null>(null);
  const [budget, setBudgetState] = useState<BudgetPlannerSummary | null>(null);
  const [otps, setOtps] = useState<OtpItemView[]>([]);
  const [scan, setScan] = useState<WeeklyScanReport | null>(null);
  const [reminderItems, setReminderItems] = useState<ReminderItem[]>([]);
  const [remindersLoading, setRemindersLoading] = useState(true);
  const [unreadHint, setUnreadHint] = useState(0);

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
      .then((c) => setPlatform(c.platform))
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
  // Push a user-chosen billing cycle to the core once (no getter command yet).
  useEffect(() => {
    if (prefs.cycleConfig) setCycle(prefs.cycleConfig).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Dev-seed builds only (VITE_DEV_SEED=1): fill an empty database once.
  useEffect(() => {
    if (import.meta.env.VITE_DEV_SEED !== '1') return;
    threadsPage('All', 'all', null, 1)
      .then((p) => (p.threads.length === 0 ? devImportSeed().then(bump) : undefined))
      .catch(() => {});
  }, [bump]);

  // Unread badge: approximated from the newest 100 non-archived threads.
  useEffect(() => {
    threadsPage('All', 'all', null, 100)
      .then((p) => setUnreadHint(p.threads.reduce((sum, t) => sum + t.unreadCount, 0)))
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

  // Reminders feed the tab badge, so scan shortly after startup and on changes.
  useEffect(() => {
    let live = true;
    setRemindersLoading(true);
    const id = window.setTimeout(() => {
      scanReminders()
        .then((r) => live && setReminderItems(r))
        .catch(() => {})
        .finally(() => live && setRemindersLoading(false));
    }, 800);
    return () => {
      live = false;
      window.clearTimeout(id);
    };
  }, [version]);

  const doneSet = new Set(prefs.completedReminderSmsIds);
  const reminders: ReminderRow[] = reminderItems
    .map((r) => ({ ...r, isDone: doneSet.has(r.smsId) }))
    .sort((a, b) => Number(a.isDone) - Number(b.isDone) || b.timestampMs - a.timestampMs);
  const pendingRemindersCount = reminders.filter((r) => !r.isDone).length;

  // ---- handlers ---------------------------------------------------------------
  const handleJumpToThread = useCallback(async (threadId: number) => {
    setActiveTab('inbox');
    setInboxFilter('All');
    setSearchQuery('');
    const row = await threadFromMessages(threadId).catch(() => null);
    if (row) setSelectedThread(row);
  }, []);

  const handleToggleReminderDone = (smsId: number) => {
    const exists = prefs.completedReminderSmsIds.includes(smsId);
    patchPrefs({
      completedReminderSmsIds: exists
        ? prefs.completedReminderSmsIds.filter((id) => id !== smsId)
        : [...prefs.completedReminderSmsIds, smsId],
    });
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
    patchPrefs({ cycleConfig: next });
    setCycle(next)
      .then(bump)
      .catch(() => {});
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

  const handleDeliverSimulatedSms = (payload: Parameters<typeof devIngest>[0]) => {
    devIngest(payload)
      .then(async (parsed) => {
        setNowMs(Date.now());
        setLatestNotification(parsed);
        setActiveTab('inbox');
        setInboxFilter(parsed.classification.category === 'Spam' ? 'Spam' : 'All');
        bump();
        const row = await threadFromMessages(parsed.sms.threadId);
        if (row) setSelectedThread(row);
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
            unreadCount={unreadHint}
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
                onCopyOtp={handleCopyOtp}
                completedReminderIds={prefs.completedReminderSmsIds}
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
                  bump();
                }}
                otps={otps}
                reminders={reminders}
                remindersLoading={remindersLoading}
                weeklyScan={scan}
                scanScheduleDay={prefs.scanScheduleDay}
                onChangeScanScheduleDay={(d) => patchPrefs({ scanScheduleDay: d })}
                onApplyCleanup={handleApplyCleanup}
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
            parsed={latestNotification}
            onDismiss={() => setLatestNotification(null)}
            onOpenThread={handleJumpToThread}
            onCopyOtp={handleCopyOtp}
            onMarkRead={(id) => markRead(id).then(bump).catch(() => {})}
            onDeleteSms={(id) => deleteMessages([id]).then(bump).catch(() => {})}
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
