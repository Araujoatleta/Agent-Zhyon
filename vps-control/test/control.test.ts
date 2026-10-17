import { afterEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { mkdtemp } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import WebSocket from 'ws';
import { createControlPlane, type ControlPlane } from '../src/server.ts';

const adminToken = 'admin-token-at-least-24-characters';
const planes: ControlPlane[] = [];
afterEach(async () => Promise.all(planes.splice(0).map(plane => plane.close())));

async function start(maxPayload = 2048) {
  const dir = await mkdtemp(join(tmpdir(), 'coucou-http-'));
  const plane = createControlPlane({ adminToken, dataFile: join(dir, 'control.json'), maxPayload });
  const address = await plane.listen(0, '127.0.0.1');
  planes.push(plane);
  return { plane, base: `http://127.0.0.1:${address.port}`, ws: `ws://127.0.0.1:${address.port}` };
}
const request = async (base: string, path: string, init: RequestInit = {}) => {
  const response = await fetch(`${base}${path}`, init);
  const json = await response.json();
  return { response, json };
};
const adminHeaders = { authorization: `Bearer ${adminToken}`, 'content-type': 'application/json' };
const rawKey = (publicKey: ReturnType<typeof generateKeyPairSync>['publicKey']) =>
  publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');

async function pair(base: string) {
  const keys = generateKeyPairSync('ed25519');
  const started = await request(base, '/control/v1/pair/start', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: 'Worker PC', publicKey: rawKey(keys.publicKey), capabilities: ['health'] })
  });
  assert.equal(started.response.status, 201);
  assert.deepEqual(Object.keys(started.json).sort(), ['claimToken', 'expiresAt', 'pairingId', 'pin']);
  const pending = await request(base, `/v1/pair/status/${started.json.pairingId}`, {
    headers: { authorization: `Bearer ${started.json.claimToken}` }
  });
  assert.equal(pending.json.status, 'pending');
  const approved = await request(base, `/v1/admin/pairings/${started.json.pairingId}/approve`, { method: 'POST', headers: adminHeaders });
  assert.equal(approved.response.status, 200);
  const status = await request(base, `/v1/pair/status/${started.json.pairingId}`, {
    headers: { authorization: `Bearer ${started.json.claimToken}` }
  });
  assert.equal(status.json.status, 'approved');
  return { ...status.json.identity, keys } as { deviceId: string; credential: string; keys: typeof keys };
}

function connect(url: string, identity: Awaited<ReturnType<typeof pair>>) {
  const timestamp = String(Math.floor(Date.now() / 1000));
  const nonce = `nonce-${Math.random().toString(36).slice(2)}-12345678`;
  const signature = sign(null, Buffer.from(`${identity.deviceId}.${timestamp}.${nonce}`), identity.keys.privateKey).toString('base64url');
  return new WebSocket(`${url}/control/v1/worker/ws?device_id=${identity.deviceId}`, {
    headers: {
      authorization: `Bearer ${identity.credential}`,
      'x-coucou-timestamp': timestamp,
      'x-coucou-nonce': nonce,
      'x-coucou-signature': signature
    }
  });
}
const opened = (socket: WebSocket) => new Promise<void>((resolve, reject) => {
  socket.once('open', resolve); socket.once('error', reject);
});
const message = (socket: WebSocket) => new Promise<any>((resolve, reject) => {
  socket.once('message', raw => resolve(JSON.parse(String(raw)))); socket.once('error', reject);
});
const closed = (socket: WebSocket) => new Promise<{ code: number; reason: string }>(resolve =>
  socket.once('close', (code, reason) => resolve({ code, reason: String(reason) })));

test('REST pairing flow validates schemas and uses correct HTTP status classes', async () => {
  const { base } = await start();
  assert.equal((await request(base, '/healthz')).response.status, 200);
  assert.equal((await request(base, '/v1/admin/devices')).response.status, 401);
  const malformed = await request(base, '/v1/pair/start', { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{' });
  assert.equal(malformed.response.status, 400);
  const invalid = await request(base, '/v1/pair/start', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name: '', publicKey: 'bad', capabilities: [] }) });
  assert.equal(invalid.response.status, 422);
  const missing = await request(base, '/v1/admin/tasks/no-such-task', { headers: adminHeaders });
  assert.equal(missing.response.status, 404);
});

test('REST and WebSocket deliver offline tasks, enforce transitions, expose GET and cancellation', async () => {
  const { base, ws } = await start();
  const identity = await pair(base);
  const envelope = {
    deviceId: identity.deviceId, capability: 'health', args: { detail: true }, idempotencyKey: 'request-12345678',
    deadlineAt: new Date(Date.now() + 60_000).toISOString(), timeoutMs: 5_000, approvalClass: 'safe',
    summary: 'Run health check', maxOutputBytes: 1024, artifactPolicy: 'none'
  };
  const created = await request(base, '/v1/admin/tasks', { method: 'POST', headers: adminHeaders, body: JSON.stringify(envelope) });
  assert.equal(created.response.status, 202);
  const conflict = await request(base, '/v1/admin/tasks', { method: 'POST', headers: adminHeaders, body: JSON.stringify({ ...envelope, args: { detail: false } }) });
  assert.equal(conflict.response.status, 409);

  const socket = connect(ws, identity);
  const replayPromise = message(socket);
  await opened(socket);
  const replay = await replayPromise;
  assert.equal(replay.type, 'task');
  assert.equal(replay.task.id, created.json.id);
  socket.send(JSON.stringify({ type: 'task_status', taskId: created.json.id, status: 'accepted' }));
  socket.send(JSON.stringify({ type: 'task_status', taskId: created.json.id, status: 'running' }));
  await new Promise(resolve => setTimeout(resolve, 30));
  const fetched = await request(base, `/control/v1/admin/tasks/${created.json.id}`, { headers: adminHeaders });
  assert.equal(fetched.response.status, 200);
  assert.equal(fetched.json.status, 'running');
  const cancelNotice = message(socket);
  const cancelled = await request(base, `/v1/admin/tasks/${created.json.id}/cancel`, { method: 'POST', headers: adminHeaders });
  assert.equal(cancelled.response.status, 200);
  assert.equal((await cancelNotice).type, 'task_cancel');
  const audit = await request(base, '/v1/admin/audit', { headers: adminHeaders });
  assert.ok(audit.json.some((entry: { action: string }) => entry.action === 'task.cancelled'));
  socket.close();
});

test('keeps multiple sockets per device and revocation closes all of them', async () => {
  const { base, ws } = await start();
  const identity = await pair(base);
  const first = connect(ws, identity);
  const second = connect(ws, identity);
  await Promise.all([opened(first), opened(second)]);
  const firstClosed = closed(first), secondClosed = closed(second);
  const revoked = await request(base, `/v1/admin/devices/${identity.deviceId}/revoke`, { method: 'POST', headers: adminHeaders });
  assert.equal(revoked.response.status, 200);
  assert.deepEqual((await Promise.all([firstClosed, secondClosed])).map(event => event.code), [4003, 4003]);
  const rejected = connect(ws, identity);
  await assert.rejects(() => opened(rejected));
});

test('rejects replayed proof and closes oversized WebSocket messages with 1009', async () => {
  const { base, ws } = await start(256);
  const identity = await pair(base);
  const timestamp = String(Math.floor(Date.now() / 1000));
  const nonce = 'fixed-nonce-123456';
  const signature = sign(null, Buffer.from(`${identity.deviceId}.${timestamp}.${nonce}`), identity.keys.privateKey).toString('base64url');
  const headers = { authorization: `Bearer ${identity.credential}`, 'x-coucou-timestamp': timestamp, 'x-coucou-nonce': nonce, 'x-coucou-signature': signature };
  const first = new WebSocket(`${ws}/v1/worker/ws?device_id=${identity.deviceId}`, { headers });
  await opened(first);
  const replay = new WebSocket(`${ws}/v1/worker/ws?device_id=${identity.deviceId}`, { headers });
  await assert.rejects(() => opened(replay));
  const closing = closed(first);
  first.send(JSON.stringify({ type: 'heartbeat', padding: 'x'.repeat(1024) }));
  assert.equal((await closing).code, 1009);
});

test('rejects HTTP bodies larger than the configured limit', async () => {
  const { base } = await start(256);
  const response = await fetch(`${base}/v1/pair/start`, {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ padding: 'x'.repeat(1024) })
  });
  assert.equal(response.status, 413);
});
