export function formatInr(amount: number): string {
  const abs = Math.abs(amount);
  const formatted = new Intl.NumberFormat('en-IN', {
    minimumFractionDigits: abs % 1 === 0 ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(abs);
  return `${amount < 0 ? '-' : ''}₹${formatted}`;
}

export function formatShortTime(timestampMs: number, nowMs: number = Date.now()): string {
  const diff = nowMs - timestampMs;
  const date = new Date(timestampMs);

  if (diff < 60_000) return 'Just now';
  if (diff < 60 * 60_000) {
    return `${Math.max(1, Math.floor(diff / 60_000))}m ago`;
  }

  const nowDate = new Date(nowMs);
  const isSameDay =
    date.getFullYear() === nowDate.getFullYear() &&
    date.getMonth() === nowDate.getMonth() &&
    date.getDate() === nowDate.getDate();

  if (isSameDay) {
    return date.toLocaleTimeString('en-IN', {
      hour: 'numeric',
      minute: '2-digit',
      hour12: true,
    });
  }

  if (diff < 7 * 24 * 60 * 60_000) {
    return date.toLocaleDateString('en-IN', { weekday: 'short' });
  }

  return date.toLocaleDateString('en-IN', {
    day: 'numeric',
    month: 'short',
  });
}

export function formatFullDateTime(timestampMs: number): string {
  return new Date(timestampMs).toLocaleString('en-IN', {
    day: '2-digit',
    month: 'short',
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  });
}

export function formatCountdown(expiresAtMs: number, nowMs: number): string {
  const rem = expiresAtMs - nowMs;
  if (rem <= 0) return 'Expired';
  const hours = Math.floor(rem / 3_600_000);
  const mins = Math.floor((rem % 3_600_000) / 60_000);
  if (hours > 0) return `${hours}h ${mins}m left`;
  return `${Math.max(1, mins)}m left`;
}

export function getInitials(name: string): string {
  const cleaned = name.replace(/[^a-zA-Z0-9 ]/g, ' ').trim();
  const parts = cleaned.split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '#';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}
