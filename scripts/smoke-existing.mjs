// Explicit local-only acceptance tests. Creates and removes only disposable fixtures.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, existsSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { randomBytes } from 'node:crypto';

const binary = resolve(process.argv[2] || 'target/debug/selfhost');
const volumeFiles = process.argv.includes('--volume-files');
const selected = process.argv.slice(3).filter((v) => v !== '--volume-files');
const apps = selected.length
  ? selected
  : [
      'ntfy',
      'vaultwarden',
      'code-server',
      'actual-budget',
      'uptime-kuma',
      'homarr',
      'prometheus',
      'node-red',
      'readeck',
      'trilium',
      'memos',
      'beszel',
      'metube',
      'changedetection',
      'silverbullet',
      'heimdall',
      'tautulli',
    ];
// Keep bind-mounted fixture files under the checkout's shared drive on Docker Desktop.
const fixtureBase = resolve('.local');
mkdirSync(fixtureBase, { recursive: true });
const root = mkdtempSync(join(fixtureBase, 'selfhost-existing-'));
function run(command, args, options = {}) {
  const r = spawnSync(command, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 300000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  if (r.status !== 0)
    writeFileSync(join(fixtureBase, 'smoke-existing-error.txt'), r.stderr || 'No stderr');
  assert.equal(r.status, 0, `${command} failed (output withheld to protect fixture credentials)`);
  return r.stdout.trim();
}
assert.match(
  JSON.parse(run('docker', ['context', 'inspect']))[0].Endpoints.docker.Host,
  /^(npipe|unix):/,
  'Requires local Docker',
);
const request = (url, options = {}) =>
  fetch(url, { ...options, signal: AbortSignal.timeout(5000) });
async function ready(url) {
  for (let n = 0; n < 100; n++) {
    try {
      if ((await request(url)).status < 500) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 1200));
  }
  throw Error('Fixture readiness timed out');
}
const setting = {
  ntfy: ['cache-duration', '24h'],
  vaultwarden: ['signups', 'false'],
  'code-server': ['log-level', 'warn'],
  'actual-budget': ['file-limit', '21'],
  'uptime-kuma': ['timezone', 'UTC'],
  homarr: ['log-level', 'warn'],
  prometheus: ['scrape-interval', '20s'],
  'node-red': ['log-level', 'warn'],
  readeck: ['log-level', 'WARN'],
  trilium: ['instance-name', 'Selfhost fixture'],
  memos: ['log-level', 'warn'],
  beszel: ['container-details', 'false'],
  metube: ['concurrency', '2'],
  changedetection: ['workers', '2'],
  silverbullet: ['index-page', 'Fixture'],
  heimdall: ['timezone', 'UTC'],
  tautulli: ['timezone', 'UTC'],
};
try {
  for (const app of apps) {
    const dir = join(root, app);
    const adaptedFiles = volumeFiles && ['prometheus', 'node-red'].includes(app);
    const runtimeFile = join(dir, 'fixture-compose.json');
    const cli = (...args) => run(binary, ['app', '--directory', dir, ...args]);
    const compose = (...args) =>
      run(
        'docker',
        [
          'compose',
          '--file',
          adaptedFiles && existsSync(runtimeFile) ? 'fixture-compose.json' : 'compose.yaml',
          '--env-file',
          '.env',
          ...args,
        ],
        {
          cwd: dir,
        },
      );
    const info = JSON.parse(
      cli(
        'init',
        app,
        ...(['prometheus', 'node-red'].includes(app) ? ['--method', 'configured'] : []),
      ),
    );
    assert.match(info.compose_project, /^selfhost-p-[a-f0-9]+$/);
    const socket = createServer();
    await new Promise((r) => socket.listen(0, '127.0.0.1', r));
    const port = socket.address().port;
    await new Promise((r) => socket.close(r));
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
    const svc = config.services[app];
    const target =
      typeof svc.ports[0] === 'string' ? svc.ports[0].split(':').at(-1) : svc.ports[0].target;
    svc.ports = [`127.0.0.1:${port}:${target}`];
    svc.mem_limit = '768m';
    svc.cpus = 1;
    if (['prometheus', 'node-red'].includes(app))
      svc.volumes = svc.volumes.map((v) =>
        typeof v === 'object' && v.type === 'bind'
          ? `./files/${app === 'prometheus' ? 'prometheus.yml' : 'settings.js'}:${v.target}:ro`
          : v,
      );
    writeFileSync(join(dir, 'compose.yaml'), JSON.stringify(config, null, 2));
    const input = (name, data) => {
      const path = join(root, `${app}-${name}.json`);
      writeFileSync(path, JSON.stringify(data));
      return path;
    };
    const action = (id, values) =>
      cli('action', app, id, ...(values ? ['--inputs', input(id, values)] : []));
    function copyFixtureFiles() {
      const current = JSON.parse(
        run(
          'docker',
          [
            'compose',
            '-f',
            'compose.yaml',
            '--env-file',
            '.env',
            'config',
            '--no-interpolate',
            '--no-normalize',
            '--no-path-resolution',
            '--format',
            'json',
          ],
          { cwd: dir },
        ),
      );
      const target = current.services[app];
      target.volumes = target.volumes.filter(
        (v) => !(typeof v === 'string' ? v.startsWith('./files/') : v.type === 'bind'),
      );
      if (app === 'prometheus') {
        current.volumes['fixture-config'] = {};
        target.volumes.push('fixture-config:/etc/prometheus:ro');
      }
      writeFileSync(runtimeFile, JSON.stringify(current, null, 2));
      compose('create');
      const destination = app === 'prometheus' ? 'fixture-config' : 'data';
      const helper = `selfhost-smoke-files-${randomBytes(8).toString('hex')}`;
      run('docker', [
        'create',
        '--name',
        helper,
        '--network',
        'none',
        '--mount',
        `type=volume,source=${info.compose_project}_${destination},target=/out`,
        'alpine:3.22',
        'true',
      ]);
      try {
        const file = app === 'prometheus' ? 'prometheus.yml' : 'settings.js';
        run('docker', ['cp', join(dir, 'files', file), `${helper}:/out/${file}`]);
      } finally {
        run('docker', ['rm', helper]);
      }
    }
    let base = `http://127.0.0.1:${port}`;
    try {
      compose('config', '--quiet');
      if (adaptedFiles) {
        copyFixtureFiles();
        compose('up', '-d');
      } else cli('start');
      const id = compose('ps', '-q', app);
      const owned = JSON.parse(run('docker', ['inspect', id]))[0];
      assert.equal(owned.Config.Labels['com.docker.compose.project'], info.compose_project);
      await ready(base);
      const volumes = owned.Mounts.filter((m) => m.Type === 'volume')
        .map((m) => m.Name)
        .sort();
      if (app === 'ntfy') {
        assert.equal(
          (await request(base + '/fixturetopic', { method: 'POST', body: 'must be denied' }))
            .status,
          403,
        );
        const password = randomBytes(20).toString('hex');
        action('create-user', { username: 'fixtureadmin', password, role: 'admin' });
        action('create-user', {
          username: 'fixtureadmin',
          password: 'Different!Password12',
          role: 'user',
        });
        assert.match(action('list-users'), /fixtureadmin/);
        const headers = {
          Authorization: 'Basic ' + Buffer.from(`fixtureadmin:${password}`).toString('base64'),
        };
        assert.equal(
          (
            await request(base + '/fixturetopic', {
              method: 'POST',
              headers,
              body: 'fixture notification',
            })
          ).status,
          200,
        );
        action('topic-access', { username: '*', topic: 'publicfixture', permission: 'read-only' });
        action('list-access');
      }
      if (app === 'vaultwarden') {
        action('sqlite-backup');
        run('docker', [
          'exec',
          id,
          'sh',
          '-c',
          'test -n "$(find /data -name \'db_*.sqlite3\' -print -quit)"',
        ]);
      }
      if (app === 'uptime-kuma') action('health');
      if (app === 'code-server') action('list-extensions');
      if (app === 'prometheus') {
        action('check-config');
        action('check-ready');
      }
      if (app === 'node-red') {
        action('check-settings');
        assert.equal((await request(base + '/flows')).status, 401);
        const password = owned.Config.Env.find((x) => x.startsWith('SELFHOST_ADMIN_PASSWORD='))
          .split('=')
          .slice(1)
          .join('=');
        const login = await request(base + '/auth/token', {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: new URLSearchParams({
            client_id: 'node-red-editor',
            grant_type: 'password',
            scope: '*',
            username: 'admin',
            password,
          }),
        });
        assert.equal(login.status, 200);
        assert.ok((await login.json()).access_token);
      }
      const [field, value] = setting[app];
      const changes = input('settings', { [field]: value });
      const plan = JSON.parse(cli('plan', app, changes));
      assert.ok(plan.revision);
      assert.equal(
        JSON.parse(cli('apply', app, changes, '--revision', plan.revision)).verified,
        true,
      );
      if (adaptedFiles) {
        compose('stop');
        copyFixtureFiles();
      }
      compose('up', '-d', '--force-recreate');
      const recreated = JSON.parse(run('docker', ['inspect', compose('ps', '-q', app)]))[0];
      assert.deepEqual(
        recreated.Mounts.filter((m) => m.Type === 'volume')
          .map((m) => m.Name)
          .sort(),
        volumes,
      );
      await ready(base);
      if (app === 'prometheus') {
        action('check-config');
        const response = await (await request(base + '/api/v1/status/config')).json();
        assert.match(response.data.yaml, /scrape_interval: 20s/);
      } else {
        const profile = JSON.parse(
          readFileSync(
            `catalog/integrations/${app === 'node-red' ? 'node-red-configured' : app}.json`,
            'utf8',
          ),
        );
        assert.ok(
          recreated.Config.Env.includes(
            `${profile.fields.find((f) => f.id === field).path[0]}=${value}`,
          ),
        );
      }
      console.log(
        `${app}: native settings, actions and persistent recreation passed${adaptedFiles ? ' (fixture volume replaces host bind mount)' : ''}`,
      );
    } finally {
      for (const id of compose('ps', '-aq').split(/\s+/).filter(Boolean))
        assert.equal(
          JSON.parse(run('docker', ['inspect', id]))[0].Config.Labels['com.docker.compose.project'],
          info.compose_project,
        );
      compose('down', '--volumes', '--remove-orphans');
    }
  }
} finally {
  assert.ok(root.startsWith(join(fixtureBase, 'selfhost-existing-')));
  rmSync(root, { recursive: true, force: true });
}
