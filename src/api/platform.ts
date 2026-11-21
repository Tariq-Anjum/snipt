import { call, isTauri } from './core';
import type { Capabilities } from './types';

export const capabilities = (): Promise<Capabilities> => call('capabilities');

let backEnabled: boolean | null = null;

/**
 * Tells the Android shell whether the web layer wants to consume the back
 * gesture. The command arrives with the Kotlin plugin (S12); until then (and on
 * Linux) the call is skipped or its rejection ignored.
 */
export function setBackHandler(enabled: boolean, platform: Capabilities['platform']): void {
  if (platform !== 'android' || !isTauri() || backEnabled === enabled) return;
  backEnabled = enabled;
  call('set_back_handler', { enabled }).catch(() => {});
}

/** Android power-save / thermal state; null when the plugin is not present. */
export async function powerSaveActive(platform: Capabilities['platform']): Promise<boolean | null> {
  if (platform !== 'android' || !isTauri()) return null;
  try {
    const s = await call<{ powerSave?: boolean; thermal?: number }>('power_state');
    return Boolean(s.powerSave) || (s.thermal ?? 0) >= 4;
  } catch {
    return null;
  }
}
