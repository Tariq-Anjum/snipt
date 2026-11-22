import { call, isTauri } from './core';
import type { Capabilities } from './types';

// Android shell (tauri-plugin-telephony). Every helper is a no-op / null off
// Android, so callers do not branch on platform themselves.

const onAndroid = (p: Capabilities['platform']) => p === 'android' && isTauri();

/** Opens the system "set default SMS app" dialog; resolves to the new state. */
export async function requestDefaultSmsRole(
  platform: Capabilities['platform']
): Promise<boolean> {
  if (!onAndroid(platform)) return false;
  const r = await call<{ held: boolean }>('plugin:telephony|request_default_sms_role');
  return r.held;
}

export type CopyResult = 'ok' | 'failed' | 'unsupported';

/**
 * Copies a one-time code with EXTRA_IS_SENSITIVE and a native 30 s wipe that
 * clears only this copy's clip (ownership token; best-effort when Android
 * hides the clipboard). `ok` only after the clip was really set; `failed`
 * means nothing was copied; `unsupported` off Android (web clipboard path).
 */
export async function copySensitive(
  platform: Capabilities['platform'],
  text: string
): Promise<CopyResult> {
  // Platform not known yet: never fall back to the web clipboard (R4 #6).
  if (platform === 'pending' || (platform === 'android' && !isTauri())) return 'failed';
  if (!onAndroid(platform)) return 'unsupported';
  try {
    await call('plugin:telephony|copy_sensitive', { text, clearAfterMs: 30_000 });
    return 'ok';
  } catch {
    return 'failed';
  }
}

/**
 * FLAG_SECURE: `vault` while a sensitive screen shows; `global` = user pref.
 * Resolves to the window's actual secure state read back on the UI thread
 * (false on failure). Off Android there is no such flag: resolves `false`.
 */
export async function setSecure(
  platform: Capabilities['platform'],
  scope: 'vault' | 'global',
  on: boolean
): Promise<boolean> {
  if (!onAndroid(platform)) return false;
  try {
    const r = await call<{ ok: boolean }>('plugin:telephony|set_secure', { scope, on });
    return r.ok === true;
  } catch {
    return false;
  }
}

let vaultHolders = 0;

/**
 * Holds the `vault` FLAG_SECURE scope for a reveal control. Resolves to a
 * release function once the native flag is confirmed on (Android), or null
 * when it could not be confirmed: the caller must keep the code hidden then.
 * Fails closed (R4 #6): while the platform is still `pending` (or Android
 * outside the native runtime) it resolves null. Only a confirmed Linux
 * desktop / dev browser (no capture protection exists) resolves at once.
 * Reference-counted across the vault tab and thread reveals.
 */
export async function acquireVaultSecure(
  platform: Capabilities['platform']
): Promise<(() => void) | null> {
  if (platform === 'linux' || platform === 'web') return () => {};
  if (!onAndroid(platform)) return null;
  vaultHolders += 1;
  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    vaultHolders = Math.max(0, vaultHolders - 1);
    if (vaultHolders === 0) void setSecure(platform, 'vault', false);
  };
  const ok = await setSecure(platform, 'vault', true);
  if (!ok) {
    release();
    return null;
  }
  return release;
}

/**
 * Explicit "import missing messages" (Android): re-imports phone inbox rows
 * Snipt never stored and did not delete itself. Resolves to the number
 * imported; rejects when SMS access is missing.
 */
export async function importMissing(platform: Capabilities['platform']): Promise<number> {
  if (!onAndroid(platform)) return 0;
  const r = await call<{ imported: number }>('plugin:telephony|import_inbox');
  return r.imported;
}

export interface ComposeDraft {
  address: string;
  body: string;
}

export interface ComposeHandoff {
  draft: ComposeDraft | null;
  /** Tab a home-screen widget tap asked for. */
  tab: 'inbox' | 'finance' | null;
}

/** Draft handed over by another app (SENDTO sms:/smsto:, SEND text), or a widget tab. */
export async function takeComposeDraft(
  platform: Capabilities['platform']
): Promise<ComposeHandoff> {
  const none: ComposeHandoff = { draft: null, tab: null };
  if (!onAndroid(platform)) return none;
  try {
    const d = await call<{ present: boolean; address: string; body: string; tab?: string }>(
      'plugin:telephony|take_compose'
    );
    return {
      draft: d.present ? { address: d.address, body: d.body } : null,
      tab: d.tab === 'inbox' || d.tab === 'finance' ? d.tab : null,
    };
  } catch {
    return none;
  }
}

/**
 * Native events: `sms-received`, `sms-changed`, `send-status` (store changed
 * by a receiver/notification action) and `compose` (a draft is waiting).
 */
export function onNativeEvents(
  handlers: Partial<Record<'sms-received' | 'sms-changed' | 'send-status' | 'compose', () => void>>
): () => void {
  if (!isTauri()) return () => {};
  const offs: Array<() => void> = [];
  let closed = false;
  import('@tauri-apps/api/event')
    .then(({ listen }) =>
      Promise.all(
        Object.entries(handlers).map(([name, fn]) =>
          listen(name, () => fn?.()).then((u) => (closed ? u() : offs.push(u)))
        )
      )
    )
    .catch(() => {});
  return () => {
    closed = true;
    offs.forEach((u) => u());
  };
}

/** Status/navigation bar icons: `light` = light app background, dark icons. */
export async function setSystemBars(
  platform: Capabilities['platform'],
  light: boolean
): Promise<void> {
  if (!onAndroid(platform)) return;
  try {
    await call('plugin:telephony|set_system_bars', { light });
  } catch {
    // older shell without the command: icons keep the system default
  }
}

/** Home-screen widget snapshot. Built by the UI from masked, OTP-free data. */
export interface WidgetSnapshot {
  v: 1;
  updatedAt: number;
  previews: boolean;
  unread: number;
  latest: Array<{ title: string; body: string; ts: number }>;
  budget: {
    label: string;
    spent: number;
    limit: number;
    categories: Array<{ name: string; spent: number; limit: number }>;
  } | null;
}

export async function updateWidgets(
  platform: Capabilities['platform'],
  snapshot: WidgetSnapshot
): Promise<boolean> {
  if (!onAndroid(platform)) return false;
  try {
    const r = await call<{ ok: boolean }>('plugin:telephony|update_widgets', {
      snapshot: JSON.stringify(snapshot),
    });
    return r.ok === true;
  } catch {
    return false;
  }
}
