import { createHash, createPublicKey, randomBytes, randomInt, randomUUID, timingSafeEqual, verify } from 'node:crypto';
import { mkdir, open, readFile, rename, stat, unlink } from 'node:fs/promises';
import { dirname } from 'node:path';

export type TaskStatus = 'queued' | 'accepted' | 'running' | 'succeeded' | 'failed' | 'cancelled';
export type TaskEnvelope = {
  deviceId: string;
  capability: string;
  args: unknown;
  idempotencyKey: string;
  deadlineAt: string;
  timeoutMs: number;
  approvalClass: string;
  summary: string;
  maxOutputBytes: number;
  artifactPolicy: string;
};
export type Task = TaskEnvelope & {
  id: string;
  status: TaskStatus;
  createdAt: string;
  updatedAt: string;
  result?: unknown;
};
type Device = {
  id: string;
  name: string;
  publicKey: string;
  credentialHash: string;
  capabilities: string[];
  revokedAt?: string;
  lastSeen?: string;
};
type Pairing = {
  id: string;
  pinHash: string;
  claimTokenHash: string;
  name: string;
  publicKey: string;
  capabilities: string[];
  expiresAt: number;
  approvedAt?: string;
  claimedAt?: string;
};
type RateLimit = { key: string; attempts: number[]; updatedAt: number };
type Nonce = { deviceId: string; nonceHash: string; expiresAt: number };
type AuditEvent = { at: string; action: string; deviceId?: string; detail?: unknown };
type DB = { version: 1; devices: Device[]; pairings: Pairing[]; tasks: Task[]; audit: AuditEvent[]; rateLimits: RateLimit[]; nonces: Nonce[] };

type StoreOptions = {
  pinTtlMs?: number;
  rateLimitMax?: number;
  rateLimitWindowMs?: number;
  rateLimitCapacity?: number;
  nonceTtlMs?: number;
  nonceCapacity?: number;
  clock?: () => number;
  randomInt?: (max: number) => number;
};

export class StoreError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, message: string, status = 400) {
    super(message);
    this.name = 'StoreError';
    this.code = code;
    this.status = status;
  }
}

const empty = (): DB => ({ version: 1, devices: [], pairings: [], tasks: [], audit: [], rateLimits: [], nonces: [] });
const hash = (value: string) => createHash('sha256').update(value).digest('hex');
const equalHash = (expected: string, value: string) => {
  const a = Buffer.from(expected, 'hex');
  const b = Buffer.from(hash(value), 'hex');
  return a.length === b.length && timingSafeEqual(a, b);
};
const canonical = (value: unknown): string => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(',')}}`;
  }
  return JSON.stringify(value);
};
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown) => typeof value === 'string' && value.length > 0;
const hexHash = (value: unknown) => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const validDB = (value: unknown): value is DB => {
  if (!record(value) || value.version !== 1) return false;
  if (!['devices', 'pairings', 'tasks', 'audit', 'rateLimits', 'nonces'].every(key => Array.isArray(value[key]))) return false;
  const devices = value.devices as unknown[];
  const pairings = value.pairings as unknown[];
  const tasks = value.tasks as unknown[];
  const audit = value.audit as unknown[];
  const rateLimits = value.rateLimits as unknown[];
  const nonces = value.nonces as unknown[];
  return devices.every(item => record(item) && text(item.id) && text(item.name) && text(item.publicKey) && hexHash(item.credentialHash) && Array.isArray(item.capabilities) && item.capabilities.every(text) && (item.revokedAt === undefined || text(item.revokedAt)) && (item.lastSeen === undefined || text(item.lastSeen)))
    && pairings.every(item => record(item) && text(item.id) && hexHash(item.pinHash) && hexHash(item.claimTokenHash) && text(item.name) && text(item.publicKey) && Array.isArray(item.capabilities) && item.capabilities.every(text) && typeof item.expiresAt === 'number' && Number.isFinite(item.expiresAt) && (item.approvedAt === undefined || text(item.approvedAt)) && (item.claimedAt === undefined || text(item.claimedAt)))
    && tasks.every(item => record(item) && text(item.id) && text(item.deviceId) && text(item.capability) && text(item.idempotencyKey) && text(item.deadlineAt) && typeof item.timeoutMs === 'number' && text(item.approvalClass) && text(item.summary) && typeof item.maxOutputBytes === 'number' && text(item.artifactPolicy) && ['queued', 'accepted', 'running', 'succeeded', 'failed', 'cancelled'].includes(String(item.status)) && text(item.createdAt) && text(item.updatedAt))
    && audit.every(item => record(item) && text(item.at) && text(item.action) && (item.deviceId === undefined || text(item.deviceId)))
    && rateLimits.every(item => record(item) && hexHash(item.key) && Array.isArray(item.attempts) && item.attempts.every(at => typeof at === 'number' && Number.isFinite(at)) && typeof item.updatedAt === 'number' && Number.isFinite(item.updatedAt))
    && nonces.every(item => record(item) && text(item.deviceId) && hexHash(item.nonceHash) && typeof item.expiresAt === 'number' && Number.isFinite(item.expiresAt));
};

export class ControlStore {
  private readonly path: string;
  private db: DB = empty();
  private readonly ready: Promise<void>;
  private queue: Promise<void>;
  private readonly options: Required<StoreOptions>;

  constructor(path: string, options: StoreOptions = {}) {
    this.path = path;
    this.options = {
      pinTtlMs: options.pinTtlMs ?? 300_000,
      rateLimitMax: options.rateLimitMax ?? 10,
      rateLimitWindowMs: options.rateLimitWindowMs ?? 60_000,
      rateLimitCapacity: options.rateLimitCapacity ?? 10_000,
      nonceTtlMs: options.nonceTtlMs ?? 60_000,
      nonceCapacity: options.nonceCapacity ?? 10_000,
      clock: options.clock ?? Date.now,
      randomInt: options.randomInt ?? randomInt
    };
    this.ready = this.load();
    this.queue = this.ready;
  }

  private async load() {
    try {
      const parsed: unknown = JSON.parse(await readFile(this.path, 'utf8'));
      if (!validDB(parsed)) throw new Error('invalid schema');
      this.db = parsed;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return;
      throw new StoreError('CORRUPT_STORE', `persistent store is corrupt: ${(error as Error).message}`, 500);
    }
  }

  private async durableWrite(db: DB) {
    await mkdir(dirname(this.path), { recursive: true, mode: 0o700 });
    const temp = `${this.path}.${process.pid}.${randomBytes(8).toString('hex')}.tmp`;
    const handle = await open(temp, 'wx', 0o600);
    try {
      await handle.writeFile(JSON.stringify(db));
      await handle.chmod(0o600);
      await handle.sync();
    } finally {
      await handle.close();
    }
    try {
      await rename(temp, this.path);
      if (process.platform !== 'win32') {
        const directory = await open(dirname(this.path), 'r');
        try { await directory.sync(); } finally { await directory.close(); }
      }
    } catch (error) {
      await unlink(temp).catch(() => undefined);
      throw error;
    }
  }

  private mutate<T>(fn: (db: DB) => T | Promise<T>): Promise<T> {
    const operation = this.queue.then(async () => {
      const draft = structuredClone(this.db);
      const result = await fn(draft);
      await this.durableWrite(draft);
      this.db = draft;
      return result;
    });
    this.queue = operation.then(() => undefined, () => undefined);
    return operation;
  }

  private audit(db: DB, action: string, deviceId?: string, detail?: unknown) {
    db.audit.push({ at: new Date(this.options.clock()).toISOString(), action, deviceId, detail });
  }

  private consumeRateLimit(db: DB, source: string) {
    const now = this.options.clock();
    db.rateLimits = db.rateLimits
      .map(entry => ({ ...entry, attempts: entry.attempts.filter(at => now - at < this.options.rateLimitWindowMs) }))
      .filter(entry => entry.attempts.length > 0);
    let entry = db.rateLimits.find(item => item.key === hash(source));
    if (!entry) {
      entry = { key: hash(source), attempts: [], updatedAt: now };
      db.rateLimits.push(entry);
    }
    if (entry.attempts.length >= this.options.rateLimitMax) throw new StoreError('RATE_LIMITED', 'rate limit exceeded', 429);
    entry.attempts.push(now);
    entry.updatedAt = now;
    if (db.rateLimits.length > this.options.rateLimitCapacity) {
      db.rateLimits.sort((a, b) => b.updatedAt - a.updatedAt);
      db.rateLimits.length = this.options.rateLimitCapacity;
    }
  }

  async startPairing(name: string, publicKey: string, capabilities: string[], source: string) {
    return this.mutate(db => {
      this.consumeRateLimit(db, source);
      const now = this.options.clock();
      const pin = String(this.options.randomInt(1_000_000)).padStart(6, '0');
      const claimToken = randomBytes(32).toString('base64url');
      const pairingId = randomUUID();
      const expiresAt = now + this.options.pinTtlMs;
      db.pairings.push({ id: pairingId, pinHash: hash(pin), claimTokenHash: hash(claimToken), name, publicKey, capabilities, expiresAt });
      this.audit(db, 'pairing.created', undefined, { pairingId, name });
      return { pairingId, pin, claimToken, expiresAt: new Date(expiresAt).toISOString() };
    });
  }

  async approvePairing(pairingId: string) {
    return this.mutate(db => {
      const pairing = db.pairings.find(item => item.id === pairingId);
      if (!pairing) throw new StoreError('NOT_FOUND', 'pairing not found', 404);
      if (pairing.expiresAt <= this.options.clock()) throw new StoreError('PAIRING_EXPIRED', 'pairing expired', 410);
      if (pairing.approvedAt) return { status: 'approved' as const };
      pairing.approvedAt = new Date(this.options.clock()).toISOString();
      this.audit(db, 'pairing.approved', undefined, { pairingId });
      return { status: 'approved' as const };
    });
  }

  async approvePairingByPin(pin: string) {
    if (!/^\d{6}$/.test(pin)) throw new StoreError('VALIDATION_ERROR', 'PIN must contain six digits', 422);
    return this.mutate(db => {
      const now = this.options.clock();
      const candidates = db.pairings.filter(item => !item.claimedAt && item.expiresAt > now && equalHash(item.pinHash, pin));
      if (candidates.length === 0) throw new StoreError('NOT_FOUND', 'active pairing not found', 404);
      if (candidates.length > 1) throw new StoreError('PIN_AMBIGUOUS', 'multiple active pairings use this PIN', 409);
      const pairing = candidates[0];
      if (!pairing.approvedAt) {
        pairing.approvedAt = new Date(now).toISOString();
        this.audit(db, 'pairing.approved', undefined, { pairingId: pairing.id });
      }
      return { status: 'approved' as const, pairingId: pairing.id };
    });
  }

  async getPairingStatus(pairingId: string, claimToken: string): Promise<{ status: 'pending' | 'approved'; identity?: { deviceId: string; credential: string } }> {
    await this.ready;
    const pairing = this.db.pairings.find(item => item.id === pairingId);
    if (!pairing || !equalHash(pairing.claimTokenHash, claimToken)) throw new StoreError('UNAUTHORIZED', 'unauthorized claim token', 401);
    if (pairing.expiresAt <= this.options.clock()) throw new StoreError('PAIRING_EXPIRED', 'pairing expired', 410);
    if (!pairing.approvedAt || pairing.claimedAt) return { status: pairing.approvedAt ? 'approved' : 'pending' };
    return this.mutate(db => {
      const current = db.pairings.find(item => item.id === pairingId)!;
      if (current.claimedAt) return { status: 'approved' as const };
      const credential = randomBytes(32).toString('base64url');
      const deviceId = randomUUID();
      current.claimedAt = new Date(this.options.clock()).toISOString();
      db.devices.push({ id: deviceId, name: current.name, publicKey: current.publicKey, credentialHash: hash(credential), capabilities: current.capabilities });
      this.audit(db, 'device.paired', deviceId, { pairingId });
      return { status: 'approved' as const, identity: { deviceId, credential } };
    });
  }

  async authenticate(deviceId: string, credential: string) {
    await this.ready;
    const device = this.db.devices.find(item => item.id === deviceId && !item.revokedAt);
    return !!device && equalHash(device.credentialHash, credential);
  }

  async authenticateProof(deviceId: string, credential: string, timestamp: string, nonce: string, signature: string) {
    if (!/^\d{10,13}$/.test(timestamp)) return false;
    const timestampNumber = Number(timestamp);
    const timestampMs = timestampNumber < 1_000_000_000_000 ? timestampNumber * 1000 : timestampNumber;
    if (!Number.isSafeInteger(timestampNumber) || Math.abs(this.options.clock() - timestampMs) > this.options.nonceTtlMs) return false;
    if (nonce.length < 8 || nonce.length > 128 || !/^[A-Za-z0-9_-]+$/.test(nonce)) return false;
    if (!await this.authenticate(deviceId, credential)) return false;
    const device = this.db.devices.find(item => item.id === deviceId)!;
    let valid = false;
    try {
      const raw = Buffer.from(device.publicKey, 'base64');
      if (raw.length !== 32 || Buffer.from(device.publicKey, 'base64').toString('base64') !== device.publicKey) return false;
      const key = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]), format: 'der', type: 'spki' });
      valid = verify(null, Buffer.from(`${deviceId}.${timestamp}.${nonce}`), key, Buffer.from(signature, 'base64url'));
    } catch { return false; }
    if (!valid) return false;
    try {
      return await this.mutate(db => {
        const now = this.options.clock();
        db.nonces = db.nonces.filter(item => item.expiresAt > now);
        const nonceHash = hash(`${deviceId}.${nonce}`);
        if (db.nonces.some(item => item.nonceHash === nonceHash)) throw new StoreError('NONCE_REPLAY', 'nonce replay', 401);
        db.nonces.push({ deviceId, nonceHash, expiresAt: now + this.options.nonceTtlMs });
        if (db.nonces.length > this.options.nonceCapacity) db.nonces.splice(0, db.nonces.length - this.options.nonceCapacity);
        const current = db.devices.find(item => item.id === deviceId && !item.revokedAt);
        if (!current) throw new StoreError('UNAUTHORIZED', 'device revoked', 401);
        current.lastSeen = new Date(now).toISOString();
        return true;
      });
    } catch (error) {
      if (error instanceof StoreError && ['NONCE_REPLAY', 'UNAUTHORIZED'].includes(error.code)) return false;
      throw error;
    }
  }

  async revoke(deviceId: string) {
    return this.mutate(db => {
      const device = db.devices.find(item => item.id === deviceId);
      if (!device) throw new StoreError('NOT_FOUND', 'device not found', 404);
      if (!device.revokedAt) {
        device.revokedAt = new Date(this.options.clock()).toISOString();
        this.audit(db, 'device.revoked', deviceId);
      }
    });
  }

  async heartbeat(deviceId: string) {
    return this.mutate(db => {
      const device = db.devices.find(item => item.id === deviceId && !item.revokedAt);
      if (!device) throw new StoreError('UNAUTHORIZED', 'device revoked', 401);
      device.lastSeen = new Date(this.options.clock()).toISOString();
    });
  }

  async listDevices() {
    await this.ready;
    return structuredClone(this.db.devices.map(({ credentialHash: _, ...device }) => device));
  }

  async createTask(envelope: TaskEnvelope) {
    return this.mutate(db => {
      const fingerprint = canonical(envelope);
      const existing = db.tasks.find(item => item.deviceId === envelope.deviceId && item.idempotencyKey === envelope.idempotencyKey);
      if (existing) {
        const { id: _id, status: _status, createdAt: _createdAt, updatedAt: _updatedAt, result: _result, ...existingEnvelope } = existing;
        if (canonical(existingEnvelope) !== fingerprint) throw new StoreError('IDEMPOTENCY_CONFLICT', 'idempotency key conflicts with another request', 409);
        return structuredClone(existing);
      }
      const device = db.devices.find(item => item.id === envelope.deviceId && !item.revokedAt);
      if (!device) throw new StoreError('NOT_FOUND', 'device unavailable', 404);
      if (!device.capabilities.includes(envelope.capability)) throw new StoreError('CAPABILITY_FORBIDDEN', 'capability unavailable', 422);
      const now = new Date(this.options.clock()).toISOString();
      const task: Task = { ...structuredClone(envelope), id: randomUUID(), status: 'queued', createdAt: now, updatedAt: now };
      db.tasks.push(task);
      this.audit(db, 'task.created', envelope.deviceId, { taskId: task.id, capability: task.capability });
      return structuredClone(task);
    });
  }

  async getTask(taskId: string) {
    await this.ready;
    const task = this.db.tasks.find(item => item.id === taskId);
    if (!task) throw new StoreError('NOT_FOUND', 'task not found', 404);
    return structuredClone(task);
  }

  async queuedTasks(deviceId: string) {
    await this.ready;
    return structuredClone(this.db.tasks.filter(item => item.deviceId === deviceId && item.status === 'queued' && Date.parse(item.deadlineAt) > this.options.clock()));
  }

  async updateTask(deviceId: string, taskId: string, status: TaskStatus, result?: unknown) {
    const allowed: Record<TaskStatus, TaskStatus[]> = { queued: ['accepted'], accepted: ['running'], running: ['succeeded', 'failed'], succeeded: [], failed: [], cancelled: [] };
    return this.mutate(db => {
      const task = db.tasks.find(item => item.id === taskId);
      if (!task) throw new StoreError('NOT_FOUND', 'task not found', 404);
      if (task.deviceId !== deviceId) throw new StoreError('FORBIDDEN', 'task ownership mismatch', 403);
      if (!allowed[task.status].includes(status)) throw new StoreError('INVALID_TRANSITION', `invalid task transition ${task.status} -> ${status}`, 409);
      if (result !== undefined && Buffer.byteLength(JSON.stringify(result)) > task.maxOutputBytes) throw new StoreError('OUTPUT_TOO_LARGE', 'task output exceeds maxOutputBytes', 413);
      task.status = status;
      task.updatedAt = new Date(this.options.clock()).toISOString();
      if (result !== undefined) task.result = structuredClone(result);
      this.audit(db, 'task.updated', deviceId, { taskId, status });
      return structuredClone(task);
    });
  }

  async cancelTask(taskId: string) {
    return this.mutate(db => {
      const task = db.tasks.find(item => item.id === taskId);
      if (!task) throw new StoreError('NOT_FOUND', 'task not found', 404);
      if (['succeeded', 'failed', 'cancelled'].includes(task.status)) throw new StoreError('INVALID_TRANSITION', `cannot cancel ${task.status} task`, 409);
      task.status = 'cancelled';
      task.updatedAt = new Date(this.options.clock()).toISOString();
      this.audit(db, 'task.cancelled', task.deviceId, { taskId });
      return structuredClone(task);
    });
  }

  async raw() { await this.ready; return readFile(this.path, 'utf8'); }
  async snapshot() { await this.ready; await this.queue; return structuredClone(this.db); }
  async fileMode() { return (await stat(this.path)).mode & 0o777; }
}
