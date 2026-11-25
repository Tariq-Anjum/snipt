import {
  Bell,
  Inbox,
  IndianRupee,
  PenSquare,
  Settings,
  TrendingUp,
} from 'lucide-react';
import { m } from 'motion/react';
import { SniptGlassIcon } from './SniptGlassIcon';

export type ActiveTab = 'inbox' | 'finance' | 'stocks' | 'reminders';
export type DesignStyle = 'glass' | 'solid';
export type ColorPalette = 'glass-os' | 'emerald' | 'warm-sand' | 'midnight' | 'monochrome';

interface TopBarProps {
  activeTab: ActiveTab;
  onSelectTab: (tab: ActiveTab) => void;
  onOpenNewMessage?: () => void;
  unreadCount: number;
  pendingReminderCount: number;
  designStyle: DesignStyle;
  onOpenSettings: () => void;
}

const NAV_ITEMS: Array<{
  id: ActiveTab;
  label: string;
  mobileLabel: string;
  icon: typeof Inbox;
}> = [
  { id: 'inbox', label: 'Inbox', mobileLabel: 'Inbox', icon: Inbox },
  { id: 'finance', label: 'Finance', mobileLabel: 'Finance', icon: IndianRupee },
  { id: 'stocks', label: 'Stocks', mobileLabel: 'Stocks', icon: TrendingUp },
  { id: 'reminders', label: 'Vault & Backup', mobileLabel: 'Vault', icon: Bell },
];

export function TopBar({
  activeTab,
  onSelectTab,
  onOpenNewMessage,
  unreadCount,
  pendingReminderCount,
  designStyle,
  onOpenSettings,
}: TopBarProps) {
  return (
    <>
      <header className="sticky top-0 z-30 h-14 md:h-15 shrink-0 bg-white dark:bg-slate-900 glass-chrome border-b border-slate-200/80 dark:border-slate-800 px-4 md:px-6 flex items-center justify-between gap-8 transition-colors">
        {/* Zone 1: Signature italic 'snipt' wordmark + Liquid Glass icon */}
        <button
          type="button"
          onClick={() => onSelectTab('inbox')}
          aria-label="snipt - Snip the noise"
          title="Snip the noise"
          className="inline-flex items-center gap-2.5 whitespace-nowrap shrink-0 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-blue-600 rounded-xl"
        >
          <SniptGlassIcon
            size={30}
            variant={designStyle}
            className="shrink-0 drop-shadow-xs"
          />
          <span className="font-brand-italic text-[1.6rem] leading-none font-extrabold tracking-tight text-slate-950 dark:text-white pr-0.5">
            snipt
          </span>
        </button>

        {/* Zone 2: 4 clean single-line navigation links */}
        <nav
          aria-label="Primary navigation"
          className="hidden md:flex items-center gap-7"
        >
          {NAV_ITEMS.map((item) => {
            const isActive = activeTab === item.id;
            const count =
              item.id === 'inbox'
                ? unreadCount
                : item.id === 'reminders'
                  ? pendingReminderCount
                  : 0;

            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onSelectTab(item.id)}
                aria-current={isActive ? 'page' : undefined}
                className={`relative h-15 inline-flex items-center gap-1.5 text-sm font-medium transition-colors whitespace-nowrap shrink-0 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-blue-600 ${
                  isActive
                    ? 'snipt-accent-text font-semibold'
                    : 'text-slate-600 hover:text-slate-950 dark:text-slate-400 dark:hover:text-slate-100'
                }`}
              >
                <span>{item.label}</span>
                {count > 0 && (
                  <span className="font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
                    ({count})
                  </span>
                )}
                {isActive && (
                  <m.span
                    layoutId="snipt-nav-indicator"
                    transition={{ type: 'spring', stiffness: 520, damping: 38 }}
                    aria-hidden="true"
                    className="absolute bottom-0 left-0 right-0 h-0.5 rounded-full snipt-accent-bg"
                  />
                )}
              </button>
            );
          })}
        </nav>

        {/* Zone 3: Settings + New Message CTA */}
        <div className="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={onOpenSettings}
            aria-label="Settings"
            className="min-w-[40px] min-h-[40px] flex items-center justify-center rounded-full text-slate-600 hover:text-slate-950 hover:bg-white/60 dark:text-slate-300 dark:hover:text-white dark:hover:bg-slate-800 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600"
          >
            <Settings className="w-5 h-5" aria-hidden="true" />
          </button>
          {onOpenNewMessage && (
            <m.button
              type="button"
              whileTap={{ scale: 0.97 }}
              transition={{ type: 'spring', stiffness: 560, damping: 36 }}
              onClick={onOpenNewMessage}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold text-white snipt-accent-bg snipt-accent-on-solid glass-cta rounded-xl transition-colors whitespace-nowrap shrink-0 min-h-[38px] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600"
            >
              <PenSquare className="w-3.5 h-3.5" aria-hidden="true" />
              <span>New SMS</span>
            </m.button>
          )}
        </div>
      </header>

      {/* Mobile Bottom Tab Bar (Thumb Zone, compact <= 15% of 844px) */}
      <nav
        aria-label="Mobile navigation"
        className="md:hidden fixed bottom-0 left-0 right-0 z-40 min-h-15 safe-bottom bg-white dark:bg-slate-900 glass-chrome glass-dock border-t border-slate-200 dark:border-slate-800 grid grid-cols-4 items-center px-1"
      >
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const isActive = activeTab === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelectTab(item.id)}
              aria-current={isActive ? 'page' : undefined}
              className={`relative min-h-[48px] flex flex-col items-center justify-center gap-1 rounded-xl transition-colors ${
                isActive
                  ? 'snipt-accent-text font-semibold'
                  : 'text-slate-500 dark:text-slate-400'
              }`}
            >
              <Icon className="w-5 h-5" aria-hidden="true" />
              <span className="text-[11px] font-medium leading-none whitespace-nowrap">
                {item.mobileLabel}
              </span>
            </button>
          );
        })}
      </nav>
    </>
  );
}
