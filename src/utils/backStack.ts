import { useEffect, useRef } from 'react';

// Small back-stack: overlays and the open thread register a handler while they
// are showing. The shell enables the Android back callback only while the stack
// is non-empty, so predictive back-to-home keeps working otherwise.
type Handler = () => void;
const stack: Handler[] = [];
const listeners = new Set<(depth: number) => void>();

const emit = () => listeners.forEach((l) => l(stack.length));

function push(h: Handler): () => void {
  stack.push(h);
  emit();
  return () => {
    const i = stack.lastIndexOf(h);
    if (i >= 0) stack.splice(i, 1);
    emit();
  };
}

/** Runs the top handler. Returns true when something consumed the back action. */
export function popBack(): boolean {
  const top = stack[stack.length - 1];
  if (!top) return false;
  top();
  return true;
}

export function subscribeBack(fn: (depth: number) => void): () => void {
  listeners.add(fn);
  fn(stack.length);
  return () => {
    listeners.delete(fn);
  };
}

export function useBackLayer(active: boolean, onBack: Handler): void {
  const ref = useRef(onBack);
  ref.current = onBack;
  useEffect(() => {
    if (!active) return;
    return push(() => ref.current());
  }, [active]);
}
