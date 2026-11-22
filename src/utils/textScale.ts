// Message text scale: one tiny external store so a pinch updates the CSS
// variable every frame and re-renders only the components that read it
// (the inbox rows), not the whole App tree.
import { useSyncExternalStore } from 'react';

const KEY = 'snipt-text-scale-v1';
export const TEXT_SCALE_MIN = 0.85;
export const TEXT_SCALE_MAX = 1.6;

const clamp = (n: number) =>
  Math.min(TEXT_SCALE_MAX, Math.max(TEXT_SCALE_MIN, Math.round(n * 100) / 100));

function load(): number {
  try {
    const n = Number(window.localStorage.getItem(KEY));
    if (Number.isFinite(n) && n > 0) return clamp(n);
  } catch {
    // private mode / blocked storage
  }
  return 1;
}

let value = load();
const listeners = new Set<() => void>();

function apply() {
  document.documentElement.style.setProperty('--text-scale', String(value));
}
apply();

export function getTextScale(): number {
  return value;
}

export function setTextScale(next: number, persist = true): void {
  const v = clamp(next);
  if (v !== value) {
    value = v;
    apply();
    listeners.forEach((l) => l());
  }
  if (persist) {
    try {
      window.localStorage.setItem(KEY, String(value));
    } catch {
      // ignore
    }
  }
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useTextScale(): number {
  return useSyncExternalStore(subscribe, getTextScale, getTextScale);
}

/**
 * Two-finger pinch inside an element marked `data-pinch-text` scales message
 * text (like a messaging app) instead of zooming the page. Listeners are
 * passive: page zoom is already off (viewport user-scalable=no + WebView
 * setSupportZoom(false)), so scrolling is never blocked.
 */
export function installPinchTextScale(): () => void {
  let startDist = 0;
  let startScale = 1;
  let raf = 0;
  let pending = 0;
  let active = false;

  const dist = (t: TouchList) => Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);

  const onStart = (e: TouchEvent) => {
    if (e.touches.length !== 2) return;
    const target = e.target as Element | null;
    if (!target?.closest?.('[data-pinch-text]')) return;
    startDist = dist(e.touches);
    startScale = value;
    active = startDist > 0;
  };
  const onMove = (e: TouchEvent) => {
    if (!active || e.touches.length !== 2) return;
    pending = startScale * (dist(e.touches) / startDist);
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        setTextScale(pending, false);
      });
    }
  };
  const onEnd = (e: TouchEvent) => {
    if (!active || e.touches.length >= 2) return;
    active = false;
    setTextScale(value, true);
  };

  document.addEventListener('touchstart', onStart, { passive: true });
  document.addEventListener('touchmove', onMove, { passive: true });
  document.addEventListener('touchend', onEnd, { passive: true });
  document.addEventListener('touchcancel', onEnd, { passive: true });
  return () => {
    cancelAnimationFrame(raf);
    document.removeEventListener('touchstart', onStart);
    document.removeEventListener('touchmove', onMove);
    document.removeEventListener('touchend', onEnd);
    document.removeEventListener('touchcancel', onEnd);
  };
}
