import assert from 'node:assert/strict';
import test from 'node:test';
import { canonicalMetadata, verifyPayload } from './canonicalize-npm-repository.mjs';

test('metadata correction rejects a different repository', () => {
  const original = JSON.stringify({
    repository: { url: 'https://github.com/example/selfhost.git' },
    version: '0.1.1',
  });
  assert.equal(
    JSON.parse(canonicalMetadata(original, 'Example/selfhost')).repository.url,
    'https://github.com/Example/selfhost.git',
  );
  assert.throws(() => canonicalMetadata(original, 'Example/other'));
});

test('payload verification rejects binary, metadata and entry changes', () => {
  const metadata = Buffer.from(
    JSON.stringify({
      repository: { url: 'https://github.com/example/selfhost.git' },
      version: '0.1.1',
    }),
  );
  const before = new Map([
    ['package/package.json', metadata],
    ['package/native/binary', Buffer.from([1, 2, 3])],
  ]);
  const after = new Map(before);
  after.set(
    'package/package.json',
    Buffer.from(canonicalMetadata(metadata.toString(), 'Example/selfhost')),
  );
  verifyPayload(before, after, 'Example/selfhost');
  for (const [key, value] of [
    ['package/native/binary', Buffer.from([4])],
    ['package/package.json', Buffer.from('{}')],
    ['package/extra', Buffer.from('extra')],
  ]) {
    assert.throws(() =>
      verifyPayload(before, new Map([...after, [key, value]]), 'Example/selfhost'),
    );
  }
  after.delete('package/native/binary');
  assert.throws(() => verifyPayload(before, after, 'Example/selfhost'));
});
