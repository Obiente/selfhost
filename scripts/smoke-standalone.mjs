// Opt-in Docker test. Creates only a uniquely named app, volume and network on a local engine.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  renameSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { createServer } from 'node:net';
const binary = resolve(process.argv[2]);
const base = mkdtempSync(join(tmpdir(), 'selfhost-standalone-smoke-'));
const recipes = join(base, 'recipes');
let app = join(base, 'app');
let project;
let cleaned = false;
const exec = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 650000,
    maxBuffer: 1024 * 1024,
    ...options,
  });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  return result.stdout.trim();
};
const cli = (...args) => exec(binary, ['app', '--directory', app, ...args]);
const compose = (...args) =>
  exec('docker', ['compose', '--file', 'compose.yaml', '--env-file', '.env', ...args], {
    cwd: app,
  });
const inspect = (container) => JSON.parse(exec('docker', ['inspect', container]))[0];
const containerId = () => {
  const id = compose('ps', '-q', 'gotify');
  assert.match(id, /^[a-f0-9]+$/);
  return id;
};
const volumeName = (container) => {
  const metadata = inspect(container);
  assert.equal(metadata.Config.Labels['com.docker.compose.project'], project);
  return metadata.Mounts.find((mount) => mount.Destination === '/app/data').Name;
};
try {
  const context = JSON.parse(exec('docker', ['context', 'inspect']))[0];
  assert.match(
    context.Endpoints.docker.Host,
    /^(npipe|unix):/,
    'Use a local Docker engine for this test',
  );
  mkdirSync(recipes);
  writeFileSync(
    join(recipes, 'gotify.toml'),
    readFileSync('catalog/gotify.toml', 'utf8') +
      `
[[actions]]
id = "read-fixture"
label = "Read fixture"
description = "Read this disposable test's storage marker."
command = ["cat", "/app/data/standalone-marker"]
timeout_seconds = 15
`,
  );
  const initialized = JSON.parse(
    exec(binary, ['--catalog-dir', recipes, 'app', '--directory', app, 'init', 'gotify']),
  );
  assert.equal(initialized.dashboard_required, false);
  assert.equal(initialized.started, false);
  project = initialized.compose_project;
  assert.match(project, /^selfhost-p-[a-f0-9]+$/);
  assert.equal(existsSync(join(app, '.selfhost', 'login.json')), false);
  const probe = createServer();
  await new Promise((done) => probe.listen(0, '127.0.0.1', done));
  const port = probe.address().port;
  await new Promise((done) => probe.close(done));
  const config = JSON.parse(
    compose(
      'config',
      '--no-interpolate',
      '--no-normalize',
      '--no-path-resolution',
      '--format',
      'json',
    ),
  );
  assert.equal(config.name, project);
  config.services.gotify.ports = [`127.0.0.1:${port}:80`];
  config.services.gotify.environment.SMOKE_LITERAL = '${SMOKE_LITERAL}';
  config['x-user-settings'] = { note: 'Preserve unknown configuration' };
  writeFileSync(join(app, 'compose.yaml'), JSON.stringify(config, null, 2));
  const env =
    readFileSync(join(app, '.env'), 'utf8') +
    "\n# Preserve this exact user comment\nSMOKE_LITERAL='value with $dollar and #hash'\n";
  writeFileSync(join(app, '.env'), env);
  const composeBefore = readFileSync(join(app, 'compose.yaml'));
  cli('start');
  let container = containerId();
  const volume = volumeName(container);
  exec('docker', ['exec', '-i', container, 'tee', '/app/data/standalone-marker'], {
    input: 'standalone-storage-ok',
  });
  assert.equal(
    exec('docker', ['exec', container, 'printenv', 'SMOKE_LITERAL']),
    'value with $dollar and #hash',
  );
  assert.match(cli('action', 'gotify', 'read-fixture'), /standalone-storage-ok/);
  const password = inspect(container)
    .Config.Env.find((entry) => entry.startsWith('GOTIFY_DEFAULTUSER_PASS='))
    .split('=')[1];
  const verifyLogin = async () => {
    let response;
    for (let attempt = 0; attempt < 80; attempt++) {
      try {
        response = await fetch(`http://127.0.0.1:${port}/current/user`, {
          headers: {
            Authorization: `Basic ${Buffer.from(`admin:${password}`).toString('base64')}`,
          },
        });
        if (response.status === 200) break;
      } catch {
        /* Initial app startup. */
      }
      await new Promise((done) => setTimeout(done, 250));
    }
    assert.equal(response?.status, 200, 'Generated credentials must still authenticate');
  };
  await verifyLogin();
  const renamed = join(base, 'renamed-app');
  renameSync(app, renamed);
  app = renamed;
  cli('update');
  assert.deepEqual(readFileSync(join(app, 'compose.yaml')), composeBefore);
  assert.equal(readFileSync(join(app, '.env'), 'utf8'), env);
  container = containerId();
  assert.equal(volumeName(container), volume);
  assert.equal(
    exec('docker', ['exec', container, 'cat', '/app/data/standalone-marker']),
    'standalone-storage-ok',
  );
  await verifyLogin();
  renameSync(join(app, '.selfhost'), join(base, 'optional-helper-records'));
  compose('up', '-d', '--force-recreate', '--wait', '--wait-timeout', '90');
  container = containerId();
  assert.equal(volumeName(container), volume);
  assert.equal(
    exec('docker', ['exec', container, 'cat', '/app/data/standalone-marker']),
    'standalone-storage-ok',
  );
  await verifyLogin();
  console.log(
    'Standalone app passed initialization, CLI start/action/update, preserved user files and credentials, directory rename, and ordinary Compose recreation without Selfhost metadata.',
  );
} finally {
  if (project && existsSync(join(app, 'compose.yaml'))) {
    try {
      assert.equal(JSON.parse(compose('config', '--format', 'json')).name, project);
      compose('down', '--volumes');
      assert.equal(
        exec('docker', ['ps', '-aq', '--filter', `label=com.docker.compose.project=${project}`]),
        '',
      );
      assert.equal(
        exec('docker', [
          'volume',
          'ls',
          '-q',
          '--filter',
          `label=com.docker.compose.project=${project}`,
        ]),
        '',
      );
      cleaned = true;
    } catch {
      console.error('Disposable test resources need inspection; fixture retained at', base);
    }
  } else {
    cleaned = true;
  }
  if (cleaned) {
    assert.equal(dirname(base), resolve(tmpdir()));
    rmSync(base, { recursive: true, force: true });
  }
}
