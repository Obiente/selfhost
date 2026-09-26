// Opt-in, disposable local Docker acceptance tests for native productivity profiles.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { randomBytes } from 'node:crypto';

const binary = resolve(process.argv[2] || 'target/debug/selfhost');
const args = process.argv.slice(3);
const renderOnly = args.includes('--render-only');
const apps = args.filter((v) => !v.startsWith('--'));
if (!apps.length) apps.push('miniflux', 'wallabag', 'wiki-js', 'bookstack', 'paperless-ngx', 'n8n');
mkdirSync('.local/productivity-pass', { recursive: true });
const root = mkdtempSync(resolve('.local/productivity-pass/fixture-'));
// Keep the event loop alive while fetch/AbortSignal use unreferenced internal timers.
const keepAlive = setInterval(() => {}, 1000);
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 650000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  if (result.status !== 0) {
    mkdirSync('.local/productivity-pass', { recursive: true });
    writeFileSync(
      '.local/productivity-pass/last-failure.txt',
      result.stderr + '\n' + result.stdout,
    );
  }
  assert.equal(
    result.status,
    0,
    'Command failed; private diagnostic saved under ignored .local/productivity-pass',
  );
  return result.stdout.trim();
};
const context = JSON.parse(run('docker', ['context', 'inspect']))[0];
assert.match(context.Endpoints.docker.Host, /^(npipe|unix):/, 'Select a local Docker engine');
const request = (url, options = {}) =>
  fetch(url, {
    ...options,
    headers: { Connection: 'close', ...options.headers },
    signal: AbortSignal.timeout(15000),
  });
const wait = async (url, json = false) => {
  for (let i = 0; i < 120; i++) {
    try {
      const r = await request(url);
      if (r.status < 500 && (!json || r.headers.get('content-type')?.includes('application/json')))
        return;
    } catch {}
    await new Promise((r) => setTimeout(r, 1500));
  }
  throw new Error('Fixture did not become ready');
};
const pdf = () => {
  const content = 'BT /F1 16 Tf 50 700 Td (Selfhost fixture document) Tj ET';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
  ];
  let text = '%PDF-1.4\n';
  const offsets = [0];
  for (let i = 0; i < objects.length; i++) {
    offsets.push(Buffer.byteLength(text));
    text += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`;
  }
  const xref = Buffer.byteLength(text);
  text += `xref\n0 6\n0000000000 65535 f \n${offsets
    .slice(1)
    .map((v) => String(v).padStart(10, '0') + ' 00000 n \n')
    .join('')}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(text);
};
try {
  for (const caseId of apps) {
    const app = caseId === 'n8n-postgres' ? 'n8n' : caseId;
    const dir = join(root, caseId);
    const cli = (...args) => run(binary, ['app', '--directory', dir, ...args]);
    const compose = (...args) =>
      run('docker', ['compose', '--file', 'compose.yaml', '--env-file', '.env', ...args], {
        cwd: dir,
      });
    const input = (name, v) => {
      const p = join(root, `${app}-${name}.json`);
      writeFileSync(p, JSON.stringify(v));
      return p;
    };
    const server = createServer();
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    await new Promise((r) => server.close(r));
    const initArgs = ['init', app];
    if (caseId === 'n8n-postgres') initArgs.push('--method', 'postgres');
    if (app !== 'n8n' || caseId === 'n8n-postgres')
      initArgs.push('--inputs', input('inputs', { port, hostname: 'localhost' }));
    if (app === 'wallabag') initArgs.push('--ack', 'initial-account');
    const created = JSON.parse(cli(...initArgs));
    const project = created.compose_project;
    assert.match(project, /^selfhost-p-[a-f0-9]+$/);
    let config = JSON.parse(
      compose(
        'config',
        '--no-interpolate',
        '--no-normalize',
        '--no-path-resolution',
        '--format',
        'json',
      ),
    );
    if (caseId === 'n8n') {
      config.services.n8n.ports = [`127.0.0.1:${port}:5678`];
      writeFileSync(join(dir, 'compose.yaml'), JSON.stringify(config, null, 2));
    }
    config = JSON.parse(compose('config', '--format', 'json'));
    for (const [id, s] of Object.entries(config.services)) {
      if (id !== app) assert.ok(!s.ports?.length, 'Dependencies must not publish ports');
      else assert.equal(s.ports[0].host_ip, '127.0.0.1');
    }
    assert.ok(config.services[app].image.includes(':'));
    if (app === 'wiki-js')
      assert.equal(
        readFileSync(join(dir, 'files/wiki-manage.cjs'), 'utf8'),
        readFileSync('catalog/scripts/wiki-manage.cjs', 'utf8'),
        'Keep the native script and stack asset synchronized',
      );
    console.log(
      `${caseId}: portable Compose, private dependencies, profiles and generated files rendered`,
    );
    if (renderOnly) continue;
    let container;
    let seededVolume;
    try {
      // Desktop engines may disallow host file shares. Seed the exact portable files
      // into an isolated volume for execution while the render check retains bind mounts.
      if (['wiki-js', 'bookstack'].includes(app)) {
        const volume = project + '_fixture_files';
        seededVolume = volume;
        run('docker', [
          'volume',
          'create',
          '--label',
          'com.docker.compose.project=' + project,
          volume,
        ]);
        const helper = run('docker', [
          'create',
          '--label',
          'com.docker.compose.project=' + project,
          '--entrypoint',
          '/bin/sh',
          '--mount',
          'type=volume,source=' + volume + ',target=/fixture',
          config.services[app].image,
          '-c',
          'sleep 120',
        ]);
        try {
          for (const mount of config.services[app].volumes.filter((m) => m.type === 'bind'))
            run('docker', [
              'cp',
              mount.source,
              helper + ':/fixture/' + mount.source.split(/[\\/]/).at(-1),
            ]);
        } finally {
          run('docker', ['rm', '-f', helper]);
        }
        config.volumes.fixture_files = { name: volume };
        config.services[app].volumes = config.services[app].volumes.map((m) =>
          m.type === 'bind'
            ? { type: 'volume', source: 'fixture_files', target: '/selfhost', read_only: true }
            : m,
        );
        writeFileSync(join(dir, 'compose.yaml'), JSON.stringify(config, null, 2));
      }
      const base = `http://127.0.0.1:${port}`;
      const password = `Fixture!${randomBytes(18).toString('hex')}`;
      const env = config.services[app].environment;
      const action = (id, values) =>
        JSON.parse(cli('action', app, id, ...(values ? ['--inputs', input(id, values)] : [])));
      cli('start');
      container = compose('ps', '-q', app);
      assert.ok(container);
      const owned = JSON.parse(run('docker', ['inspect', container]))[0];
      assert.equal(owned.Config.Labels['com.docker.compose.project'], project);
      run('docker', [
        'update',
        '--cpus',
        '2',
        '--memory',
        '2048m',
        '--memory-swap',
        '2048m',
        container,
      ]);
      await wait(base + (app === 'n8n' ? '/rest/settings' : '/'), app === 'n8n');
      if (['n8n', 'wiki-js'].includes(app)) {
        const inputs =
          app === 'n8n'
            ? {
                email: 'fixture@example.com',
                'first-name': 'Fixture',
                'last-name': 'Owner',
                password,
              }
            : { email: 'fixture@example.com', password, 'site-url': base };
        const file = input('setup', { mode: 'bootstrap', inputs, apps: [] });
        const plan = JSON.parse(cli('setup-plan', app, file));
        assert.equal(plan.status, 'true');
        assert.equal(
          JSON.parse(cli('setup-apply', app, file, '--revision', plan.revision)).completed,
          true,
        );
        await wait(base);
      }
      if (app === 'miniflux') {
        const login = await request(base + '/v1/me', {
          headers: {
            Authorization: `Basic ${Buffer.from(env.ADMIN_USERNAME + ':' + env.ADMIN_PASSWORD).toString('base64')}`,
          },
        });
        assert.equal(login.status, 200);
        assert.equal((await login.json()).is_admin, true);
        assert.equal(action('check-health').ok, true);
      } else if (app === 'wallabag') {
        assert.equal(action('change-password', { username: 'wallabag', password }).ok, true);
        assert.equal(
          action('create-user', {
            username: 'fixture-reader',
            email: 'reader@example.com',
            password,
          }).ok,
          true,
        );
        assert.equal(action('list-commands').ok, true);
      } else if (app === 'bookstack') {
        const verify = `require '/app/www/vendor/autoload.php'; $app=require '/app/www/bootstrap/app.php'; $app->make(Illuminate\\Contracts\\Console\\Kernel::class)->bootstrap(); $u=BookStack\\Users\\Models\\User::where('email',getenv('SELFHOST_ADMIN_EMAIL'))->first(); if(!$u || !Illuminate\\Support\\Facades\\Hash::check(getenv('SELFHOST_ADMIN_PASSWORD'),$u->password)){exit(1);} if(BookStack\\Users\\Models\\User::where('email','admin@admin.com')->exists()){exit(2);} echo 'initial administrator verified';`;
        assert.match(run('docker', ['exec', container, 'php', '-r', verify]), /verified/);
        assert.equal(action('rebuild-search').ok, true);
      } else if (app === 'paperless-ngx') {
        const backend = run('docker', [
          'exec',
          container,
          'python',
          '/usr/src/paperless/src/manage.py',
          'shell',
          '-c',
          'from django.db import connection; print(connection.vendor)',
        ]);
        assert.match(backend, /postgresql/, 'Paperless must use the bundled PostgreSQL database');
        const login = await request(base + '/api/token/', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            username: env.PAPERLESS_ADMIN_USER,
            password: env.PAPERLESS_ADMIN_PASSWORD,
          }),
        });
        assert.equal(login.status, 200);
        const token = (await login.json()).token;
        assert.ok(token);
        const form = new FormData();
        form.append('document', new Blob([pdf()], { type: 'application/pdf' }), 'fixture.pdf');
        form.append('title', 'Selfhost fixture');
        const upload = await request(base + '/api/documents/post_document/', {
          method: 'POST',
          headers: { Authorization: `Token ${token}` },
          body: form,
        });
        assert.equal(upload.status, 200);
        let count = 0;
        for (let i = 0; i < 120; i++) {
          const docs = await (
            await request(base + '/api/documents/', {
              headers: { Authorization: `Token ${token}` },
            })
          ).json();
          count = docs.count;
          if (count > 0) break;
          await new Promise((r) => setTimeout(r, 2000));
        }
        assert.equal(count, 1, 'Uploaded PDF must be consumed');
        assert.equal(action('sanity-check').ok, true);
        assert.equal(action('export-documents').ok, true);
        assert.equal(action('optimize-index').ok, true);
        run('docker', ['exec', container, 'test', '-f', '/usr/src/paperless/export/manifest.json']);
      } else if (app === 'n8n') {
        const login = await request(base + '/rest/login', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ emailOrLdapLoginId: 'fixture@example.com', password }),
        });
        assert.equal(login.status, 200);
        const cookie = login.headers
          .getSetCookie()
          .map((v) => v.split(';')[0])
          .join('; ');
        const createdWorkflow = await request(base + '/rest/workflows', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Cookie: cookie, Origin: base },
          body: JSON.stringify({
            name: 'Selfhost fixture',
            nodes: [],
            connections: {},
            settings: {},
          }),
        });
        assert.equal(createdWorkflow.status, 200);
        assert.equal(action('list-workflows').ok, true);
        assert.equal(action('export-workflows').ok, true);
        const createdCredential = await request(base + '/rest/credentials', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Cookie: cookie, Origin: base },
          body: JSON.stringify({
            name: 'Fixture header',
            type: 'httpHeaderAuth',
            data: { name: 'X-Fixture', value: password },
          }),
        });
        assert.equal(createdCredential.status, 200);
        assert.equal(action('export-credentials').ok, true);
        const exported = JSON.parse(
          run('docker', ['exec', container, 'cat', '/home/node/.n8n/selfhost-credentials.json']),
        );
        assert.equal(exported.length, 1);
        assert.equal(typeof exported[0].data, 'string');
        assert.ok(
          !JSON.stringify(exported).includes(password),
          'Credential export must stay encrypted',
        );
        if (caseId === 'n8n-postgres')
          assert.equal(
            compose(
              'exec',
              '-T',
              'database',
              'psql',
              '-U',
              'n8n',
              '-d',
              'n8n',
              '-tAc',
              'SELECT COUNT(*) FROM workflow_entity',
            ),
            '1',
          );
      } else if (app === 'wiki-js') {
        const result = action('list-identities', {
          'admin-email': 'fixture@example.com',
          'admin-password': password,
        });
        assert.equal(result.ok, true);
        const auth = JSON.parse(result.steps[0].output);
        assert.ok(auth.providers.some((p) => p.kind === 'local' && p.enabled));
        const connected = action('connect-oidc', {
          'admin-email': 'fixture@example.com',
          'admin-password': password,
          revision: auth.revision,
          provider: 'fixture-oidc',
          label: 'Fixture OIDC',
          issuer: 'https://id.example.com',
          'authorization-url': 'https://id.example.com/authorize',
          'token-url': 'https://id.example.com/token',
          'userinfo-url': 'https://id.example.com/userinfo',
          'client-id': 'fixture-client',
          'client-secret': password,
        });
        assert.equal(connected.ok, true);
        const updated = JSON.parse(
          action('list-identities', {
            'admin-email': 'fixture@example.com',
            'admin-password': password,
          }).steps[0].output,
        );
        assert.ok(updated.providers.some((p) => p.id === 'fixture-oidc' && p.enabled));
      }
      const fields = JSON.parse(readFileSync(`catalog/integrations/${app}.json`)).fields;
      const setting =
        fields.find((f) => f.id === 'public-url') ||
        fields.find((f) => f.id === 'editor-url') ||
        fields.find((f) => f.id === 'database-host');
      const value = setting.id === 'database-host' ? 'database' : base;
      const file = input('settings', { [setting.id]: value });
      const plan = JSON.parse(cli('plan', app, file));
      assert.equal(JSON.parse(cli('apply', app, file, '--revision', plan.revision)).verified, true);
      const volumes = owned.Mounts.filter((m) => m.Type === 'volume')
        .map((m) => m.Name)
        .sort();
      compose('up', '-d', '--force-recreate');
      const restarted = JSON.parse(run('docker', ['inspect', compose('ps', '-q', app)]))[0];
      assert.deepEqual(
        restarted.Mounts.filter((m) => m.Type === 'volume')
          .map((m) => m.Name)
          .sort(),
        volumes,
      );
      assert.ok(restarted.Config.Env.includes(`${setting.path[0]}=${value}`));
      await wait(base);
      if (app === 'wiki-js')
        assert.equal(
          action('list-identities', {
            'admin-email': 'fixture@example.com',
            'admin-password': password,
          }).ok,
          true,
        );
      console.log(
        `${caseId}: initialization/admin, native operations, effective config and persistent recreation passed`,
      );
    } catch (error) {
      if (container) {
        const logs = spawnSync('docker', ['logs', '--tail', '100', container], {
          encoding: 'utf8',
          windowsHide: true,
        });
        writeFileSync(
          '.local/productivity-pass/last-container-log.txt',
          (logs.stdout || '') + '\n' + (logs.stderr || ''),
        );
      }
      throw error;
    } finally {
      const ids = compose('ps', '-aq').split(/\s+/).filter(Boolean);
      for (const id of ids)
        assert.equal(
          JSON.parse(run('docker', ['inspect', id]))[0].Config.Labels['com.docker.compose.project'],
          project,
        );
      compose('down', '--volumes', '--remove-orphans');
      if (seededVolume) {
        const inspection = spawnSync('docker', ['volume', 'inspect', seededVolume], {
          encoding: 'utf8',
          windowsHide: true,
        });
        if (inspection.status === 0) {
          assert.equal(
            JSON.parse(inspection.stdout)[0].Labels['com.docker.compose.project'],
            project,
          );
          run('docker', ['volume', 'rm', seededVolume]);
        }
      }
    }
  }
} finally {
  clearInterval(keepAlive);
  rmSync(root, { recursive: true, force: true });
}
