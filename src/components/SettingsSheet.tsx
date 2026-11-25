import { AlertTriangle, Check, Monitor, Moon, ShieldCheck, Sun, Users, X } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useBackLayer } from '../utils/backStack';
import {
  TEXT_SCALE_MAX,
  TEXT_SCALE_MIN,
  setTextScale,
  useTextScale,
} from '../utils/textScale';
import type { ColorPalette, DesignStyle } from './TopBar';

export type ThemeMode = 'system' | 'light' | 'dark';
export type OperatingMode = 'companion' | 'default_sms';

const PALETTES: Array<{ id: ColorPalette; label: string; swatch: string; desc: string }> = [
  { id: 'glass-os', label: 'Glass OS', swatch: '#0047ff', desc: 'Ice blue & cobalt' },
  { id: 'emerald', label: 'Emerald', swatch: '#059669', desc: 'Fintech mint' },
  { id: 'warm-sand', label: 'Warm Sand', swatch: '#d97706', desc: 'Editorial blush' },
  { id: 'midnight', label: 'Midnight', swatch: '#4f46e5', desc: 'Deep iris' },
  { id: 'monochrome', label: 'Ink Mono', swatch: '#0d0d0f', desc: 'Pure graphite' },
];

interface SettingsSheetProps {
  onClose: () => void;
  themeMode: ThemeMode;
  onChangeThemeMode: (m: ThemeMode) => void;
  resolvedTheme: 'light' | 'dark';
  colorPalette: ColorPalette;
  onChangeColorPalette: (p: ColorPalette) => void;
  pureBlack: boolean;
  onTogglePureBlack: () => void;
  designStyle: DesignStyle;
  glassActive: boolean;
  onChangeDesignStyle: (s: DesignStyle) => void;
  autoTranslate: boolean;
  onToggleAutoTranslate: () => void;
  operatingMode: OperatingMode;
  onChangeOperatingMode: (mode: OperatingMode) => void;
  contactsGranted: boolean;
  onToggleContactsGranted: () => void;
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
      <h3 className="px-1 font-mono text-[11px] font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
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
        on ? 'snipt-accent-bg' : 'bg-slate-300 dark:bg-slate-700'
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
    <div
      role="radiogroup"
      aria-label={label}
      className="grid gap-1 p-1 rounded-xl bg-slate-200/70 dark:bg-slate-800"
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
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
  const [confirmDefaultModal, setConfirmDefaultModal] = useState(false);

  const handleConfirmDefaultSms = () => {
    setConfirmDefaultModal(false);
    p.onChangeOperatingMode('default_sms');
    p.onRequestRole();
  };

  return (
    <div
      className="fixed inset-0 z-[60] flex items-end md:items-center justify-center"
      role="presentation"
    >
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
        <div
          className="mx-auto mb-2 h-1.5 w-10 rounded-full bg-slate-300 dark:bg-slate-700 md:hidden"
          aria-hidden="true"
        />
        <div className="flex items-center justify-between mb-3">
          <h2 className="font-display text-lg font-bold text-slate-900 dark:text-white">
            Settings
          </h2>
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
          {/* RCS & Messaging Mode */}
          <Section title="RCS & Messaging Mode">
            <div className="px-4 py-3.5 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                    {p.operatingMode === 'companion'
                      ? 'Companion Mode (RCS stays ON)'
                      : 'Default SMS App Mode'}
                  </p>
                  <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">
                    {p.operatingMode === 'companion'
                      ? 'Google Messages stays your default app so native E2E RCS keeps working. Snipt reads notifications and replies via RemoteInput.'
                      : 'Snipt handles SMS & MMS receive directly. Native Google Messages RCS is off while Snipt is default.'}
                  </p>
                </div>
                <ShieldCheck
                  className={`w-5 h-5 shrink-0 ${
                    p.operatingMode === 'companion'
                      ? 'text-emerald-600 dark:text-emerald-400'
                      : 'text-blue-600 dark:text-blue-400'
                  }`}
                  aria-hidden="true"
                />
              </div>
              <div className="flex flex-wrap items-center gap-2">
                {p.operatingMode === 'companion' ? (
                  <button
                    type="button"
                    onClick={() => setConfirmDefaultModal(true)}
                    className="px-3.5 py-2 text-xs font-semibold rounded-xl border border-slate-300 dark:border-slate-700 text-slate-800 dark:text-slate-100 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors"
                  >
                    Switch to Default SMS App...
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => p.onChangeOperatingMode('companion')}
                    className="px-3.5 py-2 text-xs font-semibold rounded-xl text-white snipt-accent-bg snipt-accent-on-solid transition-colors"
                  >
                    Use Companion Mode (Restore RCS)
                  </button>
                )}
              </div>
            </div>
            <Row
              label="Contacts permission"
              hint={
                p.contactsGranted
                  ? 'Saved contact names appear on threads and in New SMS'
                  : 'Off — phone numbers are shown instead of contact names'
              }
            >
              <Switch
                on={p.contactsGranted}
                onToggle={p.onToggleContactsGranted}
                label="Allow Contacts access"
              />
            </Row>
            <Row label="Translate to English" hint="Offline · Hindi, Urdu, Hinglish and more">
              <Switch
                on={p.autoTranslate}
                onToggle={p.onToggleAutoTranslate}
                label="Translate to English"
              />
            </Row>
          </Section>

          {/* Appearance & Themes */}
          <Section title="Themes & Surface">
            <div className="px-4 py-3 space-y-2.5">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">Color Theme</p>
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-2">
                {PALETTES.map((pal) => {
                  const active = p.colorPalette === pal.id;
                  return (
                    <button
                      key={pal.id}
                      type="button"
                      onClick={() => p.onChangeColorPalette(pal.id)}
                      aria-pressed={active}
                      className={`flex items-center gap-2.5 px-3 py-2 rounded-xl border text-left transition-all ${
                        active
                          ? 'border-blue-600 dark:border-blue-400 bg-white dark:bg-slate-900 shadow-xs'
                          : 'border-slate-200 dark:border-slate-800 hover:bg-white/60 dark:hover:bg-slate-900/50'
                      }`}
                    >
                      <span
                        className="w-5 h-5 rounded-full shrink-0 flex items-center justify-center border border-black/10"
                        style={{ backgroundColor: pal.swatch }}
                      >
                        {active && <Check className="w-3 h-3 text-white" aria-hidden="true" />}
                      </span>
                      <span className="min-w-0">
                        <span className="block text-xs font-semibold text-slate-900 dark:text-slate-100 truncate">
                          {pal.label}
                        </span>
                        <span className="block text-[10px] text-slate-500 dark:text-slate-400 truncate">
                          {pal.desc}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="px-4 py-3 space-y-2">
              <p className="text-sm font-medium text-slate-900 dark:text-slate-100">Brightness</p>
              <Segmented
                label="Theme"
                value={p.themeMode}
                onChange={p.onChangeThemeMode}
                options={[
                  {
                    id: 'system',
                    label: 'System',
                    icon: <Monitor className="w-3.5 h-3.5" aria-hidden="true" />,
                  },
                  {
                    id: 'light',
                    label: 'Light',
                    icon: <Sun className="w-3.5 h-3.5" aria-hidden="true" />,
                  },
                  {
                    id: 'dark',
                    label: 'Dark',
                    icon: <Moon className="w-3.5 h-3.5" aria-hidden="true" />,
                  },
                ]}
              />
            </div>

            <Row
              label="Pure black (OLED)"
              hint="True black background in dark mode with higher contrast"
            >
              <Switch
                on={p.pureBlack}
                onToggle={p.onTogglePureBlack}
                label="Pure black dark mode"
              />
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
                  className="text-xs font-semibold snipt-accent-text disabled:opacity-40"
                  disabled={scale === 1}
                >
                  Reset
                </button>
              </div>
              <div className="flex items-center gap-3">
                <span className="text-xs text-slate-500 dark:text-slate-400" aria-hidden="true">
                  A
                </span>
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
                <span className="text-lg text-slate-500 dark:text-slate-400" aria-hidden="true">
                  A
                </span>
              </div>
              <p className="ts-body text-slate-700 dark:text-slate-200">
                Your OTP for login is •••••• — preview at {Math.round(scale * 100)}%.
              </p>
            </div>
          </Section>

          <Section title="Finance & Budget">
            <Row
              label="Budget period, reset and limits"
              hint="Monthly reset day, start fresh, default limits"
            >
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
                <Row
                  label="Block screenshots"
                  hint="Every screen; the code vault is always protected"
                >
                  <Switch
                    on={p.secureScreen}
                    onToggle={p.onToggleSecureScreen}
                    label="Block screenshots"
                  />
                </Row>
              )}
              {p.widgetPreviews !== undefined && (
                <Row
                  label="Message text in widget"
                  hint="Off shows sender names only; codes are never shown"
                >
                  <Switch
                    on={p.widgetPreviews}
                    onToggle={p.onToggleWidgetPreviews}
                    label="Show message text in the home-screen widget"
                  />
                </Row>
              )}
            </Section>
          )}

          <p className="px-1 text-center text-[11px] text-slate-500 dark:text-slate-400 inline-flex items-center justify-center gap-1.5 w-full">
            <Users className="w-3.5 h-3.5" aria-hidden="true" />
            <span>snipt works 100% offline. Zero cloud accounts or internet permission.</span>
          </p>
        </div>
      </div>

      {/* Explicit Pop-Up Alert when choosing Default SMS App */}
      {confirmDefaultModal && (
        <div
          className="fixed inset-0 z-[70] flex items-center justify-center p-4"
          role="presentation"
        >
          <button
            type="button"
            aria-label="Cancel default SMS switch"
            onClick={() => setConfirmDefaultModal(false)}
            className="absolute inset-0 bg-black/60"
          />
          <div
            role="alertdialog"
            aria-modal="true"
            aria-label="RCS Warning"
            className="relative w-full max-w-md rounded-3xl border border-amber-300 dark:border-amber-800 bg-white dark:bg-slate-900 glass-sheet shadow-2xl p-6 space-y-4"
          >
            <div className="flex items-start gap-3">
              <span className="w-10 h-10 rounded-2xl bg-amber-100 dark:bg-amber-950/70 text-amber-700 dark:text-amber-400 flex items-center justify-center shrink-0">
                <AlertTriangle className="w-5 h-5" aria-hidden="true" />
              </span>
              <div>
                <h3 className="font-display text-base font-bold text-slate-900 dark:text-white">
                  RCS chats will be turned off
                </h3>
                <p className="mt-1.5 text-xs leading-relaxed text-slate-600 dark:text-slate-300">
                  Android only lets Google Messages run native Google Jibe RCS. Making{' '}
                  <strong>snipt</strong> your Default SMS app pauses RCS chats on this number
                  (switching back to Google Messages restores them). MMS receive is supported; MMS
                  send is not yet supported.
                </p>
                <p className="mt-2 text-xs font-medium text-slate-700 dark:text-slate-200">
                  Stay in <strong>Companion Mode</strong> if you want to keep Google Messages RCS
                  active while still using snipt’s offline finance, OTP vault, and translation.
                </p>
              </div>
            </div>
            <div className="flex flex-col sm:flex-row justify-end gap-2 pt-2">
              <button
                type="button"
                onClick={() => setConfirmDefaultModal(false)}
                className="px-4 py-2.5 text-xs font-semibold rounded-xl text-white snipt-accent-bg snipt-accent-on-solid"
              >
                Keep Companion Mode (RCS On)
              </button>
              <button
                type="button"
                onClick={handleConfirmDefaultSms}
                className="px-4 py-2.5 text-xs font-semibold rounded-xl border border-amber-300 dark:border-amber-700 text-amber-800 dark:text-amber-300 hover:bg-amber-50 dark:hover:bg-amber-950/40"
              >
                Turn Off RCS &amp; Set Default
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
