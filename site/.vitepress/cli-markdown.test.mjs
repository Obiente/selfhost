import test from 'node:test';
import assert from 'node:assert/strict';
import { commandVariant } from './cli-markdown.mjs';
test('commands and Windows-generated usage adapt without touching arguments or paths', () => {
  const source =
    'Usage: selfhost.exe identity [OPTIONS]\nselfhost --data-dir ./selfhost-data list\n  $ selfhost serve\nURL: https://selfhost.example.test\n./selfhost.exe --help\nSELFHOST_BINARY=target/debug/selfhost.exe';
  const npx = commandVariant(source, 'npx');
  assert.match(npx, /^Usage: npx selfhost identity/);
  assert.match(npx, /npx selfhost --data-dir \.\/selfhost-data list/);
  assert.match(npx, /  \$ npx selfhost serve/);
  assert.match(npx, /https:\/\/selfhost.example.test/);
  assert.match(npx, /\.\/selfhost.exe --help/);
  assert.match(npx, /SELFHOST_BINARY=target\/debug\/selfhost.exe/);
  assert.equal(
    commandVariant(npx, 'installed'),
    source.replace('Usage: selfhost.exe', 'Usage: selfhost'),
  );
  for (const method of ['pnpx', 'pnpm']) {
    const variant = commandVariant(source, method);
    assert.match(
      variant,
      method === 'pnpx' ? /^Usage: pnpx selfhost/ : /^Usage: pnpm dlx selfhost/,
    );
    assert.equal(
      commandVariant(variant, 'installed'),
      source.replace('Usage: selfhost.exe', 'Usage: selfhost'),
    );
  }
});
