import { invoke } from '@tauri-apps/api/core';

/** True inside the Tauri webview (Linux window / Android WebView). */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Single IPC choke point. Tauri converts camelCase JS argument keys to the
 * snake_case Rust parameter names, so callers pass flat camelCase objects.
 * Outside Tauri, only `vite dev` gets the in-page engine fallback; the
 * production bundle statically drops that branch (`import.meta.env.DEV`).
 */
export async function call<T>(
  cmd: string,
  args?: Record<string, unknown>
): Promise<T> {
  if (isTauri()) {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      throw new Error(typeof e === 'string' ? e : (e as Error)?.message ?? 'command failed');
    }
  }
  if (import.meta.env.DEV) {
    const m = await import('./devFallback');
    return (await m.dispatch(cmd, args ?? {})) as T;
  }
  throw new Error('Snipt core is unavailable outside the Tauri runtime.');
}
