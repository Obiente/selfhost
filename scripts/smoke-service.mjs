// Opt-in native user-service integration test. Never uses the normal workspace.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { dirname, resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { createServer } from 'node:net';
const binary = resolve(process.argv[2]);
const data = mkdtempSync(join(tmpdir(), 'selfhost-service-smoke-'));
const run = (...args) => {
  const result = spawnSync(binary, ['--data-dir', data, 'dashboard', ...args], {
    encoding: 'utf8',
    timeout: 60000,
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
};
const probe = createServer();
await new Promise((resolve) => probe.listen(0, '127.0.0.1', resolve));
const port = probe.address().port;
await new Promise((resolve) => probe.close(resolve));
const origin = `http://127.0.0.1:${port}`;
let installed = false;
try {
  run('install', '--port', String(port));
  installed = true;
  run('start');
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${origin}/auth/info`)).ok) break;
    } catch {
      /* Starting. */
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  const log = join(data, 'dashboard-service/dashboard.log');
  const token = [...readFileSync(log, 'utf8').matchAll(/#token=([a-f0-9]+)/g)].at(-1)?.[1];
  assert.ok(token, 'Service did not retain its private recovery link');
  const login = await fetch(`${origin}/auth/local`, {
    method: 'POST',
    headers: { Origin: origin, 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  });
  assert.equal(login.status, 200);
  const headers = {
    Origin: origin,
    Cookie: login.headers.get('set-cookie').split(';')[0],
    'x-selfhost-client': (await login.json()).client_key,
  };
  const summary = await fetch(`${origin}/api/dashboard/runtime`, { headers });
  assert.equal(summary.status, 200);
  assert.equal((await summary.json()).supervised, true);
  run('restart');
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`${origin}/api/account`, { headers })).status === 401) break;
    } catch {
      /* Restarting. */
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  assert.equal((await fetch(`${origin}/api/account`, { headers })).status, 401);
  run('stop');
  run('uninstall');
  installed = false;
  assert.equal(existsSync(join(data, 'dashboard-service/service.json')), false);
  console.log(
    'Native user service passed install, background start, private login, restart, stop and uninstall.',
  );
} finally {
  if (installed) {
    try {
      run('uninstall');
      installed = false;
    } catch {
      console.error('Test service cleanup needs attention; private fixture retained at', data);
    }
  } else if (existsSync(join(data, 'dashboard-service/service.json'))) {
    try {
      run('uninstall');
    } catch {
      /* Preserve incomplete native setup for inspection. */
    }
  }
  if (!installed && !existsSync(join(data, 'dashboard-service/service.json'))) {
    assert.equal(dirname(data), resolve(tmpdir()));
    rmSync(data, { recursive: true, force: true });
  }
}
