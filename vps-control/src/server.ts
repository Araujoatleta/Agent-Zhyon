import * as http from 'node:http';
import { pathToFileURL } from 'node:url';
import { WebSocketServer, WebSocket } from 'ws';
import { ControlStore, StoreError, type TaskEnvelope, type TaskStatus } from './store.ts';

export type ControlPlane = {
  store: ControlStore;
  server: http.Server;
  listen(port?: number, host?: string): Promise<{ port: number; address: string }>;
  close(): Promise<void>;
};
type Options = { adminToken: string; dataFile: string; maxPayload?: number; store?: ControlStore };
class HttpError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(status: number, code: string, message: string) {
    super(message); this.status = status; this.code = code;
  }
}

const bearer = (request: http.IncomingMessage) => {
  const value = request.headers.authorization;
  return typeof value === 'string' && value.startsWith('Bearer ') ? value.slice(7) : undefined;
};
const normalizePath = (path: string) => path.replace(/^\/control(?=\/|$)/, '') || '/';
const sendJson = (response: http.ServerResponse, status: number, body: unknown) => {
  if (response.headersSent) return;
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' });
  response.end(JSON.stringify(body));
};
const isObject = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const stringField = (body: Record<string, unknown>, key: string, min: number, max: number, pattern?: RegExp) => {
  const value = body[key];
  if (typeof value !== 'string' || value.length < min || value.length > max || (pattern && !pattern.test(value))) {
    throw new HttpError(422, 'VALIDATION_ERROR', `invalid ${key}`);
  }
  return value;
};
const exactBase64 = (value: string, bytes: number) => {
  try {
    const decoded = Buffer.from(value, 'base64');
    return decoded.length === bytes && decoded.toString('base64') === value;
  } catch { return false; }
};

export function createControlPlane(options: Options): ControlPlane {
  if (typeof options.adminToken !== 'string' || options.adminToken.length < 24) throw new Error('adminToken must contain at least 24 characters');
  const maxPayload = options.maxPayload ?? 65_536;
  if (!Number.isSafeInteger(maxPayload) || maxPayload < 128 || maxPayload > 1_048_576) throw new Error('maxPayload must be between 128 and 1048576');
  const store = options.store ?? new ControlStore(options.dataFile);
  const sockets = new Map<string, Set<WebSocket>>();

  const readBody = async (request: http.IncomingMessage) => {
    const declared = Number(request.headers['content-length'] ?? 0);
    if (Number.isFinite(declared) && declared > maxPayload) throw new HttpError(413, 'PAYLOAD_TOO_LARGE', 'request body too large');
    const chunks: Buffer[] = [];
    let size = 0;
    for await (const chunk of request) {
      const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      size += buffer.length;
      if (size > maxPayload) throw new HttpError(413, 'PAYLOAD_TOO_LARGE', 'request body too large');
      chunks.push(buffer);
    }
    if (size === 0) return {};
    try {
      const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString('utf8'));
      if (!isObject(parsed)) throw new Error('object required');
      return parsed;
    } catch {
      throw new HttpError(400, 'INVALID_JSON', 'request body must be a JSON object');
    }
  };
  const requireAdmin = (request: http.IncomingMessage) => {
    if (bearer(request) !== options.adminToken) throw new HttpError(401, 'UNAUTHORIZED', 'admin authentication required');
  };
  const pairingInput = (body: Record<string, unknown>) => {
    const name = stringField(body, 'name', 1, 100);
    const publicKey = stringField(body, 'publicKey', 40, 48);
    if (!exactBase64(publicKey, 32)) throw new HttpError(422, 'VALIDATION_ERROR', 'publicKey must be a base64 raw Ed25519 key');
    if (!Array.isArray(body.capabilities) || body.capabilities.length > 32 || !body.capabilities.every(value => typeof value === 'string' && /^[a-zA-Z0-9._:-]{1,64}$/.test(value))) {
      throw new HttpError(422, 'VALIDATION_ERROR', 'invalid capabilities');
    }
    return { name, publicKey, capabilities: [...new Set(body.capabilities as string[])] };
  };
  const taskInput = (body: Record<string, unknown>): TaskEnvelope => {
    const deviceId = stringField(body, 'deviceId', 1, 100, /^[a-f0-9-]+$/i);
    const capability = stringField(body, 'capability', 1, 64, /^[a-zA-Z0-9._:-]+$/);
    const idempotencyKey = stringField(body, 'idempotencyKey', 8, 128, /^[a-zA-Z0-9._:-]+$/);
    const deadlineAt = stringField(body, 'deadlineAt', 20, 40);
    if (!Number.isFinite(Date.parse(deadlineAt)) || Date.parse(deadlineAt) <= Date.now()) throw new HttpError(422, 'VALIDATION_ERROR', 'deadlineAt must be a future timestamp');
    if (!Number.isSafeInteger(body.timeoutMs) || (body.timeoutMs as number) < 100 || (body.timeoutMs as number) > 3_600_000) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid timeoutMs');
    const approvalClass = stringField(body, 'approvalClass', 1, 32, /^[a-zA-Z0-9._:-]+$/);
    const summary = stringField(body, 'summary', 1, 500);
    if (!Number.isSafeInteger(body.maxOutputBytes) || (body.maxOutputBytes as number) < 1 || (body.maxOutputBytes as number) > 1_048_576) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid maxOutputBytes');
    const artifactPolicy = stringField(body, 'artifactPolicy', 1, 32, /^[a-zA-Z0-9._:-]+$/);
    if (body.args === undefined || Buffer.byteLength(JSON.stringify(body.args)) > 32_768) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid args');
    return { deviceId, capability, args: body.args, idempotencyKey, deadlineAt, timeoutMs: body.timeoutMs as number, approvalClass, summary, maxOutputBytes: body.maxOutputBytes as number, artifactPolicy };
  };
  const broadcast = (deviceId: string, payload: unknown) => {
    const encoded = JSON.stringify(payload);
    for (const socket of sockets.get(deviceId) ?? []) if (socket.readyState === WebSocket.OPEN) socket.send(encoded);
  };

  const server = http.createServer(async (request, response) => {
    try {
      const url = new URL(request.url ?? '/', `http://${request.headers.host ?? 'localhost'}`);
      const path = normalizePath(url.pathname);
      if (path === '/healthz' && request.method === 'GET') return sendJson(response, 200, { ok: true });

      if (path === '/v1/pair/start' && request.method === 'POST') {
        const input = pairingInput(await readBody(request));
        const source = request.socket.remoteAddress ?? 'unknown';
        return sendJson(response, 201, await store.startPairing(input.name, input.publicKey, input.capabilities, source));
      }
      const pairStatus = path.match(/^\/v1\/pair\/status\/([^/]+)$/);
      if (pairStatus && request.method === 'GET') {
        const token = bearer(request);
        if (!token) throw new HttpError(401, 'UNAUTHORIZED', 'claim token required');
        return sendJson(response, 200, await store.getPairingStatus(pairStatus[1], token));
      }

      requireAdmin(request);
      if (path === '/v1/admin/pairings/approve' && request.method === 'POST') {
        const input = await readBody(request);
        if (!isObject(input)) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid pairing approval');
        const pin = stringField(input, 'pin', 6, 6);
        return sendJson(response, 200, await store.approvePairingByPin(pin));
      }
      const approve = path.match(/^\/v1\/admin\/pairings\/([^/]+)\/approve$/);
      if (approve && request.method === 'POST') return sendJson(response, 200, await store.approvePairing(approve[1]));
      if (path === '/v1/admin/devices' && request.method === 'GET') return sendJson(response, 200, await store.listDevices());
      const revoke = path.match(/^\/v1\/admin\/devices\/([^/]+)\/revoke$/);
      if (revoke && request.method === 'POST') {
        await store.revoke(revoke[1]);
        for (const socket of sockets.get(revoke[1]) ?? []) socket.close(4003, 'revoked');
        return sendJson(response, 200, { ok: true });
      }
      if (path === '/v1/admin/tasks' && request.method === 'POST') {
        const task = await store.createTask(taskInput(await readBody(request)));
        broadcast(task.deviceId, { type: 'task', task });
        return sendJson(response, 202, task);
      }
      const getTask = path.match(/^\/v1\/admin\/tasks\/([^/]+)$/);
      if (getTask && request.method === 'GET') return sendJson(response, 200, await store.getTask(getTask[1]));
      const cancel = path.match(/^\/v1\/admin\/tasks\/([^/]+)\/cancel$/);
      if (cancel && request.method === 'POST') {
        const task = await store.cancelTask(cancel[1]);
        broadcast(task.deviceId, { type: 'task_cancel', taskId: task.id });
        return sendJson(response, 200, task);
      }
      if (path === '/v1/admin/audit' && request.method === 'GET') return sendJson(response, 200, (await store.snapshot()).audit);
      throw new HttpError(404, 'NOT_FOUND', 'route not found');
    } catch (error) {
      if (error instanceof HttpError || error instanceof StoreError) return sendJson(response, error.status, { error: error.code, message: error.message });
      console.error('request failed', error);
      return sendJson(response, 500, { error: 'INTERNAL_ERROR', message: 'internal server error' });
    }
  });

  const wss = new WebSocketServer({ noServer: true, maxPayload });
  server.on('upgrade', async (request, socket, head) => {
    try {
      const url = new URL(request.url ?? '/', `http://${request.headers.host ?? 'localhost'}`);
      if (normalizePath(url.pathname) !== '/v1/worker/ws') throw new HttpError(404, 'NOT_FOUND', 'route not found');
      const deviceId = url.searchParams.get('device_id') ?? '';
      const credential = bearer(request) ?? '';
      const timestamp = String(request.headers['x-coucou-timestamp'] ?? '');
      const nonce = String(request.headers['x-coucou-nonce'] ?? '');
      const signature = String(request.headers['x-coucou-signature'] ?? '');
      if (!await store.authenticateProof(deviceId, credential, timestamp, nonce, signature)) throw new HttpError(401, 'UNAUTHORIZED', 'invalid worker proof');
      wss.handleUpgrade(request, socket, head, ws => wss.emit('connection', ws, deviceId));
    } catch (error) {
      const status = error instanceof HttpError || error instanceof StoreError ? error.status : 500;
      const phrase = status === 404 ? 'Not Found' : status === 500 ? 'Internal Server Error' : 'Unauthorized';
      socket.write(`HTTP/1.1 ${status} ${phrase}\r\nConnection: close\r\n\r\n`);
      socket.destroy();
    }
  });
  wss.on('connection', (socket, deviceId: string) => {
    let set = sockets.get(deviceId);
    if (!set) { set = new Set(); sockets.set(deviceId, set); }
    set.add(socket);
    socket.on('error', () => undefined);
    socket.on('message', async raw => {
      try {
        const parsed: unknown = JSON.parse(String(raw));
        if (!isObject(parsed)) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid message');
        if (parsed.type === 'heartbeat') await store.heartbeat(deviceId);
        else if (parsed.type === 'task_status' || parsed.type === 'task_result') {
          const taskId = stringField(parsed, 'taskId', 1, 100);
          const status = stringField(parsed, 'status', 1, 16) as TaskStatus;
          if (!['accepted', 'running', 'succeeded', 'failed'].includes(status)) throw new HttpError(422, 'VALIDATION_ERROR', 'invalid task status');
          await store.updateTask(deviceId, taskId, status, parsed.result ?? parsed.evidence);
        } else throw new HttpError(422, 'VALIDATION_ERROR', 'unknown message type');
      } catch (error) {
        const code = error instanceof StoreError && error.status === 403 ? 4003 : 1008;
        socket.close(code, error instanceof Error ? error.message.slice(0, 120) : 'invalid message');
      }
    });
    socket.on('close', () => {
      const current = sockets.get(deviceId);
      current?.delete(socket);
      if (current?.size === 0) sockets.delete(deviceId);
    });
    void store.queuedTasks(deviceId).then(tasks => {
      for (const task of tasks) if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: 'task', task }));
    }).catch(() => socket.close(1011, 'store unavailable'));
  });

  return {
    store,
    server,
    listen(port = 8790, host = '0.0.0.0') {
      return new Promise((resolve, reject) => {
        const onError = (error: Error) => reject(error);
        server.once('error', onError);
        server.listen(port, host, () => {
          server.off('error', onError);
          const address = server.address();
          if (!address || typeof address === 'string') return reject(new Error('unexpected server address'));
          resolve({ port: address.port, address: address.address });
        });
      });
    },
    close() {
      for (const socket of wss.clients) socket.terminate();
      return new Promise((resolve, reject) => {
        wss.close(() => server.close(error => error ? reject(error) : resolve()));
      });
    }
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const adminToken = process.env.ADMIN_TOKEN;
  if (!adminToken) throw new Error('ADMIN_TOKEN (>=24 chars) required');
  const plane = createControlPlane({ adminToken, dataFile: process.env.DATA_FILE ?? '/data/control.json', maxPayload: Number(process.env.MAX_PAYLOAD ?? 65_536) });
  const port = Number(process.env.PORT ?? 8790);
  plane.listen(port, '0.0.0.0').then(address => console.log(`control plane listening on ${address.address}:${address.port}`));
}
