#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { constants, accessSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { delimiter, dirname, isAbsolute, join, resolve, toNamespacedPath } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const executable = process.platform === 'win32' ? 'selfhost.exe' : 'selfhost';
const host = `${process.platform}-${process.arch}`;

function nativeFile(path) {
  const file = realpathSync(path);
  if (!statSync(file).isFile()) throw new Error('The selected Selfhost executable is not a file.');
  const bytes = readFileSync(file);
  const magic = bytes.subarray(0, 4).toString('hex');
  let native =
    process.platform === 'win32'
      ? magic.startsWith('4d5a')
      : process.platform === 'darwin'
        ? ['feedface', 'feedfacf', 'cefaedfe', 'cffaedfe', 'cafebabe', 'bebafeca'].includes(magic)
        : magic === '7f454c46';
  if (native && process.platform === 'win32') {
    const pe = bytes.length >= 64 ? bytes.readUInt32LE(60) : bytes.length;
    native = pe + 6 <= bytes.length && bytes.subarray(pe, pe + 4).toString('hex') === '50450000';
  }
  if (!native)
    throw new Error('Selfhost requires a native Rust executable, not an npm shim or shell script.');
  accessSync(file, process.platform === 'win32' ? constants.R_OK : constants.X_OK);
  return { file, bytes };
}

function findBinary() {
  if (process.env.SELFHOST_BINARY) {
    return nativeFile(resolve(process.cwd(), process.env.SELFHOST_BINARY)).file;
  }
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(root, 'native', 'manifest.json'), 'utf8'));
  } catch (error) {
    if (error.code !== 'ENOENT')
      throw new Error('The bundled Selfhost binary manifest is invalid.');
  }
  if (manifest !== undefined) {
    if (
      !manifest ||
      typeof manifest !== 'object' ||
      Array.isArray(manifest) ||
      manifest.schema !== 1 ||
      !manifest.binaries ||
      typeof manifest.binaries !== 'object' ||
      Array.isArray(manifest.binaries)
    )
      throw new Error('The bundled Selfhost manifest format is unsupported.');
    const binary = manifest.binaries[host];
    if (Object.hasOwn(manifest.binaries, host)) {
      const expectedName = `selfhost-${host}${process.platform === 'win32' ? '.exe' : ''}`;
      if (
        !binary ||
        typeof binary !== 'object' ||
        binary.file !== expectedName ||
        typeof binary.sha256 !== 'string' ||
        !/^[a-f0-9]{64}$/.test(binary.sha256)
      )
        throw new Error('The bundled Selfhost binary entry is invalid.');
      const { file, bytes } = nativeFile(join(root, 'native', binary.file));
      if (createHash('sha256').update(bytes).digest('hex') !== binary.sha256)
        throw new Error(
          'The bundled Selfhost binary failed checksum verification. Reinstall a trusted package.',
        );
      return file;
    }
  }
  // npm injects its shim directories into PATH. Only exact native executable
  // names are eligible, so this launcher cannot recursively invoke itself.
  for (const directory of (process.env.PATH || '').split(delimiter)) {
    if (!directory || !isAbsolute(directory)) continue;
    try {
      return nativeFile(join(directory, executable)).file;
    } catch {
      /* Missing files and script shims are not executable candidates. */
    }
  }
  throw new Error(
    `No trusted native Selfhost executable was found for ${host}. Build or install the Rust CLI, or set SELFHOST_BINARY to its executable path. This launcher never downloads an executable.`,
  );
}

try {
  // pnpm's nested Windows cache can exceed MAX_PATH even when file reads succeed.
  const child = spawn(toNamespacedPath(findBinary()), process.argv.slice(2), {
    cwd: process.cwd(),
    env: process.env,
    stdio: 'inherit',
    shell: false,
  });
  const signals = ['SIGINT', 'SIGTERM'];
  const handlers = new Map(
    signals.map((signal) => [
      signal,
      () => {
        if (!child.killed) child.kill(signal);
      },
    ]),
  );
  for (const [signal, handler] of handlers) process.on(signal, handler);
  child.once('error', (error) => {
    console.error(`selfhost: ${error.message}`);
    process.exitCode = 1;
  });
  child.once('exit', (code, signal) => {
    for (const [name, handler] of handlers) process.removeListener(name, handler);
    process.exitCode = code ?? (signal === 'SIGINT' ? 130 : signal === 'SIGTERM' ? 143 : 1);
  });
} catch (error) {
  console.error(`selfhost: ${error.message}`);
  process.exitCode = 1;
}
