// DEV-only helpers (vite dev in a browser, or a dev-seed Tauri build). Every
// caller is guarded by a build-time constant, so production bundles drop them.
import { call } from './core';
import type { IngestReport, ParsedSms } from './types';

export const devImportSeed = (): Promise<IngestReport> => call('import_seed');

/** Browser-fallback only: there is no ingest command until the Android receiver (S12). */
export const devIngest = (payload: {
  sender: string;
  contactName?: string;
  body: string;
  isKnownContact: boolean;
  simSlot: 0 | 1;
  protocol: 'SMS' | 'RCS';
}): Promise<ParsedSms> => call('dev_ingest', payload);
