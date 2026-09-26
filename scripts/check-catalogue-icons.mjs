import { existsSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildCatalogue } from './catalogue-data.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const publicRoot = resolve(root, 'ui/public');
const groups = new Map();
for (const app of buildCatalogue(root).apps) {
  if (!groups.has(app.icon)) groups.set(app.icon, []);
  groups.get(app.icon).push(app.id);
}
const queue = [...groups];
const failures = [];
async function check(icon) {
  if (icon.startsWith('/')) {
    const file = resolve(publicRoot, `.${icon}`);
    if (!file.startsWith(publicRoot + sep) || !existsSync(file)) throw Error('Missing local image');
    return;
  }
  const url = new URL(icon);
  if (url.protocol !== 'https:' || url.username || url.password)
    throw Error('Expected a public HTTPS image URL');
  const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
  await response.body?.cancel();
  if (!response.ok) throw Error(`HTTP ${response.status}`);
  if (!response.headers.get('content-type')?.startsWith('image/'))
    throw Error('Response is not an image');
}
await Promise.all(
  Array.from({ length: 6 }, async () => {
    while (queue.length) {
      const [icon, apps] = queue.shift();
      try {
        await check(icon);
      } catch (error) {
        failures.push(`${apps.join(', ')}: ${error.message} (${icon})`);
      }
    }
  }),
);
for (const failure of failures.sort()) console.error(failure);
console.log(`Checked ${groups.size} distinct catalogue icons; ${failures.length} unavailable.`);
if (failures.length) process.exitCode = 1;
