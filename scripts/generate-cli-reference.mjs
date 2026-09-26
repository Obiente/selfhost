import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { writeFileSync } from 'node:fs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary =
  process.env.SELFHOST_BINARY ||
  resolve(root, 'target', 'debug', process.platform === 'win32' ? 'selfhost.exe' : 'selfhost');
function help(args) {
  const result = spawnSync(binary, [...args, '--help'], {
    encoding: 'utf8',
    windowsHide: true,
    maxBuffer: 1024 * 1024,
  });
  if (result.error || result.status !== 0)
    throw new Error(`Cannot read command help: ${result.error?.message || result.stderr}`);
  return result.stdout.trim().replace(/\bselfhost\.exe\b/g, 'selfhost');
}
function subcommands(text) {
  const section = text.split('Commands:')[1]?.split(/\n(?:Options|Arguments):/)[0] || '';
  return [...section.matchAll(/^  ([a-z][a-z0-9-]*)\s/gm)]
    .map((match) => match[1])
    .filter((name) => name !== 'help');
}
const top = help([]);
let content =
  '# Command reference\n\nGenerated from the current source executable. Run `selfhost COMMAND --help` to check your installed version.\n\n## selfhost\n\n```text\n' +
  top +
  '\n```\n';
for (const command of subcommands(top)) {
  const text = help([command]);
  content += `\n## ${command}\n\n\`\`\`text\n${text}\n\`\`\`\n`;
  for (const sub of subcommands(text))
    content += `\n### ${command} ${sub}\n\n\`\`\`text\n${help([command, sub])}\n\`\`\`\n`;
}
writeFileSync(resolve(root, 'docs', 'cli-reference.md'), content, 'utf8');
console.log('Updated docs/cli-reference.md');
