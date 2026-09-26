import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
const cargo = readFileSync('Cargo.toml', 'utf8');
const version = cargo.match(/^version = "([^"]+)"/m)?.[1];
assert.match(version || '', /^\d+\.\d+\.\d+$/, 'Release needs an exact stable version');
const pkg = JSON.parse(readFileSync('packages/selfhost/package.json', 'utf8'));
assert.equal(pkg.version, version, 'Cargo and npm versions must agree');
assert.equal(pkg.private, true, 'Source-only npm directory must stay private');
assert.ok(existsSync('ui/dist/index.html'), 'Build the dashboard before packaging');
const self = readFileSync('Cargo.lock', 'utf8')
  .split('[[package]]')
  .find((section) => /^name = "selfhost"$/m.test(section));
assert.ok(self?.includes(`version = "${version}"`), 'Cargo.lock must match');
if (process.env.GITHUB_REF?.startsWith('refs/tags/'))
  assert.equal(
    process.env.GITHUB_REF,
    `refs/tags/v${version}`,
    'Tag and package versions must agree',
  );
if (process.env.SELFHOST_PUBLISH === 'true')
  assert.equal(
    process.env.GITHUB_REF,
    `refs/tags/v${version}`,
    'Publication requires the matching release tag',
  );
console.log(`Release metadata valid: ${version}`);
