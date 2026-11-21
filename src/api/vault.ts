import { call } from './core';
import type { EncryptedOtpPayload, OtpItemView } from './types';

export const otpItems = (): Promise<OtpItemView[]> => call('otp_items');

/** Decrypts exactly one vault item; the list never carries plaintext codes. */
export const vaultReveal = (payload: EncryptedOtpPayload): Promise<string> =>
  call('vault_reveal', { payload });
