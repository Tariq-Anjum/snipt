import { call } from './core';
import type { EncryptedOtpPayload, OtpItemView } from './types';

/** OTP list: codes are masked in both `otp.code` and `body`. */
export const otpItems = (): Promise<OtpItemView[]> => call('otp_items');

/** Decrypts exactly one vault item; the list never carries plaintext codes. */
export const vaultReveal = (payload: EncryptedOtpPayload): Promise<string> =>
  call('vault_reveal', { payload });

/** store.ts `purgeExpiredOtpMessages` (TTL rule); returns the number purged. */
export const purgeExpiredOtps = (): Promise<number> => call('purge_expired_otps');
