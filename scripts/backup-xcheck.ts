/**
 * Node cross-check: decrypt a Rust-written .mysms.enc with the TS oracle.
 * Usage: npx tsx scripts/backup-xcheck.ts <file>   (exit 0 on success)
 * The passphrase is the shared test constant, not a secret.
 */
import { readFileSync } from 'node:fs';
import { restoreEncryptedBackup } from '../src/engine/backup';

const TEST_PASSPHRASE = 'my-secret-pass-2026';
const file = process.argv[2];
if (!file) {
  console.error('usage: backup-xcheck.ts <file>');
  process.exit(2);
}
const serialized = readFileSync(file, 'utf8');
const envelope = JSON.parse(serialized) as { iterations: number; messageCount: number };
try {
  const restored = await restoreEncryptedBackup(serialized, TEST_PASSPHRASE);
  if (restored.messages.length !== envelope.messageCount) {
    throw new Error(`messageCount mismatch ${restored.messages.length} != ${envelope.messageCount}`);
  }
  console.log(`messageCount=${restored.messages.length} iterations=${envelope.iterations} app=${restored.app}`);
  await restoreEncryptedBackup(serialized, 'wrong-passphrase').then(
    () => {
      throw new Error('wrong passphrase accepted');
    },
    (e: Error) => console.log(`wrong passphrase rejected: ${e.message}`)
  );
} catch (e) {
  console.error(`xcheck failed: ${(e as Error).message}`);
  process.exit(1);
}
