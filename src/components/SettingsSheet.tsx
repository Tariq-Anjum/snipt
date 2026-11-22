import { Monitor, Moon, Sun, X } from 'lucide-react';
import type { ReactNode } from 'react';
import { useBackLayer } from '../utils/backStack';
import {
  TEXT_SCALE_MAX,
  TEXT_SCALE_MIN,
  setTextScale,
  useTextScale,
} from '../utils/textScale';
import type { DesignStyle } from './TopBar';

export type ThemeMode = 'system' | 'light' | 'dark';

interface SettingsSheetProps {
  onClose: () => void;
  themeMode: ThemeMode;
  onChangeThemeMode: (m: ThemeMode) => void;
  resolvedTheme: 'light' | 'dark';
  pureBlack: boolean;
  onTogglePureBlack: () => void;
  designStyle: DesignStyle;
  glassActive: boolean;
  onChangeDesignStyle: (s: DesignStyle) => void;
  autoTranslate: boolean;
  onToggleAutoTranslate: () => void;
  /** Android only (undefined hides the rows). */
  secureScreen?: boolean;
  onToggleSecureScreen: () => void;
  widgetPreviews?: boolean;
  onToggleWidgetPreviews: () => void;
  defaultSmsRole?: boolean;
  onRequestRole: () => void;
  onOpenFinance: () => void;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-1">
      <h3 className="px-1 text-[11px] font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
        {title}
      </h3>
      <div className="rounded-2xl border border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/60 divide-y divide-slate-200 dark:divide-slate-800">
        {children}
      </div>
    </section>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 px-4 py-3 min-h-[56px]">
      <div className="min-w-0">
        <p className="text-sm font-medium text-slate-900 dark:text-slate-100">{label}</p>
        {hint && <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Switch({ on, onToggle, label }: { on: boolean; onToggle: () => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={onToggle}
      className={`relative inline-flex h-7 w-12 items-center rounded-full transition-colors ${
        on ? 'bg-blue-600' : 'bg-slate-300 dark:bg-slate-700'
      }`}
    >
      <span
        className={`inline-block h-5 w-5 rounded-full bg-white shadow transition-transform ${
          on ? 'translate-x-6' : 'translate-x-1'
        }`}
      />
    </button>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: Array<{ id: T; label: string; icon?: ReactNode }>;
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="grid gap-1 p-1 rounded-xl bg-slate-200/70 dark:bg-slate-800" style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}>
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={value === o.id}
          onClick={() => onChange(o.id)}
          className={`inline-flex items-center justify-center gap-1.5 px-2 py-2 text-xs font-semibold rounded-lg transition-colors min-h-[40px] ${
            value === o.id
              ? 'bg-white text-slate-900 shadow-xs dark:bg-slate-900 dark:text-white'
              : 'text-slate-600 dark:text-slate-300'
          }`}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SettingsSheet(p: SettingsSheetProps) {
  useBackLayer(true, p.onClose);
  const scale = useTextScale();

  return (
    <div className="fixed inset-0 z-[60] flex items-end md:items-center justify-center" role="presentation">
      <button
        type="button"
        aria-label="Close settings"
        onClick={p.onClose}
        className="absolute inset-0 bg-black/50"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="sheet-enter relative w-full md:max-w-lg max-h-[88dvh] overflow-y-auto overscroll-contain rounded-t-3xl md:rounded-3xl border border-slate-200 dark:border-slate-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl px-4 pt-3 pb-[calc(var(--inset-bottom,0px)+1.25rem)]"
      >
        <div className="mx-auto mb-2 h-1.5 w-10 rounded-full bg-slate-300 dark:bg-slate-700 md:hidden" aria-hidden="true" />
        <div className="flex items-center justify-between mb-3">
          <h2 className="font-display text-lg font-bold text-slate-900 dark:text-white">Settings</h2>
          <button
            type="button"
            onClick={p.onClose}
            aria-label="Close settings"
            className="min-w-[44px] min-h-[44px] -mr-2 flex items-center justify-center rounded-full text-slate-500 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
          >
            <X className="w-5 h-5" aria-hidden="true" />
          </button>
        </div>

        <div className="space-y-5">
          <Section title="Appearance">
            <div className="px-4 py-3 space-y-2">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">Theme</p>
              <Segmented
                label="Theme"
                value={p.themeMode}
                onChange={p.onChangeThemeMode}
                options={[
                  { id: 'system', label: 'System', icon: <Monitor className="w-3.5 h-3.5" aria-hidden="true" /> },
                  { id: 'light', label: 'Light', icon: <Sun className="w-3.5 h-3.5" aria-hidden="true" /> },
                  { id: 'dark', label: 'Dark', icon: <Moon className="w-3.5 h-3.5" aria-hidden="true" /> },
                ]}
              />
            </div>
            <Row label="Pure black" hint="True black background in dark mode (OLED), higher contrast">
              <Switch on={p.pureBlack} onToggle={p.onTogglePureBlack} label="Pure black dark mode" />
            </Row>
            <div className="px-4 py-3 space-y-2">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">Surface</p>
              <Segmented
                label="Interface surface"
                value={p.designStyle}
                onChange={p.onChangeDesignStyle}
                options={[
                  { id: 'glass', label: 'Liquid Glass' },
                  { id: 'solid', label: 'Crisp Solid' },
                ]}
              />
              {p.designStyle === 'glass' && !p.glassActive && (
                <p className="text-xs text-amber-700 dark:text-amber-400">
                  Crisp Solid is on for this session (performance or power saver).
                </p>
              )}
            </div>
            <div className="px-4 py-3 space-y-2">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium text-slate-900 dark:text-slate-100">Text size</p>
                <button
                  type="button"
                  onClick={() => setTextScale(1)}
                  className="text-xs font-semibold text-blue-700 dark:text-blue-400 disabled:opacity-40"
                  disabled={scale === 1}
                >
                  Reset
                </button>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-xs text-slate-500 dark:text-slate-400" aria-hidden="true">A</span>
                <input
                  type="range"
                  min={TEXT_SCALE_MIN}
                  max={TEXT_SCALE_MAX}
                  step={0.05}
                  value={scale}
                  onChange={(e) => setTextScale(Number(e.target.value))}
                  aria-label="Message text size"
                  className="flex-1 accent-blue-600"
                />
                <span className="text-lg text-slate-500 dark:text-slate-400" aria-hidden="true">A</span>
              </div>
              <p className="ts-body text-slate-700 dark:text-slate-200">
                Your OTP for login is •••••• — preview at {Math.round(scale * 100)}%.
              </p>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Tip: pinch with two fingers in the inbox or a conversation.
              </p>
            </div>
          </Section>

          <Section title="Messages">
            <Row label="Translate to English" hint="Offline · Hindi, Urdu, Hinglish and more">
              <Switch on={p.autoTranslate} onToggle={p.onToggleAutoTranslate} label="Translate to English" />
            </Row>
            {p.defaultSmsRole !== undefined && (
              <Row
                label="Default SMS app"
                hint={p.defaultSmsRole ? 'Snipt receives and sends your SMS' : 'Another app is the default'}
              >
                {p.defaultSmsRole ? (
                  <span className="text-xs font-semibold text-emerald-700 dark:text-emerald-400">Active</span>
                ) : (
                  <button
                    type="button"
                    onClick={p.onRequestRole}
                    className="px-3 py-2 text-xs font-semibold rounded-lg bg-blue-600 text-white"
                  >
                    Set default
                  </button>
                )}
              </Row>
            )}
          </Section>

          <Section title="Finance & budget">
            <Row label="Budget period, reset and limits" hint="Monthly reset day, start fresh, default limits">
              <button
                type="button"
                onClick={p.onOpenFinance}
                className="px-3 py-2 text-xs font-semibold rounded-lg bg-slate-200 text-slate-800 dark:bg-slate-800 dark:text-slate-100"
              >
                Open
              </button>
            </Row>
          </Section>

          {(p.secureScreen !== undefined || p.widgetPreviews !== undefined) && (
            <Section title="Privacy">
              {p.secureScreen !== undefined && (
                <Row label="Block screenshots" hint="Every screen; the code vault is always protected">
                  <Switch on={p.secureScreen} onToggle={p.onToggleSecureScreen} label="Block screenshots" />
                </Row>
              )}
              {p.widgetPreviews !== undefined && (
                <Row label="Message text in widget" hint="Off shows sender names only; codes are never shown">
                  <Switch
                    on={p.widgetPreviews}
                    onToggle={p.onToggleWidgetPreviews}
                    label="Show message text in the home-screen widget"
                  />
                </Row>
              )}
            </Section>
          )}

          <p className="px-1 text-center text-[11px] text-slate-500 dark:text-slate-400">
            Snipt works offline. No internet permission.
          </p>
        </div>
      </div>
    </div>
  );
}
