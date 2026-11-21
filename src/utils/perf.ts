// Perf guards from UI-PLAN section 3: pause the ambient animation while the
// user scrolls or the window is hidden, and watch frame times during scroll so
// the UI can drop to Crisp Solid when Liquid Glass cannot hold the budget.

export function installScrollFlags(): () => void {
  const root = document.documentElement;
  let idle: number | undefined;
  const onScroll = () => {
    root.classList.add('is-scrolling');
    window.clearTimeout(idle);
    idle = window.setTimeout(() => root.classList.remove('is-scrolling'), 160);
  };
  const onVisibility = () => root.classList.toggle('is-hidden', document.hidden);
  document.addEventListener('scroll', onScroll, { capture: true, passive: true });
  document.addEventListener('visibilitychange', onVisibility);
  onVisibility();
  return () => {
    document.removeEventListener('scroll', onScroll, { capture: true } as EventListenerOptions);
    document.removeEventListener('visibilitychange', onVisibility);
    window.clearTimeout(idle);
    root.classList.remove('is-scrolling', 'is-hidden');
  };
}

/**
 * rAF frame monitor, active only while scrolling. Every 90 sampled frames it
 * checks whether more than 5 % exceeded 1.5x the frame budget (the budget is
 * the 10th-percentile frame time, i.e. the display interval). Fires `onJank`
 * once, then stops.
 */
export function startFrameGuard(onJank: () => void): () => void {
  let scrolling = false;
  let running = false;
  let last = 0;
  let idle: number | undefined;
  let done = false;
  let samples: number[] = [];

  const tick = (t: number) => {
    if (done || !scrolling) {
      running = false;
      last = 0;
      return;
    }
    if (last) samples.push(t - last);
    last = t;
    if (samples.length >= 90) {
      const sorted = [...samples].sort((a, b) => a - b);
      const budget = Math.min(16.7, Math.max(8.3, sorted[Math.floor(sorted.length * 0.1)]));
      const janky = samples.filter((d) => d > budget * 1.5).length / samples.length;
      samples = [];
      if (janky > 0.05) {
        done = true;
        onJank();
        running = false;
        return;
      }
    }
    requestAnimationFrame(tick);
  };

  const onScroll = () => {
    scrolling = true;
    window.clearTimeout(idle);
    idle = window.setTimeout(() => {
      scrolling = false;
    }, 200);
    if (!running && !done) {
      running = true;
      requestAnimationFrame(tick);
    }
  };
  document.addEventListener('scroll', onScroll, { capture: true, passive: true });
  return () => {
    done = true;
    window.clearTimeout(idle);
    document.removeEventListener('scroll', onScroll, { capture: true } as EventListenerOptions);
  };
}

export function glassSupported(): boolean {
  try {
    return (
      CSS.supports('backdrop-filter', 'blur(1px)') ||
      CSS.supports('-webkit-backdrop-filter', 'blur(1px)')
    );
  } catch {
    return false;
  }
}

export function prefersReducedTransparency(): boolean {
  try {
    return window.matchMedia('(prefers-reduced-transparency: reduce)').matches;
  } catch {
    return false;
  }
}
