// Opt-in, local Docker acceptance. Only this script's random Compose resources are removed.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { randomBytes } from 'node:crypto';
import { parse } from 'smol-toml';
const binary = resolve(process.argv[2] || 'target/debug/selfhost');
const selection = process.argv.slice(3);
const apps = selection.length
  ? selection
  : ['navidrome', 'audiobookshelf', 'kavita', 'gatus', 'dozzle', 'glances', 'homepage', 'dashy'];
const root = mkdtempSync(
  join(process.env.SELFHOST_SMOKE_ROOT || tmpdir(), 'selfhost-media-smoke-'),
);
// AbortSignal's timers are unreferenced in Node. Keep the acceptance process alive until cleanup.
const keepAlive = setInterval(() => {}, 1000);
const run = (command, args, options = {}) => {
  const p = spawnSync(command, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 600000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  assert.equal(
    p.status,
    0,
    `${command} ${args[0]} failed: ${p.stderr?.slice(-1500) || p.error?.message || 'command failure'}`,
  );
  return p.stdout.trim();
};
assert.match(
  JSON.parse(run('docker', ['context', 'inspect']))[0].Endpoints.docker.Host,
  /^(unix|npipe):/,
  'A local Docker engine is required',
);
async function freePort() {
  const socket = createServer();
  await new Promise((r) => socket.listen(0, '127.0.0.1', r));
  const port = socket.address().port;
  await new Promise((r) => socket.close(r));
  return port;
}
async function ready(url) {
  for (let i = 0; i < 120; i++) {
    try {
      const res = await fetch(url, { signal: AbortSignal.timeout(4000) });
      await res.body?.cancel();
      if (res.status === 200) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error('Fixture did not become HTTP ready');
}
const changes = {
  navidrome: { uiwelcomemessage: 'Selfhost fixture' },
  audiobookshelf: { tz: 'UTC' },
  kavita: { tz: 'UTC' },
  gatus: { 'ui-title': 'Selfhost fixture' },
  dozzle: { 'dozzle-hostname': 'Selfhost fixture' },
  glances: { tz: 'UTC' },
  homepage: {
    services: [
      { Fixture: [{ Example: { href: 'https://example.test', description: 'Fixture service' } }] },
    ],
  },
  dashy: { 'pageinfo-title': 'Selfhost fixture' },
};
try {
  for (const app of apps) {
    const recipe = parse(readFileSync(`catalog/${app}.toml`, 'utf8'));
    const dir = join(root, app);
    const port = await freePort();
    const cli = (...args) => run(binary, ['app', '--directory', dir, ...args]);
    const compose = (...args) =>
      run('docker', ['compose', '--file', 'compose.yaml', '--env-file', '.env', ...args], {
        cwd: dir,
      });
    const inputs = join(root, `${app}-inputs.json`);
    writeFileSync(inputs, JSON.stringify({ port }));
    const created = JSON.parse(
      cli(
        'init',
        app,
        ...(recipe.deployment_only ? ['--inputs', inputs] : []),
        ...(app === 'dozzle' ? ['--ack', 'docker-socket'] : []),
      ),
    );
    const name = created.compose_project;
    assert.match(name, /^selfhost-p-[a-f0-9]+$/);
    if (!recipe.deployment_only) {
      const file = join(dir, 'compose.yaml');
      const source = readFileSync(file, 'utf8');
      const original = `127.0.0.1:${recipe.port}:${recipe.container_port}`;
      assert.ok(source.includes(original), 'Generated local port binding was not found');
      writeFileSync(file, source.replace(original, `127.0.0.1:${port}:${recipe.container_port}`));
    }
    compose('config', '--quiet');
    const patch = join(root, `${app}-patch.json`);
    writeFileSync(patch, JSON.stringify(changes[app]));
    const plan = JSON.parse(cli('plan', app, patch));
    assert.equal(JSON.parse(cli('apply', app, patch, '--revision', plan.revision)).verified, true);
    const state = JSON.parse(cli('config', app));
    for (const [key, value] of Object.entries(changes[app]))
      assert.deepEqual(state.values[key], value, `${app} typed setting did not roundtrip`);
    try {
      // Explicit test-only accommodation for Hyper-V hosts without shared directories.
      // The unmodified portable binds were validated above. Product configuration is unchanged.
      if (
        process.env.SELFHOST_SMOKE_VOLUME_CONFIGS === '1' &&
        recipe.deployment_only &&
        Object.keys(JSON.parse(readFileSync(`catalog/stacks/${app}.json`, 'utf8')).files || {})
          .length
      ) {
        const runtime = JSON.parse(compose('config', '--no-interpolate', '--format', 'json'));
        const service = runtime.services[app];
        const copies = [];
        service.volumes = service.volumes.filter((mount) => {
          if (mount.type !== 'bind' || !mount.source.replaceAll('\\', '/').includes('/files/'))
            return true;
          copies.push({ source: mount.source, target: mount.target });
          return false;
        });
        for (const copy of copies) {
          const parent = copy.target.slice(0, copy.target.lastIndexOf('/'));
          if (
            !service.volumes.some(
              (mount) => mount.target === parent || parent.startsWith(`${mount.target}/`),
            )
          ) {
            const name = `fixture-config-${service.volumes.length}`;
            runtime.volumes[name] = {};
            service.volumes.push({ type: 'volume', source: name, target: parent });
          }
        }
        writeFileSync(join(dir, 'compose.yaml'), JSON.stringify(runtime, null, 2));
        compose('create', '--pull', 'missing');
        const staged = compose('ps', '-aq', app);
        assert.equal(
          JSON.parse(run('docker', ['inspect', staged]))[0].Config.Labels[
            'com.docker.compose.project'
          ],
          name,
        );
        for (const copy of copies) run('docker', ['cp', copy.source, `${staged}:${copy.target}`]);
      }
      cli('start');
      const base = `http://127.0.0.1:${port}`;
      await ready(base);
      if (['navidrome', 'audiobookshelf', 'kavita'].includes(app)) {
        const setup = join(root, `${app}-onboarding.json`);
        const password = `Fixture!Aa7${randomBytes(16).toString('hex')}`;
        writeFileSync(
          setup,
          JSON.stringify({
            mode: 'bootstrap',
            inputs: {
              username: 'fixtureadmin',
              password,
              ...(app === 'kavita' ? { email: 'fixture@example.test' } : {}),
            },
            apps: [],
          }),
        );
        const reviewed = JSON.parse(cli('setup-plan', app, setup));
        assert.ok(!JSON.stringify(reviewed).includes(password));
        assert.equal(
          JSON.parse(cli('setup-apply', app, setup, '--revision', reviewed.revision)).completed,
          true,
        );
      }
      let id = compose('ps', '-q', app);
      let owned = JSON.parse(run('docker', ['inspect', id]))[0];
      assert.equal(owned.Config.Labels['com.docker.compose.project'], name);
      const volumes = owned.Mounts.filter((m) => m.Type === 'volume')
        .map((m) => m.Name)
        .sort();
      assert.ok(volumes.length);
      const marker = join(root, `${app}-marker`);
      writeFileSync(marker, `persistent-${app}`);
      run('docker', ['cp', marker, `${id}:${recipe.data_path}/selfhost-fixture-marker`]);
      for (const action of recipe.actions || []) if (!action.confirm) cli('action', app, action.id);
      compose('up', '-d', '--force-recreate');
      await ready(base);
      id = compose('ps', '-q', app);
      owned = JSON.parse(run('docker', ['inspect', id]))[0];
      assert.deepEqual(
        owned.Mounts.filter((m) => m.Type === 'volume')
          .map((m) => m.Name)
          .sort(),
        volumes,
      );
      const restored = join(root, `${app}-restored`);
      run('docker', ['cp', `${id}:${recipe.data_path}/selfhost-fixture-marker`, restored]);
      assert.equal(readFileSync(restored, 'utf8'), `persistent-${app}`);
      if (app === 'audiobookshelf')
        assert.equal((await (await fetch(`${base}/status`)).json()).isInit, true);
      if (app === 'kavita')
        assert.equal(await (await fetch(`${base}/api/Admin/exists`)).json(), true);
      if (app === 'navidrome')
        assert.ok(!(await (await fetch(`${base}/app/`)).text()).includes('\\"firstTime\\":true'));
      console.log(
        `${app}: Compose, native configuration roundtrip, HTTP, declared read-only actions and volume persistence passed`,
      );
    } finally {
      for (const id of compose('ps', '-aq').split(/\s+/).filter(Boolean))
        assert.equal(
          JSON.parse(run('docker', ['inspect', id]))[0].Config.Labels['com.docker.compose.project'],
          name,
        );
      compose('down', '--volumes', '--remove-orphans');
    }
  }
} finally {
  clearInterval(keepAlive);
  rmSync(root, { recursive: true, force: true });
}
