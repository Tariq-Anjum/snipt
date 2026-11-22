// R4 #6: the vault fails closed until the native platform is known.
// Run: npx tsx --test src/api/telephony.test.ts
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { acquireVaultSecure, copySensitive } from './telephony';
import { PENDING_CAPABILITIES } from './types';

test('startup placeholder never counts as an acquired vault', async () => {
  assert.equal(PENDING_CAPABILITIES.platform, 'pending');
  assert.equal(await acquireVaultSecure('pending'), null);
  // Android outside the native runtime cannot confirm FLAG_SECURE either.
  assert.equal(await acquireVaultSecure('android'), null);
});

test('sensitive copy never takes the web clipboard path while pending', async () => {
  assert.equal(await copySensitive('pending', '123456'), 'failed');
  assert.equal(await copySensitive('android', '123456'), 'failed');
});

test('a confirmed desktop platform has no capture protection to wait for', async () => {
  const release = await acquireVaultSecure('linux');
  assert.equal(typeof release, 'function');
  assert.equal(await copySensitive('linux', '123456'), 'unsupported');
});
