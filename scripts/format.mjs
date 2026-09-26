import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { parse } from 'smol-toml';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const check = process.argv.includes('--check');
const tools = [
  ['cargo', ['fmt', '--all', ...(check ? ['--check'] : [])]],
  [
    process.execPath,
    [
      require.resolve('prettier/bin/prettier.cjs'),
      check ? '--check' : '--write',
      '.',
      '--ignore-unknown',
    ],
  ],
];
for (const [command, args] of tools) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}

// Taplo's WASM file discovery can silently skip files on Windows. Feed each
// document through stdin on every platform so local and CI checks agree.
const options = parse(readFileSync(resolve(root, 'taplo.toml'), 'utf8')).formatting;
const tomlFiles = ['Cargo.toml', 'taplo.toml'];
function collect(directory) {
  for (const entry of readdirSync(resolve(root, directory), { withFileTypes: true })) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) collect(path);
    else if (entry.isFile() && entry.name.endsWith('.toml')) tomlFiles.push(path);
  }
}
collect('catalog');
let failed = false;
for (const file of tomlFiles) {
  const path = resolve(root, file);
  const input = readFileSync(path, 'utf8');
  const result = spawnSync(
    process.execPath,
    [
      require.resolve('@taplo/cli/dist/cli.js'),
      'format',
      '-',
      '--no-auto-config',
      ...Object.entries(options).flatMap(([key, value]) => ['--option', `${key}=${value}`]),
    ],
    { cwd: root, input, encoding: 'utf8', shell: false },
  );
  if (result.error) throw result.error;
  if (result.status !== 0 || !result.stdout.trim()) {
    console.error(`TOML formatting failed: ${file}\n${result.stderr}`);
    process.exit(result.status || 1);
  }
  if (input !== result.stdout) {
    if (check) {
      console.error(`TOML formatting differs: ${file}`);
      failed = true;
    } else writeFileSync(path, result.stdout);
  }
}
console.log(`Checked ${tomlFiles.length} TOML files with Taplo.`);
if (failed) process.exit(1);
