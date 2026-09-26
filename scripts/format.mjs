import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
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
  [
    process.execPath,
    [require.resolve('@taplo/cli/dist/cli.js'), 'format', ...(check ? ['--check'] : [])],
  ],
];
for (const [command, args] of tools) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}
