// Correct repository casing without changing the verified release payload.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function canonicalMetadata(original, repository) {
  assert.match(repository || '', /^[\w.-]+\/[\w.-]+$/);
  const pkg = JSON.parse(original);
  const canonical = `https://github.com/${repository}.git`;
  assert.equal(pkg.repository.url.toLowerCase(), canonical.toLowerCase());
  pkg.repository.url = canonical;
  return `${JSON.stringify(pkg, null, 2)}\n`;
}

export function verifyPayload(before, after, repository) {
  assert.deepEqual([...after.keys()].sort(), [...before.keys()].sort(), 'Package entries changed');
  for (const [name, bytes] of before) {
    const expected =
      name === 'package/package.json'
        ? Buffer.from(canonicalMetadata(bytes.toString('utf8'), repository))
        : bytes;
    assert.deepEqual(after.get(name), expected, `Unexpected payload change: ${name}`);
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const source = resolve(process.argv[2]);
  const destination = resolve(process.argv[3]);
  const repository = process.env.GITHUB_REPOSITORY;
  const temp = mkdtempSync(join(tmpdir(), 'selfhost-npm-metadata-'));
  const entries = (archive) => {
    const names = execFileSync('tar', ['-tzf', archive], { encoding: 'utf8' })
      .trim()
      .split(/\r?\n/);
    const files = names.filter((name) => !name.endsWith('/'));
    assert.equal(new Set(files).size, files.length);
    for (const name of names) {
      assert.ok(name.startsWith('package/') && !name.includes('..') && !name.includes('\\'));
    }
    return new Map(
      files.map((name) => [
        name,
        execFileSync('tar', ['-xOf', archive, name], { maxBuffer: 128 * 1024 * 1024 }),
      ]),
    );
  };
  try {
    const before = entries(source);
    execFileSync('tar', ['-xzf', source, '-C', temp]);
    writeFileSync(
      join(temp, 'package/package.json'),
      canonicalMetadata(before.get('package/package.json').toString('utf8'), repository),
    );
    mkdirSync(destination, { recursive: true });
    assert.equal(readdirSync(destination).length, 0, 'Output directory must be empty');
    execFileSync(
      process.platform === 'win32' ? 'npm.cmd' : 'npm',
      ['pack', '--ignore-scripts', '--pack-destination', destination],
      {
        cwd: join(temp, 'package'),
        shell: process.platform === 'win32',
        stdio: 'pipe',
      },
    );
    const files = readdirSync(destination).filter((name) => name.endsWith('.tgz'));
    assert.equal(files.length, 1);
    const archive = join(destination, files[0]);
    verifyPayload(before, entries(archive), repository);
    const hash = createHash('sha256').update(readFileSync(archive)).digest('hex');
    writeFileSync(join(destination, 'SHA256SUMS'), `${hash}  ${files[0]}\n`);
    console.log('Verified repository casing correction; all other package bytes are unchanged.');
  } finally {
    assert.equal(dirname(resolve(temp)), resolve(tmpdir()));
    rmSync(temp, { recursive: true, force: true });
  }
}
