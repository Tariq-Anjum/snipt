import {
  CheckCircle2,
  Cloud,
  Download,
  Eye,
  EyeOff,
  HardDrive,
  Lock,
  Upload,
} from 'lucide-react';
import { useRef, useState, type ChangeEvent } from 'react';
import {
  backupCreate,
  backupRestore,
  backupRestorePicked,
  backupSave,
  CLOUD_BACKUP_PROVIDERS,
  downloadText,
  type CloudBackupProvider,
  type RestoreReport,
} from '../api';
import { formatFullDateTime } from '../utils/format';

interface CloudBackupSectionProps {
  preferredProvider: CloudBackupProvider;
  onChangeProvider: (provider: CloudBackupProvider) => void;
  lastBackupAtMs: number | null;
  onBackupSuccess: (timestampMs: number) => void;
  onRestored: (report: RestoreReport) => void;
  /** Linux viewer: restore only (backups are made on the phone). */
  readOnly: boolean;
  /** Native save/open picker (SAF / file dialog) exists; else download + file input. */
  nativePicker: boolean;
}

const MAX_BACKUP_FILE_BYTES = 64 * 1024 * 1024;

export function CloudBackupSection({
  preferredProvider,
  onChangeProvider,
  lastBackupAtMs,
  onBackupSuccess,
  onRestored,
  readOnly,
  nativePicker,
}: CloudBackupSectionProps) {
  const [passphrase, setPassphrase] = useState('');
  const [showPassphrase, setShowPassphrase] = useState(false);
  const [busy, setBusy] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const selectedProviderMeta =
    CLOUD_BACKUP_PROVIDERS.find((p) => p.id === preferredProvider) ??
    CLOUD_BACKUP_PROVIDERS[0];

  const handleCreateBackup = async () => {
    setErrorMessage(null);
    setStatusMessage(null);

    if (passphrase.trim().length < 6) {
      setErrorMessage(
        'Enter a backup passphrase of at least 6 characters to lock your archive.'
      );
      return;
    }

    setBusy(true);
    try {
      if (nativePicker) {
        // The system picker lists the installed cloud apps (Drive, OneDrive,
        // Dropbox, ...) and local folders; the provider card is a hint only.
        const saved = await backupSave(passphrase);
        if (saved.cancelled) {
          setStatusMessage('Backup cancelled: nothing was saved.');
        } else {
          onBackupSuccess(Date.now());
          setStatusMessage(
            `Encrypted ${saved.messageCount.toLocaleString('en-IN')} messages with AES-256-GCM and saved ${saved.filename}.`
          );
        }
      } else {
        const file = await backupCreate(passphrase);
        downloadText(file.filename, file.content);
        onBackupSuccess(Date.now());
        setStatusMessage(
          `Encrypted ${file.messageCount.toLocaleString('en-IN')} messages with AES-256-GCM into ${file.filename} (ready for ${selectedProviderMeta.label}).`
        );
      }
    } catch (err) {
      setErrorMessage(
        err instanceof Error ? err.message : 'Failed to create encrypted backup.'
      );
    } finally {
      // The passphrase is only needed for this call.
      setPassphrase('');
      setShowPassphrase(false);
      setBusy(false);
    }
  };

  const handleRestoreClick = () => {
    setErrorMessage(null);
    setStatusMessage(null);
    if (!passphrase.trim()) {
      setErrorMessage(
        'Enter the passphrase that locked your .mysms.enc file before choosing it.'
      );
      return;
    }
    if (nativePicker) {
      void handleRestoreNative();
      return;
    }
    fileInputRef.current?.click();
  };

  const restoredMessage = (report: RestoreReport, name: string) =>
    `Restored ${report.ingest.inserted.toLocaleString('en-IN')} messages (${report.ingest.skippedExisting.toLocaleString('en-IN')} already present), ${report.archivedThreads} archived threads and ${report.blockedSenders} blocked senders${name ? ` from ${name}` : ''}.`;

  const wrongPassphraseHint = (err: unknown) => {
    const m = err instanceof Error ? err.message : '';
    return /passphrase/i.test(m)
      ? `${m} Re-enter the passphrase and choose the file again.`
      : m || 'Could not decrypt this backup file.';
  };

  const handleRestoreNative = async () => {
    setBusy(true);
    try {
      const r = await backupRestorePicked(passphrase);
      if (r.cancelled || !r.report) {
        setStatusMessage('Restore cancelled: no file was chosen.');
      } else {
        onRestored(r.report);
        setStatusMessage(restoredMessage(r.report, r.name));
      }
    } catch (err) {
      setErrorMessage(wrongPassphraseHint(err));
    } finally {
      setPassphrase('');
      setShowPassphrase(false);
      setBusy(false);
    }
  };

  const handleFileSelected = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;

    setBusy(true);
    setErrorMessage(null);
    setStatusMessage(null);

    try {
      // Refuse oversized files before reading them into memory (the core's
      // own cap applies after IPC; the Android picker uses the same limit).
      if (file.size > MAX_BACKUP_FILE_BYTES) {
        throw new Error('Backup file is too large (over 64 MiB).');
      }
      const text = await file.text();
      const report = await backupRestore(text, passphrase);
      onRestored(report);
      setStatusMessage(restoredMessage(report, file.name));
    } catch (err) {
      setErrorMessage(wrongPassphraseHint(err));
    } finally {
      setPassphrase('');
      setShowPassphrase(false);
      setBusy(false);
    }
  };

  return (
    <section
      aria-label="Encrypted cloud and device backup"
      className="bg-white dark:bg-slate-900 glass-panel border border-slate-200 dark:border-slate-800 rounded-2xl overflow-hidden"
    >
      <div className="p-5 border-b border-slate-200 dark:border-slate-800 flex flex-col lg:flex-row lg:items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <Cloud className="w-4 h-4 text-blue-700 dark:text-blue-400" aria-hidden="true" />
            <h2 className="font-display text-base font-semibold text-slate-900 dark:text-slate-100">
              Private Cloud &amp; Device Backup
            </h2>
          </div>
          <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
            Locked on-device with AES-256-GCM before saving to Google Drive, OneDrive, Dropbox, or any cloud app installed on your phone.
          </p>
        </div>

        <div className="text-xs font-mono tabular-nums text-slate-600 dark:text-slate-400 shrink-0">
          {lastBackupAtMs ? (
            <span className="inline-flex items-center gap-1.5 text-emerald-700 dark:text-emerald-400">
              <CheckCircle2 className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Last backup: {formatFullDateTime(lastBackupAtMs)}</span>
            </span>
          ) : (
            <span>No backup saved yet</span>
          )}
        </div>
      </div>

      <div className="p-5 space-y-5">
        {/* Destination Provider Picker */}
        <div>
          <p className="text-xs font-semibold text-slate-700 dark:text-slate-300 mb-2.5">
            1. Choose where to store your encrypted backup
          </p>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-2.5">
            {CLOUD_BACKUP_PROVIDERS.map((provider) => {
              const active = preferredProvider === provider.id;
              return (
                <button
                  key={provider.id}
                  type="button"
                  onClick={() => onChangeProvider(provider.id)}
                  aria-pressed={active}
                  className={`text-left p-3 rounded-lg border transition-colors ${
                    active
                      ? 'bg-blue-50/80 border-blue-300 text-slate-900 dark:bg-blue-950/40 dark:border-blue-700 dark:text-slate-100'
                      : 'bg-slate-50/70 border-slate-200 text-slate-700 hover:bg-slate-100 dark:bg-slate-950 dark:border-slate-800 dark:text-slate-300 dark:hover:bg-slate-800/70'
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-semibold">{provider.label}</span>
                    {provider.id === 'device_picker' ? (
                      <HardDrive className="w-3.5 h-3.5 text-slate-400 shrink-0" aria-hidden="true" />
                    ) : (
                      <Cloud className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400 shrink-0" aria-hidden="true" />
                    )}
                  </div>
                  <p className="mt-1 text-[11px] text-slate-500 dark:text-slate-400 leading-snug">
                    {provider.subtitle}
                  </p>
                </button>
              );
            })}
          </div>
        </div>

        {/* Passphrase + Save/Restore Controls */}
        <div className="grid grid-cols-1 lg:grid-cols-[1fr_auto] gap-4 items-end pt-2 border-t border-slate-100 dark:border-slate-800">
          <div>
            <label
              htmlFor="backup-passphrase"
              className="block text-xs font-semibold text-slate-700 dark:text-slate-300 mb-1.5"
            >
              2. Backup lock passphrase (never leaves your device)
            </label>
            <div className="relative max-w-md">
              <Lock
                className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"
                aria-hidden="true"
              />
              <input
                id="backup-passphrase"
                type={showPassphrase ? 'text' : 'password'}
                value={passphrase}
                onChange={(e) => {
                  setPassphrase(e.target.value);
                  setErrorMessage(null);
                }}
                placeholder="At least 6 characters (e.g. snipt-home-2026)"
                className="w-full pl-9 pr-10 py-2 text-sm bg-slate-50 dark:bg-slate-950 border border-slate-300 dark:border-slate-700 rounded-lg text-slate-900 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-600"
              />
              <button
                type="button"
                onClick={() => setShowPassphrase((v) => !v)}
                aria-label={showPassphrase ? 'Hide backup passphrase' : 'Show backup passphrase'}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-slate-500 hover:text-slate-800 dark:hover:text-slate-200"
              >
                {showPassphrase ? (
                  <EyeOff className="w-4 h-4" aria-hidden="true" />
                ) : (
                  <Eye className="w-4 h-4" aria-hidden="true" />
                )}
              </button>
            </div>
            <p className="mt-1.5 text-[11px] text-slate-500 dark:text-slate-400">
              Android SAF target:{' '}
              <span className="font-mono text-slate-700 dark:text-slate-300">
                {selectedProviderMeta.androidSafHint}
              </span>{' '}
              · Zero cloud servers or accounts required.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2.5">
            {!readOnly && (
            <button
              type="button"
              onClick={handleCreateBackup}
              disabled={busy}
              className="inline-flex items-center gap-1.5 px-4 py-2 text-xs font-semibold text-white bg-blue-700 hover:bg-blue-800 disabled:opacity-50 dark:bg-blue-600 dark:hover:bg-blue-500 rounded-lg transition-colors whitespace-nowrap"
            >
              <Download className="w-3.5 h-3.5" aria-hidden="true" />
              <span>
                {busy ? 'Encrypting...' : 'Create Encrypted Backup (.mysms.enc)'}
              </span>
            </button>
            )}

            <button
              type="button"
              onClick={handleRestoreClick}
              disabled={busy}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 rounded-lg transition-colors whitespace-nowrap"
            >
              <Upload className="w-3.5 h-3.5" aria-hidden="true" />
              <span>Restore Backup</span>
            </button>

            <input
              ref={fileInputRef}
              type="file"
              accept=".enc,.json,application/octet-stream,application/json"
              onChange={handleFileSelected}
              aria-label="Choose encrypted Snipt backup file to restore"
              className="sr-only"
            />
          </div>
        </div>

        {readOnly && (
          <p className="text-[11px] text-slate-500 dark:text-slate-400">
            This device is a read-only viewer: restore the encrypted backup you made with Snipt on your phone.
          </p>
        )}

        {errorMessage && (
          <p
            role="alert"
            className="px-3.5 py-2.5 text-xs font-medium text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/40 border border-red-200 dark:border-red-900/60 rounded-lg"
          >
            {errorMessage}
          </p>
        )}

        {statusMessage && (
          <p
            role="status"
            className="px-3.5 py-2.5 text-xs font-medium text-emerald-800 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-900/60 rounded-lg"
          >
            {statusMessage}
          </p>
        )}
      </div>
    </section>
  );
}
