import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
import { assemble, targets } from './assemble-npm.mjs';

test('release assembly requires every platform, exact versions and untampered binaries', () => {
  const root = mkdtempSync(join(tmpdir(), 'selfhost-release-test-'));
  const metadata = JSON.parse(readFileSync('packages/selfhost/package.json', 'utf8'));
  const input = join(root, 'input');
  mkdirSync(input);
  try {
    assert.throws(() => assemble(input, join(root, 'missing')));
    for (const target of targets) {
      const [os, arch] = target.split('-');
      const path = join(input, target);
      mkdirSync(join(path, 'native'), { recursive: true });
      writeFileSync(
        join(path, 'package.json'),
        JSON.stringify({ name: 'selfhost', version: metadata.version, os: [os], cpu: [arch] }),
      );
      const file = `selfhost-${target}${os === 'win32' ? '.exe' : ''}`;
      const binary = Buffer.from(`synthetic test artifact ${target}`);
      writeFileSync(join(path, 'native', file), binary);
      writeFileSync(
        join(path, 'native/manifest.json'),
        JSON.stringify({
          schema: 1,
          binaries: {
            [target]: { file, sha256: createHash('sha256').update(binary).digest('hex') },
          },
        }),
      );
    }
    const output = join(root, 'complete');
    assemble(input, output);
    const manifest = JSON.parse(readFileSync(join(output, 'native/manifest.json')));
    assert.deepEqual(Object.keys(manifest.binaries), targets);
    const pkg = JSON.parse(readFileSync(join(output, 'package.json')));
    assert.equal(pkg.private, undefined);
    assert.equal(pkg.scripts, undefined);
    writeFileSync(join(input, 'linux-x64/native/selfhost-linux-x64'), 'tampered');
    assert.throws(() => assemble(input, join(root, 'tampered')), /Checksum mismatch/);
    writeFileSync(
      join(input, 'darwin-arm64/package.json'),
      JSON.stringify({ ...metadata, version: '0.0.0', os: ['darwin'], cpu: ['arm64'] }),
    );
    assert.throws(() => assemble(input, join(root, 'wrong-version')));
  } finally {
    assert.equal(dirname(root), resolve(tmpdir()));
    rmSync(root, { recursive: true, force: true });
  }
});
