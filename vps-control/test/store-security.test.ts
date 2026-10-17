import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { chmod, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { ControlStore, StoreError } from '../src/store.ts';

const fixture = async (options = {}) => {
  const dir = await mkdtemp(join(tmpdir(), 'coucou-store-'));
  return { dir, path: join(dir, 'control.json'), store: new ControlStore(join(dir, 'control.json'), options) };
};
const publicRaw = (key: ReturnType<typeof generateKeyPairSync>['publicKey']) =>
  key.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');

async function pair(store: ControlStore, capabilities = ['health']) {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const pending = await store.startPairing('pc', publicRaw(publicKey), capabilities, 'source');
  await store.approvePairing(pending.pairingId);
  const identity = await store.getPairingStatus(pending.pairingId, pending.claimToken);
  assert.equal(identity.status, 'approved');
  return { ...identity.identity!, privateKey };
}

test('fails closed when the persistent store is corrupt', async () => {
  const { path } = await fixture();
  await writeFile(path, '{not-json');
  const store = new ControlStore(path);
  await assert.rejects(() => store.snapshot(), /corrupt/i);
});

test('fails closed when persistent JSON has an invalid schema', async () => {
  const { path } = await fixture();
  await writeFile(path, JSON.stringify({ version: 1, devices: [{}], pairings: [], tasks: [], audit: [], rateLimits: [], nonces: [] }));
  const store = new ControlStore(path);
  await assert.rejects(() => store.snapshot(), /corrupt/i);
});

test('serializes concurrent durable mutations without losing records', async () => {
  const { path, store } = await fixture();
  await Promise.all(Array.from({ length: 30 }, (_, i) =>
    store.startPairing(`pc-${i}`, Buffer.alloc(32, i).toString('base64'), [], `ip-${i}`)));
  const db = JSON.parse(await readFile(path, 'utf8'));
  assert.equal(db.pairings.length, 30);
});

test('uses a six digit PIN generated through the injected CSPRNG', async () => {
  const { store } = await fixture({ randomInt: () => 42 });
  const pairing = await store.startPairing('pc', Buffer.alloc(32).toString('base64'), [], 'ip');
  assert.equal(pairing.pin, '000042');
});

test('persists bounded rate-limit state across restarts', async () => {
  const { path } = await fixture();
  const options = { rateLimitMax: 2, rateLimitWindowMs: 60_000, rateLimitCapacity: 3 };
  let store = new ControlStore(path, options);
  const key = Buffer.alloc(32).toString('base64');
  await store.startPairing('one', key, [], 'same-ip');
  await store.startPairing('two', key, [], 'same-ip');
  store = new ControlStore(path, options);
  await assert.rejects(() => store.startPairing('three', key, [], 'same-ip'), (e: unknown) =>
    e instanceof StoreError && e.code === 'RATE_LIMITED');
  for (let i = 0; i < 8; i++) {
    await store.startPairing(`other-${i}`, key, [], `ip-${i}`);
  }
  assert.ok((await store.snapshot()).rateLimits.length <= 3);
});

test('pairing approval and claim-token polling issue an identity once', async () => {
  const { store } = await fixture();
  const key = Buffer.alloc(32, 7).toString('base64');
  const pending = await store.startPairing('pc', key, ['health'], 'ip');
  assert.match(pending.pin, /^\d{6}$/);
  assert.equal((await store.getPairingStatus(pending.pairingId, pending.claimToken)).status, 'pending');
  await assert.rejects(() => store.getPairingStatus(pending.pairingId, 'wrong'), /unauthorized/i);
  await store.approvePairing(pending.pairingId);
  const approved = await store.getPairingStatus(pending.pairingId, pending.claimToken);
  assert.equal(approved.status, 'approved');
  assert.ok(approved.identity?.credential);
  assert.equal((await store.getPairingStatus(pending.pairingId, pending.claimToken)).identity, undefined);
});

test('verifies Ed25519 connection proof and rejects nonce replay', async () => {
  const { store } = await fixture();
  const identity = await pair(store);
  const timestamp = String(Math.floor(Date.now() / 1000));
  const nonce = 'nonce-1234567890';
  const signature = sign(null, Buffer.from(`${identity.deviceId}.${timestamp}.${nonce}`), identity.privateKey).toString('base64url');
  assert.equal(await store.authenticateProof(identity.deviceId, identity.credential, timestamp, nonce, signature), true);
  assert.equal(await store.authenticateProof(identity.deviceId, identity.credential, timestamp, nonce, signature), false);
  assert.equal(await store.authenticateProof(identity.deviceId, identity.credential, String(Math.floor(Date.now() / 1000) - 120), 'fresh-nonce-123', signature), false);
});

test('enforces task ownership, transitions, idempotency conflicts and cancellation audit', async () => {
  const { store } = await fixture();
  const first = await pair(store, ['health']);
  const second = await pair(store, ['health']);
  const envelope = {
    deviceId: first.deviceId, capability: 'health', args: { value: 1 }, idempotencyKey: 'idem-12345678',
    deadlineAt: new Date(Date.now() + 60_000).toISOString(), timeoutMs: 5_000,
    approvalClass: 'safe', summary: 'Health check', maxOutputBytes: 1024, artifactPolicy: 'none'
  };
  const task = await store.createTask(envelope);
  assert.equal((await store.createTask(envelope)).id, task.id);
  await assert.rejects(() => store.createTask({ ...envelope, args: { value: 2 } }), (e: unknown) =>
    e instanceof StoreError && e.code === 'IDEMPOTENCY_CONFLICT');
  await assert.rejects(() => store.updateTask(second.deviceId, task.id, 'accepted'), /ownership/i);
  await store.updateTask(first.deviceId, task.id, 'accepted');
  await assert.rejects(() => store.updateTask(first.deviceId, task.id, 'succeeded', { ok: true }), /transition/i);
  await store.updateTask(first.deviceId, task.id, 'running');
  await store.cancelTask(task.id);
  assert.equal((await store.getTask(task.id)).status, 'cancelled');
  assert.ok((await store.snapshot()).audit.some((event: { action: string }) => event.action === 'task.cancelled'));
});

test('lists queued tasks for offline replay and creates private data files', async () => {
  const { path, store } = await fixture();
  const identity = await pair(store, ['health']);
  const task = await store.createTask({
    deviceId: identity.deviceId, capability: 'health', args: {}, idempotencyKey: 'offline-12345678',
    deadlineAt: new Date(Date.now() + 60_000).toISOString(), timeoutMs: 5_000,
    approvalClass: 'safe', summary: 'Offline task', maxOutputBytes: 1024, artifactPolicy: 'none'
  });
  assert.deepEqual((await store.queuedTasks(identity.deviceId)).map(t => t.id), [task.id]);
  const mode = (await import('node:fs/promises')).stat(path).then(s => s.mode & 0o777);
  if (process.platform !== 'win32') assert.equal(await mode, 0o600);
  await chmod(path, 0o600);
});
