// Discovery is read-only. --prepare requires explicit disposable Docker smoke tests.
import { spawnSync } from 'node:child_process';
import {
  readFileSync,
  writeFileSync,
  readdirSync,
  mkdirSync,
  mkdtempSync,
  cpSync,
  rmSync,
} from 'node:fs';
import { resolve, join, dirname, basename } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';
import { parse } from 'smol-toml';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
export function stable(text) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(text)) return null;
  const values = text.split('.').map(Number);
  return values.every(Number.isSafeInteger) ? values : null;
}
const compare = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
export function candidatesFor(app, profile, releases) {
  const source = profile.source;
  if (!source || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(source.repository)) return [];
  const split = app.image.lastIndexOf(':');
  const repository = app.image.slice(0, split),
    tag = app.image.slice(split + 1);
  const current = tag.startsWith(source.image_prefix)
    ? stable(tag.slice(source.image_prefix.length))
    : null;
  if (!current) return [];
  return releases
    .slice(0, 30)
    .flatMap((release) => {
      if (
        release.draft ||
        release.prerelease ||
        typeof release.tag_name !== 'string' ||
        !release.tag_name.startsWith(source.tag_prefix)
      )
        return [];
      const version = stable(release.tag_name.slice(source.tag_prefix.length));
      if (!version || compare(version, current) <= 0) return [];
      const withinStream =
        `${version[0]}.${version[1]}` === source.stream &&
        version[0] === current[0] &&
        version[1] === current[1];
      // Conservative text screening is an additional stop, never evidence of compatibility.
      const migration = /migrat|breaking|schema|manual (step|action)|incompatib|downgrade/i.test(
        String(release.body || ''),
      );
      const image = `${repository}:${source.image_prefix}${version.join('.')}`;
      return [
        {
          app: app.id,
          from: app.image,
          image,
          tag: release.tag_name,
          within_stream: withinStream,
          eligible: Boolean(source.automatic && withinStream && !migration),
          reason: !withinStream
            ? 'Outside the explicitly maintained patch stream'
            : migration
              ? 'Release notes require migration or compatibility review'
              : !source.automatic
                ? 'Automatic candidate preparation disabled for this profile'
                : 'Eligible for manifest, Compose and isolated upgrade smoke gates',
          release_url: `https://github.com/${source.repository}/releases/tag/${release.tag_name}`,
        },
      ];
    })
    .sort((a, b) =>
      compare(
        stable(b.tag.slice(source.tag_prefix.length)),
        stable(a.tag.slice(source.tag_prefix.length)),
      ),
    );
}
function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
    shell: false,
    timeout: 180000,
    maxBuffer: 2 * 1024 * 1024,
    ...options,
  });
  if (result.error || result.status !== 0)
    throw new Error(
      `${command} failed (${result.status ?? 'unavailable'}). Review the isolated job log; no candidate was promoted.`,
    );
  return result.stdout.trim();
}
async function releases(repository) {
  for (let attempt = 0; attempt < 3; attempt++) {
    const result = spawnSync(
      'gh',
      [
        'api',
        `repos/${repository}/releases?per_page=30`,
        '-H',
        'Accept: application/vnd.github+json',
        '-H',
        'X-GitHub-Api-Version: 2022-11-28',
      ],
      {
        cwd: root,
        encoding: 'utf8',
        windowsHide: true,
        timeout: 30000,
        maxBuffer: 2 * 1024 * 1024,
      },
    );
    if (result.status === 0) {
      const value = JSON.parse(result.stdout);
      if (!Array.isArray(value)) throw new Error('Invalid release metadata');
      return value;
    }
    if (!/HTTP 50[234]/.test(result.stderr || '') || attempt === 2)
      throw new Error(
        `Release metadata unavailable for ${repository}; rate limits and ambiguous responses require a later retry.`,
      );
    await sleep(1000 * 2 ** attempt);
  }
}
async function unusedPort() {
  const server = createServer();
  await new Promise((done, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', done);
  });
  const port = server.address().port;
  await new Promise((done) => server.close(done));
  return port;
}
export async function smoke(candidate, app, profile, binary) {
  if (
    !/^[a-z0-9-]+$/.test(app.id) ||
    !/^\/[A-Za-z0-9_./-]+$/.test(app.data_path) ||
    app.data_path.includes('..')
  )
    throw new Error('Unsupported smoke recipe');
  const base = mkdtempSync(join(tmpdir(), 'selfhost-catalog-'));
  const directory = join(base, 'app'),
    catalog = join(base, 'catalog');
  let compose;
  try {
    run('docker', ['buildx', 'imagetools', 'inspect', candidate.image]);
    cpSync(join(root, 'catalog'), catalog, { recursive: true });
    const port = await unusedPort();
    const source = readFileSync(join(catalog, `${app.id}.toml`), 'utf8');
    writeFileSync(
      join(catalog, `${app.id}.toml`),
      source
        .replace(/^image\s*=\s*"[^"\r\n]+"\s*$/m, `image = "${candidate.from}"`)
        .replace(/^port\s*=\s*\d+\s*$/m, `port = ${port}`),
    );
    run(binary, ['--catalog-dir', catalog, 'app', '--directory', directory, 'init', app.id]);
    compose = ['compose', '-f', join(directory, 'compose.yaml'), '--project-directory', directory];
    run('docker', [...compose, 'config', '--quiet']);
    run('docker', [...compose, 'up', '-d'], { timeout: 600000 });
    const currentContainer = run('docker', [...compose, 'ps', '-q', app.id]);
    if (
      run('docker', ['inspect', '--format', '{{.Config.Image}}', currentContainer]) !==
      candidate.from
    )
      throw new Error('Initial fixture did not run the reviewed source image');
    const probe = async () => {
      for (let attempt = 0; attempt < 90; attempt++) {
        try {
          const response = await fetch(`http://127.0.0.1:${port}${profile.source.smoke_path}`, {
            redirect: 'manual',
            signal: AbortSignal.timeout(3000),
          });
          await response.body?.cancel();
          if (response.status === profile.source.smoke_status) return;
        } catch {}
        await sleep(1000);
      }
      throw new Error('Isolated application health probe did not pass');
    };
    await probe();
    const marker = `${app.data_path}/.selfhost-upgrade-smoke`;
    run('docker', [
      ...compose,
      'exec',
      '-T',
      app.id,
      'sh',
      '-c',
      'printf selfhost-data-retained > "$1"',
      'selfhost-smoke',
      marker,
    ]);
    // Use the app's actual generated Compose document, retaining all storage and credentials.
    const plan = JSON.parse(
      run(binary, [
        'app',
        '--directory',
        directory,
        'version-plan',
        app.id,
        candidate.image,
        '--allow-untested',
      ]),
    );
    run(binary, [
      'app',
      '--directory',
      directory,
      'version-apply',
      app.id,
      candidate.image,
      '--allow-untested',
      '--revision',
      plan.revision,
    ]);
    run('docker', [...compose, 'config', '--quiet']);
    run('docker', [...compose, 'up', '-d'], { timeout: 600000 });
    const candidateContainer = run('docker', [...compose, 'ps', '-q', app.id]);
    if (
      run('docker', ['inspect', '--format', '{{.Config.Image}}', candidateContainer]) !==
        candidate.image ||
      (candidate.image !== candidate.from && candidateContainer === currentContainer)
    )
      throw new Error('Fixture did not recreate into the candidate image');
    await probe();
    if (
      run('docker', [...compose, 'exec', '-T', app.id, 'cat', marker]) !== 'selfhost-data-retained'
    )
      throw new Error('Upgrade lost the persistent-data marker');
    for (const action of app.actions || []) {
      if (!action.confirm)
        run(binary, ['app', '--directory', directory, 'action', app.id, action.id], {
          timeout: 330000,
        });
    }
  } finally {
    if (compose)
      run('docker', [...compose, 'down', '--volumes', '--remove-orphans'], { timeout: 120000 });
    if (
      dirname(resolve(base)) !== resolve(tmpdir()) ||
      !basename(base).startsWith('selfhost-catalog-')
    )
      throw new Error('Temporary fixture cleanup path failed verification');
    rmSync(base, { recursive: true, force: true });
  }
}
export async function main(args = process.argv.slice(2)) {
  const prepare = args.includes('--prepare');
  const binaryIndex = args.indexOf('--binary');
  const binary = binaryIndex >= 0 ? resolve(args[binaryIndex + 1]) : null;
  if (prepare && (!args.includes('--allow-docker-smoke') || !binary))
    throw new Error('--prepare needs --binary and explicit --allow-docker-smoke');
  const outputIndex = args.indexOf('--output');
  const output = resolve(
    outputIndex >= 0 ? args[outputIndex + 1] : join(root, '.local', 'catalog-candidates.json'),
  );
  const results = [],
    promoted = [];
  const files = readdirSync(join(root, 'catalog', 'versions'))
    .filter((f) => /^[a-z0-9-]+\.json$/.test(f))
    .sort();
  if (files.length > 100)
    throw new Error('Split release discovery into batches of at most 100 apps');
  for (const file of files) {
    const profile = JSON.parse(readFileSync(join(root, 'catalog', 'versions', file), 'utf8'));
    if (!profile.source) continue;
    const app = parse(
      readFileSync(join(root, 'catalog', file.replace(/\.json$/, '.toml')), 'utf8'),
    );
    try {
      if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(profile.source.repository))
        throw new Error('Invalid upstream release repository');
      const candidates = candidatesFor(app, profile, await releases(profile.source.repository));
      results.push(...candidates);
      const candidate = candidates.find((c) => c.eligible);
      if (prepare && candidate) {
        await smoke(candidate, app, profile, binary);
        const path = join(root, 'catalog', `${app.id}.toml`),
          source = readFileSync(path, 'utf8');
        if (parse(source).image !== candidate.from)
          throw new Error('Recipe changed during candidate verification');
        const replacement = source.replace(
          /^image\s*=\s*"[^"\r\n]+"\s*$/m,
          `image = "${candidate.image}"`,
        );
        if (replacement === source || parse(replacement).image !== candidate.image)
          throw new Error('Could not safely replace the recipe image');
        writeFileSync(path, replacement);
        profile.tested.push({
          image: candidate.image,
          label: candidate.tag,
          notes:
            'CI Linux amd64: image manifest, generated Compose validation, HTTP startup, same-stream upgrade data marker and declared non-confirming actions passed. Native settings, OIDC and other architectures still require review.',
        });
        writeFileSync(
          join(root, 'catalog', 'versions', file),
          JSON.stringify(profile, null, 2) + '\n',
        );
        promoted.push(candidate);
      }
    } catch (error) {
      results.push({ app: app.id, eligible: false, error: error.message });
    }
    await sleep(1000);
  }
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(
    output,
    JSON.stringify({ candidates: results, prepared: promoted }, null, 2) + '\n',
  );
  console.log(
    `Checked ${files.length} version sources; prepared ${promoted.length} candidates. No running user project was selected.`,
  );
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
