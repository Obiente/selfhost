import assert from 'node:assert/strict';
import test from 'node:test';
import { stable, candidatesFor } from './catalog-maintenance.mjs';
const app = { id: 'example', image: 'example/app:v2.4.1' };
const profile = {
  source: {
    repository: 'example/app',
    tag_prefix: 'v',
    image_prefix: 'v',
    stream: '2.4',
    automatic: true,
  },
};
test('patch stream, prerelease and migration gates fail closed', () => {
  const releases = ['v2.4.2', 'v2.5.0', 'v3.0.0', 'v2.4.3-rc1', 'v2.4.0'].map((tag_name) => ({
    tag_name,
  }));
  const result = candidatesFor(app, profile, releases);
  assert.deepEqual(
    result.filter((c) => c.eligible).map((c) => c.tag),
    ['v2.4.2'],
  );
  assert.equal(
    candidatesFor(app, profile, [
      { tag_name: 'v2.4.2', body: 'Manual schema migration required' },
    ])[0].eligible,
    false,
  );
  assert.equal(candidatesFor(app, profile, [{ tag_name: 'v2.4.2', prerelease: true }]).length, 0);
  assert.equal(
    candidatesFor(app, { source: { ...profile.source, automatic: false } }, [
      { tag_name: 'v2.4.2' },
    ])[0].eligible,
    false,
  );
});
test('malformed, rolling and ambiguous tags never produce automatic updates', () => {
  for (const tag of [
    'latest',
    '1.2',
    '01.2.3',
    '1.2.3-rc1',
    '1.2.3+build',
    '999999999999999999.2.3',
  ])
    assert.equal(stable(tag), null);
  assert.deepEqual(
    candidatesFor({ ...app, image: 'example/app:latest' }, profile, [{ tag_name: 'v2.4.2' }]),
    [],
  );
});
