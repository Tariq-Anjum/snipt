import { call } from './core';
import type { BackupFile, RestoreReport } from './types';

export const backupCreate = (passphrase: string): Promise<BackupFile> =>
  call('backup_create', { passphrase });

export const backupRestore = (content: string, passphrase: string): Promise<RestoreReport> =>
  call('backup_restore', { content, passphrase });

/** Interim "save": browser download of the encrypted string (native dialog: S14). */
export function downloadText(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
