// Opt-in local acceptance test for an app consuming a generated MySQL-family binding.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createServer } from 'node:net';
const binary = resolve(process.argv[2] || 'target/debug/selfhost');
const engine = process.argv[3] || 'mysql';
assert.ok(['mysql', 'mariadb'].includes(engine));
mkdirSync('.local/database-consumer', { recursive: true });
const root = mkdtempSync(resolve('.local/database-consumer/fixture-'));
const timer = setInterval(() => {}, 1000);
const run = (cmd, args, options = {}) => {
  const r = spawnSync(cmd, args, {
    encoding: 'utf8',
    windowsHide: true,
    timeout: 360000,
    maxBuffer: 4 * 1024 * 1024,
    ...options,
  });
  if (r.status !== 0)
    writeFileSync(
      '.local/database-consumer/last-failure.txt',
      (r.stdout || '') + '\n' + (r.stderr || ''),
    );
  assert.equal(
    r.status,
    0,
    'Command failed; private diagnostic is in ignored .local/database-consumer',
  );
  return r.stdout.trim();
};
const cli = (...args) => run(binary, ['--data-dir', root, ...args]);
let compose, project;
try {
  const context = JSON.parse(run('docker', ['context', 'inspect']))[0];
  assert.match(context.Endpoints.docker.Host, /^(npipe|unix):/);
  const created = cli('create', 'Database consumer fixture', '--apps', 'gotify');
  const id = created.match(/\((p-[a-f0-9]+)\)/)?.[1];
  assert.ok(id);
  project = 'selfhost-' + id;
  const directory = join(root, 'projects', id);
  compose = (...args) =>
    run('docker', ['compose', '--file', 'compose.json', '--env-file', '.env', ...args], {
      cwd: directory,
    });
  cli('database-attach', id, 'dedicated', '--engine', engine);
  const setup = JSON.parse(cli('setup', id, '--reveal'));
  assert.equal(setup.database.engine, engine);
  assert.ok(setup.environment.DATABASE_GO_MYSQL_DSN);
  const socket = createServer();
  await new Promise((r) => socket.listen(0, '127.0.0.1', r));
  const port = socket.address().port;
  await new Promise((r) => socket.close(r));
  setup.compose.services.gotify.ports = [`127.0.0.1:${port}:80`];
  const file = join(root, 'setup.json');
  writeFileSync(file, JSON.stringify(setup));
  cli('setup', id, '--file', file);
  cli('run', id, 'start');
  const config = JSON.parse(compose('config', '--format', 'json'));
  const db = Object.keys(config.services).find((v) => v !== 'gotify');
  assert.ok(db);
  assert.ok(!config.services[db].ports?.length);
  assert.equal(config.services.gotify.environment.GOTIFY_DATABASE_DIALECT, 'mysql');
  assert.equal(
    config.services.gotify.environment.GOTIFY_DATABASE_CONNECTION,
    setup.environment.DATABASE_GO_MYSQL_DSN,
  );
  const container = compose('ps', '-q', 'gotify');
  assert.ok(container);
  assert.equal(
    JSON.parse(run('docker', ['inspect', container]))[0].Config.Labels[
      'com.docker.compose.project'
    ],
    project,
  );
  const base = `http://127.0.0.1:${port}`;
  const credentials = config.services.gotify.environment;
  const auth = {
    Connection: 'close',
    Authorization:
      'Basic ' +
      Buffer.from(
        credentials.GOTIFY_DEFAULTUSER_NAME + ':' + credentials.GOTIFY_DEFAULTUSER_PASS,
      ).toString('base64'),
  };
  const login = async () =>
    fetch(base + '/current/user', { headers: auth, signal: AbortSignal.timeout(5000) });
  let user;
  for (let i = 0; i < 90; i++) {
    try {
      const r = await login();
      if (r.status === 200) {
        user = await r.json();
        break;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 1000));
  }
  assert.equal(
    user?.admin,
    true,
    'Native selected administrator must authenticate against the generated database',
  );
  const prefix = engine === 'mysql' ? 'MYSQL' : 'MARIADB';
  const client = engine === 'mysql' ? 'mysql' : 'mariadb';
  const sql = `export MYSQL_PWD="$${prefix}_PASSWORD"; exec ${client} --protocol=tcp -h127.0.0.1 -u"$${prefix}_USER" "$${prefix}_DATABASE" --batch --skip-column-names -e 'SELECT COUNT(*) FROM users; SHOW GRANTS FOR CURRENT_USER;'`;
  const proof = compose('exec', '-T', db, 'sh', '-ec', sql);
  assert.match(proof, /^1\s/m);
  assert.ok(
    !proof.includes('GRANT ALL PRIVILEGES ON *.*'),
    'App database user must not be a global administrator',
  );
  compose('up', '-d', '--force-recreate', 'gotify');
  let persisted = false;
  for (let i = 0; i < 60; i++) {
    try {
      const r = await login();
      if (r.status === 200) {
        persisted = (await r.json()).id === user.id;
        if (persisted) break;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 1000));
  }
  assert.equal(persisted, true);
  console.log(
    `Gotify ${engine}: Selfhost binding, native administrator login, actual users table, scoped database grants and persistent recreation passed`,
  );
} finally {
  try {
    if (compose) {
      const ids = compose('ps', '-aq').split(/\s+/).filter(Boolean);
      for (const id of ids)
        assert.equal(
          JSON.parse(run('docker', ['inspect', id]))[0].Config.Labels['com.docker.compose.project'],
          project,
        );
      compose('down', '--volumes', '--remove-orphans');
    }
  } finally {
    clearInterval(timer);
    rmSync(root, { recursive: true, force: true });
  }
}
