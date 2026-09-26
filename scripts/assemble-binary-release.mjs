import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';

const bundle = resolve(process.argv[2]);
const output = resolve(process.argv[3]);
const manifest = JSON.parse(readFileSync(join(bundle, 'native/manifest.json'), 'utf8'));
const targets = [
  'darwin-arm64',
  'darwin-x64',
  'linux-arm64',
  'linux-x64',
  'win32-arm64',
  'win32-x64',
];
assert.deepEqual(Object.keys(manifest.binaries).sort(), targets);
mkdirSync(output, { recursive: true });
assert.equal(readdirSync(output).length, 0, 'Output must be empty');
const checksums = [];
for (const target of targets) {
  const name = `selfhost-${target}${target.startsWith('win32') ? '.exe' : ''}`;
  const entry = manifest.binaries[target];
  assert.equal(entry.file, name);
  const binary = join(bundle, 'native', name);
  const hash = createHash('sha256').update(readFileSync(binary)).digest('hex');
  assert.equal(hash, entry.sha256);
  copyFileSync(binary, join(output, name));
  chmodSync(join(output, name), 0o755);
  checksums.push(`${hash}  ${name}`);
}
for (const name of ['install.sh', 'install.ps1']) {
  const source = new URL(name, import.meta.url);
  copyFileSync(source, join(output, name));
  const hash = createHash('sha256').update(readFileSync(source)).digest('hex');
  checksums.push(`${hash}  ${name}`);
}
writeFileSync(join(output, 'BINARY-SHA256SUMS'), checksums.join('\n') + '\n');
console.log('Prepared six checksum-verified binaries and installers. Nothing published.');
