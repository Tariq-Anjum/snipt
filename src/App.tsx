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
  setBudgetPeriod,
  setCycle,
  setReminderDone,
  threadsPage,
  weeklyScan as fetchWeeklyScan,
  DEFAULT_BUDGET_PERIOD,
  DEFAULT_CYCLE_CONFIG,
  PENDING_CAPABILITIES,
  type BudgetPeriod,
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
import {
  acquireVaultSecure,
  copySensitive,
  importMissing,
  onNativeEvents,
  requestDefaultSmsRole,
  setSecure,
  setSystemBars,
  takeComposeDraft,
  updateWidgets,
  type ComposeDraft,
} from './api/telephony';
import { DEFAULT_BUDGET_LIMITS } from './engine/budget';
import { SettingsSheet, type ThemeMode } from './components/SettingsSheet';
import { installPinchTextScale, useTextScale } from './utils/textScale';
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

// Dev-only deep links for store screenshots (`?tab=finance&settings=1&open=1`);
// import.meta.env.DEV is false in release builds, so this is stripped there.
const DEV_SHOT = import.meta.env.DEV ? new URLSearchParams(window.location.search) : null;
if (DEV_SHOT?.get('shot') === '1') {
  // Capture tools paint only what is laid out: render every bubble.
  const style = document.createElement('style');
  style.textContent = '.msg-row{content-visibility:visible!important}*{scrollbar-width:none!important}';
  document.head.appendChild(style);
  // `&delay=<url>`: a slow 1px image holds the page's load event (which a
  // headless --screenshot waits for) until async views have rendered.
  const delay = DEV_SHOT.get('delay');
  if (delay) {
    const img = document.createElement('img');
    img.src = delay;
    img.style.cssText = 'position:fixed;width:1px;height:1px;opacity:0';
    document.body.appendChild(img);
  }
}
const devTab = (): ActiveTab => {
  const t = DEV_SHOT?.get('tab');
  return t === 'finance' || t === 'reminders' ? t : 'inbox';
};

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
  /** Android: FLAG_SECURE on every screen (S13 global option). */
  secureScreen: boolean;
  /** Dark mode uses true black (OLED) instead of navy. */
  pureBlack: boolean;
  /** Android widget shows message text (off = sender names only). */
  widgetPreviews: boolean;
}

const DEFAULT_PREFS: Prefs = {
  scanScheduleDay: 'Every Sunday · 9:00 AM',
  autoTranslateToEnglish: true,
  preferredCloudProvider: 'device_picker',
  lastBackupAtMs: null,
  secureScreen: false,
  pureBlack: true,
  widgetPreviews: true,
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
        secureScreen: p.secureScreen === true,
        pureBlack: p.pureBlack ?? DEFAULT_PREFS.pureBlack,
        widgetPreviews: p.widgetPreviews ?? DEFAULT_PREFS.widgetPreviews,
      };
    }
  } catch {
    // ignore malformed storage
  }
  return DEFAULT_PREFS;
}

// Theme follows the system unless the user picked one. v1 stored only an
// explicit 'light'/'dark' (defaulting to light), so a stored v1 value is
// ignored: everyone starts on 'system' once.
const THEME_MODE_KEY = 'snipt-theme-mode-v2';

function getInitialThemeMode(): ThemeMode {
  try {
    const saved = window.localStorage.getItem(THEME_MODE_KEY);
    if (saved === 'system' || saved === 'dark' || saved === 'light') return saved;
  } catch {
    // ignore
  }
  return 'system';
}

function systemDark(): boolean {
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  } catch {
    return false;
  }
}

function useSystemDark(): boolean {
  const [dark, setDark] = useState(systemDark);
  useEffect(() => {
    let mq: MediaQueryList;
    try {
      mq = window.matchMedia('(prefers-color-scheme: dark)');
    } catch {
      return;
    }
    const on = () => setDark(mq.matches);
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, []);
  return dark;
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
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => getInitialThemeMode());
  const sysDark = useSystemDark();
  const theme: 'light' | 'dark' =
    themeMode === 'system' ? (sysDark ? 'dark' : 'light') : themeMode;
  const [settingsOpen, setSettingsOpen] = useState(() => DEV_SHOT?.get('settings') === '1');
  const [budgetPeriod, setBudgetPeriodState] = useState<BudgetPeriod>(DEFAULT_BUDGET_PERIOD);
  const textScale = useTextScale();
  const [designStyle, setDesignStyle] = useState<DesignStyle>(() => getInitialDesignStyle());
  const [degraded, setDegraded] = useState(
    () => !glassSupported() || prefersReducedTransparency()
  );
  const [notice, setNotice] = useState<string | null>(null);
  const [caps, setCaps] = useState<Capabilities>(PENDING_CAPABILITIES);
  const platform = caps.platform;

  const [version, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  const [activeTab, setActiveTab] = useState<ActiveTab>(devTab);
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
  // A failed capabilities call keeps the fail-closed 'pending' platform, so
  // retry with backoff and then offer a manual retry (R5 optional) instead
  // of leaving vault and backup controls disabled with no explanation.
  const [capsError, setCapsError] = useState(false);
  const capsAttempt = useRef(0);
  const refreshCaps = useCallback(() => {
    capabilities()
      .then((c) => {
        capsAttempt.current = 0;
        setCapsError(false);
        setCaps(c);
      })
      .catch(() => {
        const n = ++capsAttempt.current;
        if (n <= 3) window.setTimeout(refreshCaps, 500 * 3 ** (n - 1));
        else setCapsError(true);
      });
  }, []);
  useEffect(refreshCaps, [refreshCaps]);

  // Android: receivers / notification actions change the store with the UI
  // open; a draft from another app (SENDTO) opens the compose sheet.
  const [draft, setDraft] = useState<ComposeDraft | null>(null);
  useEffect(() => {
    if (platform !== 'android') return;
    const pull = () =>
      takeComposeDraft(platform).then(({ draft: d, tab }) => {
        if (d) setDraft(d);
        if (tab) {
          setSelectedThread(null);
          setActiveTab(tab);
        }
      });
    pull();
    return onNativeEvents({
      'sms-received': bump,
      'sms-changed': bump,
      'send-status': bump,
      compose: pull,
    });
  }, [platform, bump]);

  // FLAG_SECURE while the OTP vault screen is showing. Its reveal controls
  // stay disabled until the native flag is confirmed (vaultSecure), and
  // nothing is acquired while the platform is still pending (fail closed:
  // the startup placeholder never counts as a confirmed platform).
  const [vaultSecure, setVaultSecure] = useState(false);
  useEffect(() => {
    setVaultSecure(false);
    if (activeTab !== 'reminders' || platform === 'pending') return;
    let release: (() => void) | null = null;
    let live = true;
    acquireVaultSecure(platform).then((r) => {
      if (!live) {
        r?.();
        return;
      }
      release = r;
      setVaultSecure(r !== null);
      if (r === null) setNotice('Screen protection could not be turned on, so codes stay hidden.');
    });
    return () => {
      live = false;
      release?.();
    };
  }, [platform, activeTab]);

  // Global FLAG_SECURE preference (Android); re-applied on start.
  useEffect(() => {
    if (platform !== 'android') return;
    setSecure(platform, 'global', prefs.secureScreen).then((on) => {
      if (prefs.secureScreen && !on) setNotice('Screenshot blocking could not be turned on.');
    });
  }, [platform, prefs.secureScreen]);

  const handleRequestRole = async () => {
    try {
      await requestDefaultSmsRole(platform);
    } catch {
      setNotice('Snipt could not open the default SMS app setting.');
    }
    refreshCaps();
    bump();
  };

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
  useEffect(() => installPinchTextScale(), []);
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
    const root = document.documentElement;
    root.classList.toggle('dark', theme === 'dark');
    root.classList.toggle('amoled', prefs.pureBlack);
    root.style.colorScheme = theme;
    // Status/navigation bar icons must contrast with the app background.
    void setSystemBars(platform, theme === 'light');
  }, [theme, prefs.pureBlack, platform]);

  useEffect(() => {
    try {
      window.localStorage.setItem(THEME_MODE_KEY, themeMode);
      window.localStorage.removeItem(THEME_KEY);
    } catch {
      // ignore
    }
  }, [themeMode]);

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

  // 30-second OTP clipboard countdown, shown only after a native Android
  // copy: the native side wipes exactly that clip (ownership token). The
  // web/Linux clipboard exposes text only, which cannot prove the clip is
  // still ours (the same text copied again elsewhere is indistinguishable),
  // so that path never auto-clears and tells the user instead.
  useEffect(() => {
    if (clipboardClearSecondsLeft === null) return;
    if (clipboardClearSecondsLeft <= 0) {
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
      .then((s) => {
        setCycleConfig(s.cycleConfig);
        if (s.budgetPeriod) setBudgetPeriodState(s.budgetPeriod);
      })
      .catch(() => {});
  }, []);

  // Dev screenshots: open the first conversation (`?open=1`).
  useEffect(() => {
    if (DEV_SHOT?.get('open') !== '1') return;
    threadsPage('All', 'all', null, 3)
      .then((p) => p.threads[Number(DEV_SHOT.get('idx') ?? 0)] && setSelectedThread(p.threads[Number(DEV_SHOT.get('idx') ?? 0)]))
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

  /** Resolves true only when the code really reached the clipboard. */
  const handleCopyOtp = async (code: string): Promise<boolean> => {
    if (platform === 'pending') {
      setNotice('Snipt is still starting. Try again in a moment.');
      return false;
    }
    const native = await copySensitive(platform, code);
    if (native === 'ok') {
      setClipboardClearSecondsLeft(30);
      return true;
    }
    if (native === 'failed') {
      setNotice('Could not copy the code.');
      return false;
    }
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      setNotice('Could not copy the code.');
      return false;
    }
    setNotice(
      'Code copied. This system cannot confirm the clipboard is still Snipt\'s, so it will not clear it automatically: clear it yourself after use.'
    );
    return true;
  };

  const handleMarkRead = (id: number) =>
    markRead(id)
      .then(bump)
      .catch((e: unknown) => {
        setNotice(e instanceof Error ? e.message : String(e));
        bump();
      });

  const handleDeleteOne = (id: number) =>
    deleteMessages([id])
      .then((r) => {
        if (r.failedIds.length > 0)
          setNotice('Could not delete the message from the system SMS store; it is kept. Try again.');
        bump();
      })
      .catch(bump);

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

  const handleChangeBudgetPeriod = (next: BudgetPeriod) => {
    const prev = budgetPeriod;
    setBudgetPeriodState(next);
    setBudgetPeriod(next)
      .then((saved) => {
        setBudgetPeriodState(saved);
        bump();
      })
      .catch(() => {
        setBudgetPeriodState(prev);
        setNotice('Could not save the budget period.');
      });
  };

  const handleRestoreDefaultLimits = () => {
    window.clearTimeout(budgetTimer.current);
    setBudget({ ...DEFAULT_BUDGET_LIMITS })
      .then(bump)
      .catch(() => setNotice('Could not restore the default limits.'));
  };

  // Home-screen widgets (Android): masked, OTP-free snapshot after data
  // changes, throttled. The budget is the current budget month.
  useEffect(() => {
    if (platform !== 'android') return;
    const id = window.setTimeout(() => {
      Promise.all([fetchBudgetStats(true), threadsPage('All', 'all', null, 6), fetchInboxCounts()])
        .then(([b, page, c]) => {
          const latest = page.threads
            .filter((t) => t.classification.category !== 'Spam')
            .slice(0, 3)
            .map((t) => ({
              title: t.displayName,
              body: t.latestHasOtp ? 'Verification code' : t.preview,
              ts: t.latestTs,
            }));
          const month = new Intl.DateTimeFormat('en-IN', { month: 'long' }).format(new Date());
          const cats = [...b.categories]
            .sort((x, y) => y.spent - x.spent)
            .slice(0, 3)
            .map((x) => ({ name: x.category, spent: x.spent, limit: x.limit }));
          return updateWidgets(platform, {
            v: 1,
            updatedAt: Date.now(),
            previews: prefs.widgetPreviews,
            unread: c.unread,
            latest,
            budget: { label: `${month} budget`, spent: b.totalSpent, limit: b.totalBudget, categories: cats },
          });
        })
        .catch(() => {});
    }, 1200);
    return () => window.clearTimeout(id);
  }, [platform, version, prefs.widgetPreviews]);

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
              className="ambient-canvas pointer-events-none fixed inset-0 -z-10 overflow-hidden transition-colors"
            >
              <div
                className="liquid-ambient-a absolute -top-28 -left-24 w-[58vw] h-[58vw] max-w-[780px] max-h-[780px] rounded-full opacity-85 dark:opacity-40 blur-3xl"
                style={{
                  background:
                    theme === 'dark'
                      ? 'radial-gradient(circle, rgba(37,99,235,0.42) 0%, rgba(14,116,144,0.18) 52%, transparent 74%)'
                      : 'radial-gradient(circle, rgba(125,211,252,0.62) 0%, rgba(191,219,254,0.42) 52%, transparent 74%)',
                }}
              />
              <div
                className="liquid-ambient-b absolute -bottom-32 -right-24 w-[62vw] h-[62vw] max-w-[840px] max-h-[840px] rounded-full opacity-80 dark:opacity-35 blur-3xl"
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
            onOpenSimulate={
              import.meta.env.DEV && !DEV_SHOT?.get('shot') ? () => setSimulateOpen(true) : undefined
            }
            unreadCount={counts?.unread ?? 0}
            pendingReminderCount={pendingRemindersCount}
            designStyle={glassActive ? 'glass' : 'solid'}
            onOpenSettings={() => setSettingsOpen(true)}
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
                onToggleTheme={() => setThemeMode(theme === 'dark' ? 'light' : 'dark')}
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
                secureScreen={platform === 'android' ? prefs.secureScreen : undefined}
                onToggleSecureScreen={() => patchPrefs({ secureScreen: !prefs.secureScreen })}
                onLoadSample={import.meta.env.DEV ? handleLoadSample : undefined}
                isLoadingSample={isLoadingSample}
                onGoToBackup={() => setActiveTab('reminders')}
                nowMs={nowMs}
                textScale={textScale}
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
                  budgetPeriod={budgetPeriod}
                  onChangeBudgetPeriod={handleChangeBudgetPeriod}
                  onRestoreDefaultLimits={handleRestoreDefaultLimits}
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
                nativePicker={platform === 'pending' ? null : caps.nativePicker}
                onImportMissing={
                  platform === 'android'
                    ? async () => {
                        const n = await importMissing(platform);
                        bump();
                        return n;
                      }
                    : undefined
                }
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
                revealEnabled={vaultSecure}
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
            onMarkRead={handleMarkRead}
            onDeleteSms={handleDeleteOne}
            onQuickReply={caps.canSend ? handleQuickReply : undefined}
          />

          {capsError && (
            <div
              role="alert"
              className="fixed top-[calc(var(--inset-top,0px)+3.5rem)] left-4 right-4 md:left-auto md:right-6 md:w-96 z-40 px-4 py-3 text-xs rounded-xl border border-red-200 dark:border-red-900 bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 shadow-lg"
            >
              <p className="font-semibold">Snipt could not finish starting</p>
              <p className="mt-1 text-slate-600 dark:text-slate-300">
                The code vault and backup stay locked until it does.
              </p>
              <button
                type="button"
                onClick={() => {
                  capsAttempt.current = 0;
                  setCapsError(false);
                  refreshCaps();
                }}
                className="mt-2 px-3 py-1.5 rounded-lg bg-slate-900 text-white dark:bg-white dark:text-slate-900 font-semibold"
              >
                Retry
              </button>
            </div>
          )}

          {platform === 'android' && !caps.defaultSmsRole && (
            <div
              role="region"
              aria-label="Default SMS app"
              className="fixed top-[calc(var(--inset-top,0px)+3.5rem)] left-4 right-4 md:left-auto md:right-6 md:w-96 z-40 px-4 py-3 text-xs rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 shadow-lg"
            >
              <p className="font-semibold">Make Snipt your SMS app</p>
              <p className="mt-1 text-slate-600 dark:text-slate-300">
                Android lets only the default SMS app receive, send and delete messages. RCS
                chats stay in Google Messages and end there when you switch; MMS is not supported
                yet.
              </p>
              <button
                type="button"
                onClick={handleRequestRole}
                className="mt-2 px-3 py-1.5 rounded-lg bg-slate-900 text-white dark:bg-white dark:text-slate-900 font-semibold"
              >
                Set as default
              </button>
            </div>
          )}

          {settingsOpen && (
            <SettingsSheet
              onClose={() => setSettingsOpen(false)}
              themeMode={themeMode}
              onChangeThemeMode={setThemeMode}
              resolvedTheme={theme}
              pureBlack={prefs.pureBlack}
              onTogglePureBlack={() => patchPrefs({ pureBlack: !prefs.pureBlack })}
              designStyle={designStyle}
              glassActive={glassActive}
              onChangeDesignStyle={(s) => {
                setDesignStyle(s);
                if (s === 'glass') setDegraded(!glassSupported() || prefersReducedTransparency());
              }}
              autoTranslate={prefs.autoTranslateToEnglish}
              onToggleAutoTranslate={() =>
                patchPrefs({ autoTranslateToEnglish: !prefs.autoTranslateToEnglish })
              }
              secureScreen={platform === 'android' ? prefs.secureScreen : undefined}
              onToggleSecureScreen={() => patchPrefs({ secureScreen: !prefs.secureScreen })}
              widgetPreviews={platform === 'android' ? prefs.widgetPreviews : undefined}
              onToggleWidgetPreviews={() => patchPrefs({ widgetPreviews: !prefs.widgetPreviews })}
              defaultSmsRole={platform === 'android' ? caps.defaultSmsRole : undefined}
              onRequestRole={handleRequestRole}
              onOpenFinance={() => {
                setSettingsOpen(false);
                setSelectedThread(null);
                setActiveTab('finance');
              }}
            />
          )}

          {draft && (
            <DraftSheet
              draft={draft}
              canSend={caps.canSend}
              onCancel={() => setDraft(null)}
              onSend={async (address, body) => {
                await queueOutgoing({ address, body, simSlot: 0 });
                setDraft(null);
                bump();
              }}
            />
          )}

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

function DraftSheet({
  draft,
  canSend,
  onCancel,
  onSend,
}: {
  draft: ComposeDraft;
  canSend: boolean;
  onCancel: () => void;
  onSend: (address: string, body: string) => Promise<void>;
}) {
  const [address, setAddress] = useState(draft.address);
  const [body, setBody] = useState(draft.body);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useBackLayer(true, onCancel);
  const ready = canSend && address.trim() !== '' && body.trim() !== '' && !busy;
  return (
    <div
      role="dialog"
      aria-label="New message"
      className="fixed inset-x-4 bottom-[calc(var(--inset-bottom,0px)+1rem)] md:left-auto md:right-6 md:w-96 z-50 p-4 rounded-2xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-slate-800 dark:text-slate-100 shadow-xl text-sm"
    >
      <p className="font-semibold mb-2">New message</p>
      <input
        aria-label="To"
        value={address}
        onChange={(e) => setAddress(e.target.value)}
        maxLength={128}
        inputMode="tel"
        placeholder="To"
        className="w-full mb-2 px-3 py-2 rounded-lg border border-slate-200 dark:border-slate-700 bg-transparent"
      />
      <textarea
        aria-label="Message"
        value={body}
        onChange={(e) => setBody(e.target.value)}
        rows={3}
        placeholder="Message"
        className="w-full px-3 py-2 rounded-lg border border-slate-200 dark:border-slate-700 bg-transparent"
      />
      {error && <p className="mt-1 text-xs text-red-600">{error}</p>}
      <div className="mt-2 flex justify-end gap-2">
        <button type="button" onClick={onCancel} className="px-3 py-1.5 rounded-lg">
          Cancel
        </button>
        <button
          type="button"
          disabled={!ready}
          onClick={() => {
            setBusy(true);
            onSend(address.trim(), body).catch(() => {
              setError('Could not send this message.');
              setBusy(false);
            });
          }}
          className="px-3 py-1.5 rounded-lg bg-slate-900 text-white dark:bg-white dark:text-slate-900 font-semibold disabled:opacity-40"
        >
          Send
        </button>
      </div>
    </div>
  );
}
