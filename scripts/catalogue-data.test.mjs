import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'smol-toml';
import { buildCatalogue } from './catalogue-data.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
test('website includes every app and stack, with unique IDs and defined support tags', () => {
  const data = buildCatalogue(root);
  const expected = readdirSync(join(root, 'catalog'))
    .filter((name) => name.endsWith('.toml'))
    .map((name) => parse(readFileSync(join(root, 'catalog', name), 'utf8')).id);
  expected.push(
    ...readdirSync(join(root, 'catalog/stacks')).map((name) => name.replace(/\.json$/, '')),
  );
  for (const id of expected)
    assert.ok(
      data.apps.some((app) => app.id === id),
      id,
    );
  assert.equal(new Set(data.apps.map((app) => app.id)).size, data.apps.length);
  for (const app of data.apps) {
    assert.ok(app.name && app.description && app.category, app.id);
    for (const tag of app.capabilities) assert.ok(data.capabilities[tag], tag);
  }
  const homarr = data.apps.find((app) => app.id === 'homarr');
  assert.ok(homarr.capabilities.includes('onboarding'));
  assert.ok(homarr.capabilities.includes('triggers'));
  const aio = data.apps.find((app) => app.id === 'nextcloud-aio');
  assert.ok(aio.capabilities.includes('existing'));
  assert.ok(aio.actions.find((action) => action.id === 'update-apps').confirmation);
  assert.deepEqual(data, buildCatalogue(root));
});

test('new recipes appear automatically without claiming unsupported features or exporting secret defaults', () => {
  const fixture = mkdtempSync(join(tmpdir(), 'selfhost-catalogue-'));
  try {
    mkdirSync(join(fixture, 'catalog'));
    writeFileSync(
      join(fixture, 'catalog/example.toml'),
      `schema = 1
id = "example"
name = "Example"
description = "Synthetic recipe"
category = "Testing"
image = "example/app:1.0"
[environment]
PASSWORD = "synthetic-private-sentinel"
`,
    );
    const data = buildCatalogue(fixture);
    assert.equal(data.apps[0].id, 'example');
    assert.deepEqual(data.apps[0].capabilities, ['deploy']);
    assert.deepEqual(data.apps[0].testedVersions, []);
    assert.ok(!JSON.stringify(data).includes('synthetic-private-sentinel'));
    assert.ok(!JSON.stringify(data).includes('environment'));
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});

test('guides follow selected deployment bindings and do not expose secret defaults', () => {
  const data = buildCatalogue(root);
  const nextcloud = data.apps.find((app) => app.id === 'nextcloud');
  assert.equal(nextcloud.guideUrl, '/apps/nextcloud');
  assert.ok(nextcloud.deployments.find((method) => method.id === 'docker').services[0].oidc);
  const aio = nextcloud.deployments.find((method) => method.id === 'aio');
  assert.equal(aio.services.length, 0);
  assert.ok(aio.requirements.length > 0);
  assert.ok(aio.images.some((item) => item.image.includes('all-in-one')));
  for (const app of data.apps) {
    assert.equal(app.guideUrl, `/apps/${app.id}`);
    if (app.deploymentOnly) assert.ok(app.deployments.some((method) => method.default));
    for (const method of app.deployments) {
      for (const service of method.services) {
        for (const field of [
          ...service.settings,
          ...(service.onboarding?.fields || []),
          ...service.actions.flatMap((action) => action.inputs),
        ]) {
          if (field.kind === 'secret') assert.ok(!Object.hasOwn(field, 'default'));
        }
      }
    }
  }
});
