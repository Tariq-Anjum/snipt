/**
 * AES-256-GCM on-device OTP vault encryption using Web Crypto API.
 * Mirrors the Rust ChaCha20-Poly1305 / Android Keystore OTP isolation layer
 * so extracted OTPs are never exposed in plaintext to third-party apps.
 */

export interface EncryptedOtpPayload {
  ivB64: string;
  ciphertextB64: string;
  algorithm: 'AES-256-GCM';
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(b64);
  const buffer = new ArrayBuffer(binary.length);
  const bytes = new Uint8Array(buffer);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

let cachedKeyPromise: Promise<CryptoKey> | null = null;

async function getOrCreateVaultKey(): Promise<CryptoKey> {
  if (cachedKeyPromise) return cachedKeyPromise;
  cachedKeyPromise = crypto.subtle.generateKey(
    { name: 'AES-GCM', length: 256 },
    false, // non-extractable key isolated inside app context
    ['encrypt', 'decrypt']
  );
  return cachedKeyPromise;
}

export async function encryptOtpCode(plainCode: string): Promise<EncryptedOtpPayload> {
  const key = await getOrCreateVaultKey();
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encoded = new TextEncoder().encode(plainCode);
  const encrypted = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    encoded
  );
  return {
    ivB64: bytesToBase64(iv),
    ciphertextB64: bytesToBase64(new Uint8Array(encrypted)),
    algorithm: 'AES-256-GCM',
  };
}

export async function decryptOtpCode(payload: EncryptedOtpPayload): Promise<string> {
  const key = await getOrCreateVaultKey();
  const iv = base64ToBytes(payload.ivB64);
  const data = base64ToBytes(payload.ciphertextB64);
  const decrypted = await crypto.subtle.decrypt(
    { name: 'AES-GCM', iv },
    key,
    data
  );
  return new TextDecoder().decode(decrypted);
}
