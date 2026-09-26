import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const launcher = join(root, 'bin', 'selfhost.mjs');
function clean(directory) {
  assert.equal(dirname(directory), resolve(tmpdir()));
  assert.ok(directory.startsWith(join(resolve(tmpdir()), 'selfhost-launcher-')));
  rmSync(directory, { recursive: true, force: true });
}

test('rejects script overrides and does not recurse into npm shims', () => {
  const directory = mkdtempSync(join(tmpdir(), 'selfhost-launcher-'));
  try {
    const shim = join(directory, process.platform === 'win32' ? 'selfhost.exe' : 'selfhost');
    writeFileSync(shim, '#!/bin/sh\necho unsafe\n', { mode: 0o755 });
    const override = spawnSync(process.execPath, [launcher], {
      encoding: 'utf8',
      env: { ...process.env, SELFHOST_BINARY: shim },
    });
    assert.equal(override.status, 1);
    assert.match(override.stderr, /native Rust executable/);
    if (process.platform === 'win32') {
      const fake = Buffer.alloc(80);
      fake.write('MZ');
      fake.writeUInt32LE(64, 60);
      writeFileSync(shim, fake);
      const invalidPe = spawnSync(process.execPath, [launcher], {
        encoding: 'utf8',
        env: { ...process.env, SELFHOST_BINARY: shim },
      });
      assert.equal(invalidPe.status, 1);
      assert.match(invalidPe.stderr, /native Rust executable/);
    }
    const path = spawnSync(process.execPath, [launcher], {
      encoding: 'utf8',
      timeout: 5000,
      env: { ...process.env, SELFHOST_BINARY: '', PATH: directory },
    });
    assert.equal(path.status, 1);
    assert.match(path.stderr, /No trusted native Selfhost executable/);
  } finally {
    clean(directory);
  }
});

test('native override preserves arguments, input, working directory and exit status', () => {
  const directory = mkdtempSync(join(tmpdir(), 'selfhost-launcher-'));
  try {
    // Node is itself native and provides an observable child for launcher tests.
    const source =
      'process.stdout.write(JSON.stringify({cwd:process.cwd(),args:process.argv.slice(1),input:require("node:fs").readFileSync(0,"utf8")}));process.exitCode=7';
    const result = spawnSync(
      process.execPath,
      [launcher, '-e', source, '--', 'spaces remain', '$(literal)', 'semi;colon'],
      {
        cwd: directory,
        encoding: 'utf8',
        input: 'stdin value',
        env: { ...process.env, SELFHOST_BINARY: process.execPath },
      },
    );
    assert.equal(result.status, 7);
    const output = JSON.parse(result.stdout);
    assert.equal(output.cwd, directory);
    assert.deepEqual(output.args, ['spaces remain', '$(literal)', 'semi;colon']);
    assert.equal(output.input, 'stdin value');
  } finally {
    clean(directory);
  }
});

test('bundled binaries require a valid checksum and confined manifest path', () => {
  const directory = mkdtempSync(join(tmpdir(), 'selfhost-launcher-'));
  try {
    mkdirSync(join(directory, 'bin'));
    mkdirSync(join(directory, 'native'));
    copyFileSync(launcher, join(directory, 'bin', 'selfhost.mjs'));
    const host = `${process.platform}-${process.arch}`,
      filename = `selfhost-${host}${process.platform === 'win32' ? '.exe' : ''}`;
    copyFileSync(process.execPath, join(directory, 'native', filename));
    const entry = {
      file: filename,
      sha256: createHash('sha256').update(readFileSync(process.execPath)).digest('hex'),
    };
    const manifest = { schema: 1, binaries: { [host]: entry } };
    const run = () =>
      spawnSync(process.execPath, [join(directory, 'bin', 'selfhost.mjs'), '--version'], {
        encoding: 'utf8',
        env: { ...process.env, SELFHOST_BINARY: '' },
      });
    const save = () =>
      writeFileSync(join(directory, 'native', 'manifest.json'), JSON.stringify(manifest));
    save();
    const valid = run();
    assert.equal(valid.status, 0);
    assert.equal(valid.stdout.trim(), process.version);
    entry.sha256 = '0'.repeat(64);
    save();
    const corrupt = run();
    assert.equal(corrupt.status, 1);
    assert.match(corrupt.stderr, /checksum verification/);
    entry.file = '../outside';
    save();
    const traversal = run();
    assert.equal(traversal.status, 1);
    assert.match(traversal.stderr, /entry is invalid/);
    manifest.binaries[host] = null;
    save();
    const emptyEntry = run();
    assert.equal(emptyEntry.status, 1);
    assert.match(emptyEntry.stderr, /entry is invalid/);
    writeFileSync(join(directory, 'native', 'manifest.json'), 'null');
    const invalid = run();
    assert.equal(invalid.status, 1);
    assert.match(invalid.stderr, /format is unsupported/);
  } finally {
    clean(directory);
  }
});
