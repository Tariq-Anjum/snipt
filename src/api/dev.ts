// DEV-only helpers (vite dev in a browser, or a dev-seed Tauri build). Every
// caller is guarded by a build-time constant, so production bundles drop them.
import { call } from './core';
import type { IngestReport } from './types';

export const devImportSeed = (): Promise<IngestReport> => call('import_seed');
