import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { appendFileSync, readdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

export function validateRun(run, jobs, tagCommit, repository) {
  assert.equal(run.repository.full_name, repository);
  assert.equal(run.path, '.github/workflows/release.yml');
  assert.equal(run.event, 'workflow_dispatch');
  assert.equal(run.status, 'completed');
  assert.match(run.head_branch, /^v\d+\.\d+\.\d+$/);
  assert.equal(run.head_sha, tagCommit, 'Release tag no longer matches the tested commit');
  for (const name of ['dashboard', 'source', 'npm-package']) {
    const matches = jobs.filter((job) => job.name === name);
    assert.equal(matches.length, 1, `Missing or ambiguous gate: ${name}`);
    assert.equal(matches[0].conclusion, 'success', `Gate did not pass: ${name}`);
  }
  for (const os of [
    'ubuntu-22.04',
    'ubuntu-24.04-arm',
    'macos-15-intel',
    'macos-14',
    'windows-2022',
    'windows-11-arm',
  ]) {
    for (const prefix of [`native (${os}, `, `install-test (${os})`]) {
      const matches = jobs.filter((job) => job.name.startsWith(prefix));
      assert.equal(matches.length, 1, `Missing or ambiguous platform gate: ${prefix}`);
      assert.equal(matches[0].conclusion, 'success', `Platform gate did not pass: ${prefix}`);
    }
  }
  return run.head_branch.slice(1);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  if (process.argv.includes('--package')) {
    const files = readdirSync('.').filter((file) => file.endsWith('.tgz'));
    assert.equal(files.length, 1, 'Expected one verified tarball');
    const pkg = JSON.parse(
      execFileSync('tar', ['-xOf', files[0], 'package/package.json'], { encoding: 'utf8' }),
    );
    assert.equal(pkg.name, 'selfhost');
    assert.equal(pkg.version, process.env.SELFHOST_VERIFIED_VERSION);
    assert.equal(
      pkg.repository.url.toLowerCase(),
      `https://github.com/${process.env.GITHUB_REPOSITORY.toLowerCase()}.git`,
    );
  } else {
    const id = process.env.SELFHOST_VERIFIED_RUN;
    const repository = process.env.GITHUB_REPOSITORY;
    assert.match(id || '', /^\d+$/);
    assert.match(repository || '', /^[\w.-]+\/[\w.-]+$/);
    const api = (path) =>
      JSON.parse(execFileSync('gh', ['api', `repos/${repository}/${path}`], { encoding: 'utf8' }));
    const run = api(`actions/runs/${id}`);
    assert.match(run.head_branch, /^v\d+\.\d+\.\d+$/);
    const tag = api(`commits/${run.head_branch}`);
    const jobs = api(`actions/runs/${id}/jobs?filter=latest&per_page=100`).jobs;
    const version = validateRun(run, jobs, tag.sha, repository);
    appendFileSync(process.env.GITHUB_ENV, `SELFHOST_VERIFIED_VERSION=${version}\n`);
    console.log(`Verified release v${version} from run ${id}; all platform gates passed.`);
  }
}
