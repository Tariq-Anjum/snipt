import type {
  BudgetLimits,
  PaymentCycleConfig,
  RawSms,
} from './types';

export type CloudBackupProvider =
  | 'device_picker'
  | 'google_drive'
  | 'onedrive'
  | 'dropbox';

export interface CloudProviderOption {
  id: CloudBackupProvider;
  label: string;
  subtitle: string;
  androidSafHint: string;
}

export const CLOUD_BACKUP_PROVIDERS: readonly CloudProviderOption[] = [
  {
    id: 'device_picker',
    label: 'Any Installed Cloud App / Device',
    subtitle: 'Uses your phone or OS file picker (SAF) to save to any cloud drive or local folder.',
    androidSafHint: 'Android ACTION_CREATE_DOCUMENT',
  },
  {
    id: 'google_drive',
    label: 'Google Drive',
    subtitle: 'Save encrypted .mysms.enc directly to your private Google Drive folder.',
    androidSafHint: 'com.google.android.apps.docs.storage',
  },
  {
    id: 'onedrive',
    label: 'Microsoft OneDrive',
    subtitle: 'Save encrypted .mysms.enc directly to your Microsoft OneDrive app.',
    androidSafHint: 'com.microsoft.skydrive.content.StorageAccessProvider',
  },
  {
    id: 'dropbox',
    label: 'Dropbox',
    subtitle: 'Save encrypted .mysms.enc directly to your Dropbox storage.',
    androidSafHint: 'com.dropbox.android.provider',
  },
] as const;

export interface MySmsBackupPayload {
  schemaVersion: 1;
  app: 'mySMS';
  createdAtMs: number;
  provider: CloudBackupProvider;
  messages: RawSms[];
  archivedThreadIds: number[];
  blockedSenders: string[];
  completedReminderSmsIds: number[];
  cycleConfig: PaymentCycleConfig;
  budgetLimits: BudgetLimits;
  scanScheduleDay: string;
  autoTranslateToEnglish: boolean;
}

export interface EncryptedBackupEnvelope {
  format: 'mySMS-encrypted-backup-v1';
  cipher: 'AES-256-GCM';
  kdf: 'PBKDF2-SHA256';
  iterations: number;
  createdAtMs: number;
  provider: CloudBackupProvider;
  messageCount: number;
  saltB64: string;
  ivB64: string;
  ciphertextB64: string;
}

const PBKDF2_ITERATIONS = 150_000;

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const slice = bytes.subarray(i, i + chunkSize);
    binary += String.fromCharCode(...slice);
  }
  return btoa(binary);
}

function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(b64);
  const buffer = new ArrayBuffer(binary.length);
  const out = new Uint8Array(buffer);
  for (let i = 0; i < binary.length; i += 1) {
    out[i] = binary.charCodeAt(i);
  }
  return out;
}

async function deriveAesKeyFromPassphrase(
  passphrase: string,
  salt: Uint8Array<ArrayBuffer>,
  iterations: number
): Promise<CryptoKey> {
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(passphrase),
    'PBKDF2',
    false,
    ['deriveKey']
  );

  return crypto.subtle.deriveKey(
    {
      name: 'PBKDF2',
      salt,
      iterations,
      hash: 'SHA-256',
    },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt']
  );
}

/**
 * Encrypts the complete mySMS history (10,000+ SMS, billing cycles, budgets, settings)
 * into a password-protected AES-256-GCM .mysms.enc envelope safe to store in any cloud
 * (Google Drive, OneDrive, Dropbox, or local storage) because cloud providers only see ciphertext.
 */
export async function createEncryptedBackup(
  payload: MySmsBackupPayload,
  passphrase: string
): Promise<{ envelope: EncryptedBackupEnvelope; serialized: string }> {
  const trimmedPass = passphrase.trim();
  if (trimmedPass.length < 6) {
    throw new Error('Passphrase must be at least 6 characters long.');
  }

  const salt = new Uint8Array(new ArrayBuffer(16));
  const iv = new Uint8Array(new ArrayBuffer(12));
  crypto.getRandomValues(salt);
  crypto.getRandomValues(iv);

  const key = await deriveAesKeyFromPassphrase(
    trimmedPass,
    salt,
    PBKDF2_ITERATIONS
  );

  const plaintext = new TextEncoder().encode(JSON.stringify(payload));
  const ciphertextBuf = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    plaintext
  );

  const envelope: EncryptedBackupEnvelope = {
    format: 'mySMS-encrypted-backup-v1',
    cipher: 'AES-256-GCM',
    kdf: 'PBKDF2-SHA256',
    iterations: PBKDF2_ITERATIONS,
    createdAtMs: payload.createdAtMs,
    provider: payload.provider,
    messageCount: payload.messages.length,
    saltB64: bytesToBase64(salt),
    ivB64: bytesToBase64(iv),
    ciphertextB64: bytesToBase64(new Uint8Array(ciphertextBuf)),
  };

  return {
    envelope,
    serialized: JSON.stringify(envelope, null, 2),
  };
}

/**
 * Decrypts and validates a .mysms.enc backup file using the user's passphrase.
 */
export async function restoreEncryptedBackup(
  serialized: string,
  passphrase: string
): Promise<MySmsBackupPayload> {
  const trimmedPass = passphrase.trim();
  if (!trimmedPass) {
    throw new Error('Enter your backup passphrase to decrypt this file.');
  }

  let parsed: Partial<EncryptedBackupEnvelope>;
  try {
    parsed = JSON.parse(serialized) as Partial<EncryptedBackupEnvelope>;
  } catch {
    throw new Error('Invalid backup file format. Expected a .mysms.enc vault file.');
  }

  if (
    parsed.format !== 'mySMS-encrypted-backup-v1' ||
    !parsed.saltB64 ||
    !parsed.ivB64 ||
    !parsed.ciphertextB64
  ) {
    throw new Error('Unsupported or corrupted mySMS backup file.');
  }

  const salt = base64ToBytes(parsed.saltB64);
  const iv = base64ToBytes(parsed.ivB64);
  const ciphertext = base64ToBytes(parsed.ciphertextB64);
  const iterations =
    typeof parsed.iterations === 'number' && parsed.iterations >= 10_000
      ? parsed.iterations
      : PBKDF2_ITERATIONS;

  const key = await deriveAesKeyFromPassphrase(trimmedPass, salt, iterations);

  try {
    const plainBuf = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv },
      key,
      ciphertext
    );
    const decoded = JSON.parse(
      new TextDecoder().decode(plainBuf)
    ) as MySmsBackupPayload;

    if (!decoded || decoded.app !== 'mySMS' || !Array.isArray(decoded.messages)) {
      throw new Error('Decrypted payload is not a valid mySMS archive.');
    }

    return decoded;
  } catch (err) {
    if (err instanceof Error && err.message.includes('valid mySMS archive')) {
      throw err;
    }
    throw new Error(
      'Incorrect passphrase or tampered backup file. Could not decrypt.'
    );
  }
}

export function buildBackupFilename(nowMs: number = Date.now()): string {
  const d = new Date(nowMs);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `mysms-backup-${yyyy}-${mm}-${dd}.mysms.enc`;
}
