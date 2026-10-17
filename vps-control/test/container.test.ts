import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');

test('container prepares /data and readable application files for the unprivileged runtime user', async () => {
  const dockerfile = await readFile(join(root, 'Dockerfile'), 'utf8');
  assert.match(dockerfile, /install -d -o node -g node -m 0700 \/data/);
  assert.match(dockerfile, /chmod -R u=rwX,go=rX \/app/);
  assert.match(dockerfile, /USER node/);
});

test('container process binds through its published port', async () => {
  const server = await readFile(join(root, 'src', 'server.ts'), 'utf8');
  assert.match(server, /listen\(port, '0\.0\.0\.0'\)/);
});
