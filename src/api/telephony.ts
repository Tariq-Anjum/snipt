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
 * Off Android (Linux desktop: no capture protection exists) it resolves at
 * once. Reference-counted across the vault tab and thread reveals.
 */
export async function acquireVaultSecure(
  platform: Capabilities['platform']
): Promise<(() => void) | null> {
  if (!onAndroid(platform)) return () => {};
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

export interface ComposeDraft {
  address: string;
  body: string;
}

/** Draft handed over by another app (SENDTO sms:/smsto:, SEND text). */
export async function takeComposeDraft(
  platform: Capabilities['platform']
): Promise<ComposeDraft | null> {
  if (!onAndroid(platform)) return null;
  try {
    const d = await call<{ present: boolean; address: string; body: string }>(
      'plugin:telephony|take_compose'
    );
    return d.present ? { address: d.address, body: d.body } : null;
  } catch {
    return null;
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
