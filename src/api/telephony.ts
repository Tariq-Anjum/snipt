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

/**
 * Copies a one-time code with EXTRA_IS_SENSITIVE and a native 30 s wipe that
 * is cancelled if the user copies something else. Returns false off Android
 * (the caller falls back to the web clipboard).
 */
export async function copySensitive(
  platform: Capabilities['platform'],
  text: string
): Promise<boolean> {
  if (!onAndroid(platform)) return false;
  try {
    await call('plugin:telephony|copy_sensitive', { text, clearAfterMs: 30_000 });
    return true;
  } catch {
    return false;
  }
}

/** FLAG_SECURE: `vault` while a sensitive screen shows; `global` = user pref. */
export function setSecure(
  platform: Capabilities['platform'],
  scope: 'vault' | 'global',
  on: boolean
): void {
  if (!onAndroid(platform)) return;
  call('plugin:telephony|set_secure', { scope, on }).catch(() => {});
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
