// Assemble only verified, matching artifacts from the six native build jobs.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
export const targets = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'win32-arm64',
  'win32-x64',
];
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function assemble(input, output) {
  input = realpathSync(input);
  output = resolve(output);
  assert.ok(!existsSync(output), 'Output must be a new directory');
  let ancestor = dirname(output);
  while (!existsSync(ancestor)) ancestor = dirname(ancestor);
  const physical = resolve(realpathSync(ancestor), relative(ancestor, output));
  const inside = relative(realpathSync(root), physical);
  assert.ok(
    inside &&
      (isAbsolute(inside) ||
        inside === '..' ||
        inside.startsWith('../') ||
        inside.startsWith('..\\') ||
        inside.replaceAll('\\', '/').startsWith('.local/')),
    'Stage outside the repository or under ignored .local',
  );
  const metadata = JSON.parse(readFileSync(join(root, 'packages/selfhost/package.json'), 'utf8'));
  const verified = [];
  for (const target of targets) {
    const [os, arch] = target.split('-');
    const stage = join(input, target);
    const pkg = JSON.parse(readFileSync(join(stage, 'package.json'), 'utf8'));
    assert.equal(pkg.name, 'selfhost');
    assert.equal(pkg.version, metadata.version);
    assert.deepEqual(pkg.os, [os]);
    assert.deepEqual(pkg.cpu, [arch]);
    const manifest = JSON.parse(readFileSync(join(stage, 'native/manifest.json'), 'utf8'));
    assert.equal(manifest.schema, 1);
    assert.deepEqual(Object.keys(manifest.binaries), [target]);
    const entry = manifest.binaries[target];
    const file = `selfhost-${target}${os === 'win32' ? '.exe' : ''}`;
    assert.equal(entry.file, file);
    const source = realpathSync(join(stage, 'native', file));
    assert.equal(
      dirname(source),
      realpathSync(join(stage, 'native')),
      'Native file must remain inside its stage',
    );
    const digest = createHash('sha256').update(readFileSync(source)).digest('hex');
    assert.equal(entry.sha256, digest, `Checksum mismatch: ${target}`);
    verified.push({ target, file, source, sha256: digest });
  }
  mkdirSync(join(output, 'native'), { recursive: true });
  cpSync(join(root, 'packages/selfhost/bin'), join(output, 'bin'), { recursive: true });
  copyFileSync(join(root, 'LICENSE'), join(output, 'LICENSE'));
  copyFileSync(join(root, 'packages/selfhost/README.md'), join(output, 'README.md'));
  delete metadata.private;
  delete metadata.scripts;
  metadata.os = ['darwin', 'linux', 'win32'];
  metadata.cpu = ['arm64', 'x64'];
  writeFileSync(join(output, 'package.json'), JSON.stringify(metadata, null, 2) + '\n');
  const binaries = {};
  for (const entry of verified) {
    copyFileSync(entry.source, join(output, 'native', entry.file));
    chmodSync(join(output, 'native', entry.file), 0o755);
    binaries[entry.target] = { file: entry.file, sha256: entry.sha256 };
  }
  chmodSync(join(output, 'bin/selfhost.mjs'), 0o755);
  writeFileSync(
    join(output, 'native/manifest.json'),
    JSON.stringify({ schema: 1, binaries }, null, 2) + '\n',
  );
  console.log(
    `Assembled selfhost ${metadata.version} for ${targets.length} platforms. Nothing published.`,
  );
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  assert.equal(
    process.argv.length,
    4,
    'Usage: node scripts/assemble-npm.mjs INPUT_DIRECTORY NEW_OUTPUT_DIRECTORY',
  );
  assemble(process.argv[2], process.argv[3]);
}
