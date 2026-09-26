// Opt-in local Docker acceptance test. No existing project or container is changed.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { randomBytes } from 'node:crypto';

const binary = resolve(process.argv[2] || 'target/debug/selfhost');
const selected = process.argv.slice(3);
const apps = selected.length
  ? selected
  : ['jellyfin', 'searxng', 'linkding', 'freshrss', 'gitea', 'forgejo', 'gotify', 'mealie'];
const root = mkdtempSync(join(tmpdir(), 'selfhost-profiles-'));
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 300000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  assert.equal(
    result.status,
    0,
    `${command} failed (output withheld because actions may contain credentials)`,
  );
  return result.stdout.trim();
};
const context = JSON.parse(run('docker', ['context', 'inspect']))[0];
assert.match(context.Endpoints.docker.Host, /^(npipe|unix):/, 'Use a local Docker engine');
const request = async (url, options = {}) =>
  fetch(url, { ...options, signal: AbortSignal.timeout(5000) });
const wait = async (url) => {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await request(url)).status < 500) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 1500));
  }
  throw new Error('Fixture did not become ready');
};
try {
  for (const app of apps) {
    const dir = join(root, app);
    const cli = (...args) => run(binary, ['app', '--directory', dir, ...args]);
    const compose = (...args) =>
      run('docker', ['compose', '--file', 'compose.yaml', '--env-file', '.env', ...args], {
        cwd: dir,
      });
    const created = JSON.parse(cli('init', app));
    const name = created.compose_project;
    assert.match(name, /^selfhost-p-[a-f0-9]+$/);
    const server = createServer();
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    await new Promise((r) => server.close(r));
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
    const service = config.services[app];
    const targetPort =
      typeof service.ports[0] === 'string'
        ? service.ports[0].split(':').at(-1)
        : service.ports[0].target;
    service.ports = [`127.0.0.1:${port}:${targetPort}`];
    if (['gitea', 'forgejo'].includes(app))
      service.environment[`${app.toUpperCase()}__security__INSTALL_LOCK`] = 'true';
    writeFileSync(join(dir, 'compose.yaml'), JSON.stringify(config, null, 2));
    const password = `Fixture!${randomBytes(18).toString('hex')}`;
    const input = (file, values) => {
      const path = join(root, file);
      writeFileSync(path, JSON.stringify(values));
      return path;
    };
    const action = (id, values) =>
      cli('action', app, id, ...(values ? ['--inputs', input(`${app}-${id}.json`, values)] : []));
    let container;
    let appliedSetting;
    try {
      cli('start');
      container = compose('ps', '-q', app);
      run('docker', [
        'update',
        '--memory',
        '768m',
        '--memory-swap',
        '768m',
        '--cpus',
        '1',
        container,
      ]);
      const owned = JSON.parse(run('docker', ['inspect', container]))[0];
      assert.equal(owned.Config.Labels['com.docker.compose.project'], name);
      const base = `http://127.0.0.1:${port}`;
      await wait(base + (app === 'jellyfin' ? '/System/Info/Public' : '/'));
      if (app === 'jellyfin') {
        const setup = input('jellyfin-setup.json', {
          mode: 'bootstrap',
          inputs: { username: 'fixture-admin', password, server: 'Fixture' },
          apps: [],
        });
        const review = JSON.parse(cli('setup-plan', app, setup));
        assert.equal(review.status, 'false');
        const applied = JSON.parse(cli('setup-apply', app, setup, '--revision', review.revision));
        assert.equal(applied.completed, true);
        const info = await (await request(base + '/System/Info/Public')).json();
        assert.equal(info.StartupWizardCompleted, true);
        const login = await request(base + '/Users/AuthenticateByName', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            Authorization:
              'MediaBrowser Client="Selfhost fixture", Device="test", DeviceId="selfhost-fixture", Version="1"',
          },
          body: JSON.stringify({ Username: 'fixture-admin', Pw: password }),
        });
        assert.equal(login.status, 200);
        assert.equal((await login.json()).User.Policy.IsAdministrator, true);
        assert.match(action('check-health'), /Healthy/);
      } else if (app === 'freshrss') {
        action('initialize', {
          username: 'fixtureadmin',
          password,
          email: 'fixture@example.com',
          url: base,
        });
        assert.match(action('list-users'), /fixtureadmin/);
        action('backup-database');
      } else if (['gitea', 'forgejo'].includes(app)) {
        action('create-admin', {
          username: 'fixtureadmin',
          password,
          email: 'fixture@example.com',
        });
        assert.match(action('list-users'), /fixtureadmin/);
        action('list-auth');
        const login = await request(base + '/api/v1/user', {
          headers: {
            Authorization: `Basic ${Buffer.from(`fixtureadmin:${password}`).toString('base64')}`,
          },
        });
        assert.equal(login.status, 200);
        assert.equal((await login.json()).is_admin, true);
      } else if (app === 'linkding') {
        action('check');
        run('docker', [
          'exec',
          container,
          'python',
          '/etc/linkding/manage.py',
          'shell',
          '-c',
          "import os; from django.contrib.auth import get_user_model; u=get_user_model().objects.get(username=os.environ['LD_SUPERUSER_NAME']); assert u.is_superuser and u.check_password(os.environ['LD_SUPERUSER_PASSWORD'])",
        ]);
      } else if (app === 'gotify') {
        const initialPassword = owned.Config.Env.find((v) =>
          v.startsWith('GOTIFY_DEFAULTUSER_PASS='),
        ).slice('GOTIFY_DEFAULTUSER_PASS='.length);
        const login = await request(base + '/current/user', {
          headers: {
            Authorization: `Basic ${Buffer.from(`admin:${initialPassword}`).toString('base64')}`,
          },
        });
        assert.equal(login.status, 200);
        assert.equal((await login.json()).admin, true);
      }
      if (app !== 'jellyfin') {
        const profile = JSON.parse(readFileSync(`catalog/integrations/${app}.json`));
        const setting = profile.fields.find((f) =>
          ['public-url', 'timezone', 'registration', 'trusted-origins'].includes(f.id),
        );
        const value = setting.choices?.[0] ?? (setting.id === 'timezone' ? 'UTC' : base);
        const settings = input(`${app}-settings.json`, { [setting.id]: value });
        const plan = JSON.parse(cli('plan', app, settings));
        const applied = JSON.parse(cli('apply', app, settings, '--revision', plan.revision));
        assert.equal(applied.verified, true);
        assert.equal(applied.restart_required, true);
        appliedSetting = `${setting.path[0]}=${value}`;
      }
      // Recreate, then verify the owned persistent volume is still attached.
      const volumes = owned.Mounts.filter((m) => m.Type === 'volume')
        .map((m) => m.Name)
        .sort();
      compose('up', '-d', '--force-recreate');
      const recreated = JSON.parse(run('docker', ['inspect', compose('ps', '-q', app)]))[0];
      assert.deepEqual(
        recreated.Mounts.filter((m) => m.Type === 'volume')
          .map((m) => m.Name)
          .sort(),
        volumes,
      );
      if (appliedSetting)
        assert.ok(
          recreated.Config.Env.includes(appliedSetting),
          'Native setting must reach recreated app environment',
        );
      await wait(base + (app === 'jellyfin' ? '/System/Info/Public' : '/'));
      if (app === 'jellyfin')
        assert.equal(
          (await (await request(base + '/System/Info/Public')).json()).StartupWizardCompleted,
          true,
        );
      if (['gitea', 'forgejo', 'freshrss'].includes(app))
        assert.match(action('list-users'), /fixtureadmin/);
      console.log(`${app}: native setup/settings, HTTP readiness and persistent recreation passed`);
    } finally {
      const ids = compose('ps', '-aq').split(/\s+/).filter(Boolean);
      for (const id of ids)
        assert.equal(
          JSON.parse(run('docker', ['inspect', id]))[0].Config.Labels['com.docker.compose.project'],
          name,
        );
      compose('down', '--volumes', '--remove-orphans');
    }
  }
} finally {
  // Only our mkdtemp directory; never a user-supplied path.
  rmSync(root, { recursive: true, force: true });
}
