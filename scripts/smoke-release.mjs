// Test an isolated release executable without a checkout, credentials or Docker mutations.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createServer } from 'node:net';
const executable = resolve(process.argv[2]);
const version = JSON.parse(readFileSync('packages/selfhost/package.json', 'utf8')).version;
const data = mkdtempSync(join(tmpdir(), 'selfhost-release-smoke-'));
const env = { ...process.env, SELFHOST_BINARY: '' };
let child;
try {
  const run = (args) => {
    const result = spawnSync(executable, ['--data-dir', data, ...args], {
      cwd: data,
      env,
      encoding: 'utf8',
      timeout: 30000,
      windowsHide: true,
    });
    assert.equal(
      result.status,
      0,
      result.error?.message || result.stderr || `Process terminated by ${result.signal}`,
    );
    return result.stdout;
  };
  assert.equal(run(['--version']).trim(), `selfhost ${version}`);
  assert.match(run(['catalog']), /homarr/);
  assert.match(run(['deployments']), /nextcloud/);
  assert.match(run(['identity', '--help']), /register-account/);
  assert.doesNotMatch(run(['identity', '--help']), /selfhost\.exe/);
  const service = JSON.parse(run(['dashboard', 'install', '--dry-run']));
  assert.equal(service.starts_now, false);
  assert.ok(service.definition);
  const domain = JSON.parse(run(['dashboard', 'domain', 'https://selfhost.example.test']));
  assert.equal(domain.callback, 'https://selfhost.example.test/auth/callback');
  const standalone = join(data, 'independent-app');
  const initialized = spawnSync(executable, ['app', '--directory', standalone, 'init', 'gotify'], {
    cwd: data,
    env,
    encoding: 'utf8',
    timeout: 30000,
    windowsHide: true,
  });
  assert.equal(initialized.status, 0, initialized.stderr);
  assert.ok(existsSync(join(standalone, 'compose.yaml')), 'Standalone Compose missing');
  assert.ok(readFileSync(join(standalone, '.env'), 'utf8').length, 'Generated credentials missing');
  const originalCompose = readFileSync(join(standalone, 'compose.yaml'));
  const collision = spawnSync(executable, ['app', '--directory', standalone, 'init', 'gotify'], {
    cwd: data,
    env,
    encoding: 'utf8',
    timeout: 30000,
    windowsHide: true,
  });
  assert.notEqual(collision.status, 0, 'Initializing again must not overwrite an existing app');
  assert.deepEqual(readFileSync(join(standalone, 'compose.yaml')), originalCompose);
  const probe = createServer();
  await new Promise((resolve) => probe.listen(0, '127.0.0.1', resolve));
  const port = probe.address().port;
  await new Promise((resolve) => probe.close(resolve));
  const origin = `http://127.0.0.1:${port}`;
  child = spawn(executable, ['--data-dir', data, 'serve', '--port', String(port)], {
    cwd: data,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  let output = '';
  child.stdout.on('data', (chunk) => {
    output += chunk;
  });
  child.stderr.resume();
  for (let i = 0; i < 100 && !output.includes('#token='); i++) {
    assert.equal(child.exitCode, null, 'Dashboard exited before listening');
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const token = output.match(/#token=([a-f0-9]+)/)?.[1];
  assert.ok(token, 'Missing recovery link');
  const page = await fetch(origin);
  assert.equal(page.status, 200);
  const html = await page.text();
  assert.match(html, /selfhost/i);
  const asset = html.match(/src="([^"]+\.js)"/)?.[1];
  assert.ok(asset, 'Dashboard JavaScript missing');
  const js = await fetch(new URL(asset, origin));
  assert.equal(js.status, 200);
  assert.match(js.headers.get('content-type'), /javascript/);
  assert.equal((await fetch(`${origin}/api/updates`)).status, 401);
  const login = await fetch(`${origin}/auth/local`, {
    method: 'POST',
    headers: { Origin: origin, 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  });
  assert.equal(login.status, 200);
  const cookie = login.headers.get('set-cookie').split(';')[0];
  const account = await login.json();
  const headers = { Cookie: cookie, Origin: origin, 'x-selfhost-client': account.client_key };
  assert.equal((await fetch(`${origin}/api/account`, { headers })).status, 200);
  console.log(
    `Release ${version}: standalone app generation, embedded catalog, deployment profiles, dashboard assets and authenticated API passed.`,
  );
} finally {
  if (child && child.exitCode === null) {
    const ended = new Promise((resolve) => child.once('exit', resolve));
    child.kill();
    await ended;
  }
  assert.equal(dirname(data), resolve(tmpdir()));
  rmSync(data, { recursive: true, force: true });
}
