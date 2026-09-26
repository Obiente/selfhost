import assert from 'node:assert/strict';
import test from 'node:test';
import { validateRun } from './verify-npm-recovery.mjs';

test('recovery requires the original tag and every successful platform gate', () => {
  const run = {
    repository: { full_name: 'example/selfhost' },
    path: '.github/workflows/release.yml',
    event: 'workflow_dispatch',
    status: 'completed',
    head_branch: 'v0.1.1',
    head_sha: 'tested-commit',
  };
  const names = ['dashboard', 'source', 'npm-package'];
  for (const os of [
    'ubuntu-22.04',
    'ubuntu-24.04-arm',
    'macos-15-intel',
    'macos-14',
    'windows-2022',
    'windows-11-arm',
  ]) {
    names.push(`native (${os}, target, host, binary)`, `install-test (${os})`);
  }
  const jobs = names.map((name) => ({ name, conclusion: 'success' }));
  assert.equal(validateRun(run, jobs, 'tested-commit', 'example/selfhost'), '0.1.1');
  assert.throws(() => validateRun(run, jobs, 'changed-tag', 'example/selfhost'));
  assert.throws(() => validateRun(run, jobs, 'tested-commit', 'other/selfhost'));
  assert.throws(() =>
    validateRun({ ...run, head_branch: 'main' }, jobs, 'tested-commit', 'example/selfhost'),
  );
  for (let index = 0; index < jobs.length; index++) {
    const failed = jobs.map((job, at) => (at === index ? { ...job, conclusion: 'failure' } : job));
    assert.throws(() => validateRun(run, failed, 'tested-commit', 'example/selfhost'));
    assert.throws(() =>
      validateRun(
        run,
        jobs.filter((_, at) => at !== index),
        'tested-commit',
        'example/selfhost',
      ),
    );
  }
});
