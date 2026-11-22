import type { CloudBackupProvider, PaymentCycleConfig } from './types';

export interface CloudProviderOption {
  id: CloudBackupProvider;
  label: string;
  subtitle: string;
  androidSafHint: string;
}

// Mirrors src/engine/backup.ts (copied so the UI does not import the TS crypto engine).
export const CLOUD_BACKUP_PROVIDERS: readonly CloudProviderOption[] = [
  {
    id: 'device_picker',
    label: 'Any Installed Cloud App / Device',
    subtitle: 'Uses your phone or OS file picker (SAF) to save to any cloud drive or local folder.',
    androidSafHint: 'Android ACTION_CREATE_DOCUMENT (system picker)',
  },
  {
    id: 'google_drive',
    label: 'Google Drive',
    subtitle: 'Save the encrypted backup to your private Google Drive folder.',
    androidSafHint: 'com.google.android.apps.docs.storage',
  },
  {
    id: 'onedrive',
    label: 'Microsoft OneDrive',
    subtitle: 'Save the encrypted backup to your Microsoft OneDrive app.',
    androidSafHint: 'com.microsoft.skydrive.content.StorageAccessProvider',
  },
  {
    id: 'dropbox',
    label: 'Dropbox',
    subtitle: 'Save the encrypted backup to your Dropbox storage.',
    androidSafHint: 'com.dropbox.android.provider',
  },
];

export const DEFAULT_CYCLE_CONFIG: PaymentCycleConfig = {
  UPI: 1,
  'Credit Card': 15,
  'Savings Account': 1,
};
