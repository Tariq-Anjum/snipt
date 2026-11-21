import { Bell, Inbox, IndianRupee, Plus } from 'lucide-react';
import { m } from 'motion/react';
import { SniptGlassIcon } from './SniptGlassIcon';

export type ActiveTab = 'inbox' | 'finance' | 'reminders';
export type DesignStyle = 'glass' | 'solid';

interface TopBarProps {
  activeTab: ActiveTab;
  onSelectTab: (tab: ActiveTab) => void;
  onOpenSimulate?: () => void;
  unreadCount: number;
  pendingReminderCount: number;
  designStyle: DesignStyle;
}

const NAV_ITEMS: Array<{
  id: ActiveTab;
  label: string;
  icon: typeof Inbox;
}> = [
  { id: 'inbox', label: 'Inbox', icon: Inbox },
  { id: 'finance', label: 'Finance', icon: IndianRupee },
  { id: 'reminders', label: 'Vault & Backup', icon: Bell },
];

export function TopBar({
  activeTab,
  onSelectTab,
  onOpenSimulate,
  unreadCount,
  pendingReminderCount,
  designStyle,
}: TopBarProps) {
  return (
    <>
      <header className="sticky top-0 z-30 h-13 md:h-14 shrink-0 bg-white dark:bg-slate-900 glass-chrome border-b border-slate-200 dark:border-slate-800 px-4 md:px-6 flex items-center justify-between gap-8 transition-colors">
        {/* Zone 1: Single-line brand mark + Liquid Glass icon */}
        <button
          type="button"
          onClick={() => onSelectTab('inbox')}
          aria-label="Snipt - Snip the noise"
          title="Snip the noise"
          className="inline-flex items-center gap-2.5 font-display text-lg font-bold tracking-tight text-slate-900 dark:text-slate-100 whitespace-nowrap shrink-0 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-blue-600 rounded-lg"
        >
          <SniptGlassIcon
            size={28}
            variant={designStyle}
            className="shrink-0 drop-shadow-xs"
          />
          <span>Snipt</span>
        </button>

        {/* Zone 2: 4 concise single-line text navigation links with adaptive-rate spring underline */}
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
                className={`relative h-14 inline-flex items-center gap-1.5 text-sm font-medium transition-colors whitespace-nowrap shrink-0 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-blue-600 ${
                  isActive
                    ? 'text-blue-700 dark:text-blue-400'
                    : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-100'
                }`}
              >
                <span>{item.label}</span>
                {count > 0 && (
                  <span className="font-mono text-xs tabular-nums text-slate-500 dark:text-slate-400">
                    ({count})
                  </span>
                )}
                {isActive && (
                  <span
                    aria-hidden="true"
                    className="absolute bottom-0 left-0 right-0 h-0.5 rounded-full bg-blue-700 dark:bg-blue-400"
                  />
                )}
              </button>
            );
          })}
        </nav>

        {/* Zone 3: Single primary CTA */}
        <div className="flex items-center shrink-0">
          {import.meta.env.DEV && onOpenSimulate && (
          <m.button
            type="button"
            whileTap={{ scale: 0.97 }}
            transition={{ type: 'spring', stiffness: 560, damping: 36 }}
            onClick={onOpenSimulate}
            className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-500 glass-cta rounded-lg transition-colors whitespace-nowrap shrink-0 min-h-[38px] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600"
          >
            <Plus className="w-3.5 h-3.5" aria-hidden="true" />
            <span>Simulate SMS</span>
          </m.button>
          )}
        </div>
      </header>

      {/* Mobile Bottom Tab Bar (Thumb Zone, total fixed mobile chrome <= 15% of 844px) */}
      <nav
        aria-label="Mobile navigation"
        className="md:hidden fixed bottom-0 left-0 right-0 z-40 min-h-16 safe-bottom bg-white/95 dark:bg-slate-900/95 glass-chrome border-t border-slate-200 dark:border-slate-800 grid grid-cols-3 items-center px-1"
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
                  ? 'text-blue-700 dark:text-blue-400'
                  : 'text-slate-500 dark:text-slate-400'
              }`}
            >
              <Icon className="w-5 h-5" aria-hidden="true" />
              <span className="text-[11px] font-medium leading-none whitespace-nowrap">
                {item.label}
              </span>
            </button>
          );
        })}
      </nav>
    </>
  );
}
