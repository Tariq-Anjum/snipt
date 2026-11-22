import { call } from './core';
import type { BackupFile, RestoreReport } from './types';

export const backupCreate = (passphrase: string): Promise<BackupFile> =>
  call('backup_create', { passphrase });

export const backupRestore = (content: string, passphrase: string): Promise<RestoreReport> =>
  call('backup_restore', { content, passphrase });

export interface BackupSaved {
  cancelled: boolean;
  filename: string;
  messageCount: number;
}

export interface BackupRestored {
  cancelled: boolean;
  name: string;
  report: RestoreReport | null;
}

/**
 * Native save: encrypts in the core, then opens the SAF create-document
 * picker (Android) or a save dialog (Linux). The ciphertext never reaches JS.
 */
export const backupSave = (passphrase: string): Promise<BackupSaved> =>
  call('backup_save', { passphrase });

/** Native restore: open-document picker / file dialog, decrypt in the core. */
export const backupRestorePicked = (passphrase: string): Promise<BackupRestored> =>
  call('backup_restore_picked', { passphrase });

/** Dev-browser fallback "save": browser download of the encrypted string. */
export function downloadText(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
