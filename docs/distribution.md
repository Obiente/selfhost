# Distribution and releases

Selfhost uses one executable and matching versions on npm and crates.io. This
checkout targets **0.1.1**. See the [release page](https://github.com/Obiente/selfhost/releases/tag/v0.1.1)
for publication status and release notes. User installation commands are in
[getting started](getting-started.md).

## What users receive

| Method               | Contents                                                                | User requirements                                                   |
| -------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------- |
| npm, npx, pnpm, pnpx | JavaScript launcher plus six native executables with a SHA-256 manifest | Node.js 22+; no compiler, Git checkout or install scripts           |
| Cargo                | Rust source, locked dependencies, built dashboard and service catalog   | Rust 1.98+ and platform compiler/linker; no Node.js or Git checkout |

The CLI, TUI, dashboard, recipes and integration profiles ship together. Docker
with Compose is required to run container services. No Docker access is needed to
install or inspect the CLI.

The npm artifact targets Windows x64/ARM64, macOS Intel/Apple Silicon, and Linux
x64/ARM64. Linux uses musl binaries to avoid depending on a particular glibc
version. The workflow executes installation tests on native runners for every
target. macOS binaries are not notarized and Windows binaries are not Authenticode
signed. Checksums verify artifact consistency; they are not code signatures.

One npm tarball carries all six binaries. This uses more download space than
separate platform packages, but requires no install-time downloads, optional
dependency selection, lifecycle scripts or additional npm package ownership.

## Launcher behavior

The launcher forwards arguments, working directory, input, output and exit status
without shell evaluation. Resolution order:

1. A native executable explicitly selected with `SELFHOST_BINARY`.
2. The matching bundled executable, verified against its SHA-256 manifest.
3. A native executable in an absolute `PATH` directory, for local development.

Scripts and npm shims are rejected. A corrupt bundled binary fails closed. The
launcher never downloads an executable. The source-only `packages/selfhost`
directory is marked private to prevent accidentally publishing an empty launcher.
Release assembly removes that flag only after validating every platform artifact.

## GitHub Actions

`.github/workflows/release.yml` is the release entry point in `obiente/selfhost`.
It supports both manual dispatch and `v*` tags. **Both publishing inputs default
to false. A tag push builds artifacts but does not publish packages.**

Every run:

1. Checks matching Cargo/npm versions and the Cargo lockfile.
2. Builds the dashboard, checks Rust formatting/Clippy/tests, and builds the docs.
3. Verifies the Cargo package, installs from that packaged source, and checks its
   embedded assets, catalog and authenticated API from outside the checkout.
4. Builds and tests six native targets on matching runners.
5. Assembles one npm tarball only when all versions and checksums agree.
6. Installs that tarball with scripts disabled, npx, pnpm dlx and pnpx on each
   target, then tests the installed application from an isolated directory.

Build artifacts include `crate`, `npm-release`, `dashboard` and six platform
artifacts. They contain product files only, not private test data. The npm artifact
includes `SHA256SUMS`. No registry credentials are needed for these preparation
steps. Dependencies are locked, actions are pinned by commit, and the Rust
toolchain is pinned in the workflow.

### One-time registry setup

These settings are required before choosing publication, and are not configured
by running a preparation build:

- Create the public `obiente/selfhost` repository and push reviewed source.
- Create GitHub environments **npm-release** and **crates-release**. Add required
  reviewers and restrict deployment refs to release tags before enabling publish.
- For npm's existing `selfhost` package, configure a trusted publisher for
  `obiente/selfhost`, workflow `release.yml`, environment `npm-release`. The npm
  publishing job uses GitHub OIDC and provenance, without an npm access token.
- For crates.io, put a narrowly scoped publishing token for `selfhost` in the
  `crates-release` environment secret named `CARGO_REGISTRY_TOKEN`. Keep it out of
  repository files, logs and chat. Only the final Cargo publishing step receives it.

See [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) for the
registry-side configuration. pnpm documents [pnpx and pnpm dlx as equivalent
launchers](https://pnpm.io/cli/dlx).

### Prepare a release

Update `Cargo.toml`, `Cargo.lock`, `packages/selfhost/package.json`, the dashboard
package version and installation examples together. Choose an unused version on
both registries. Version 0.1.0 already exists on crates.io, so the prepared release
uses 0.1.1.

Review source and staged diffs for private paths, credentials and local captures.
Run **Release packages** from a reviewed branch with both publication inputs off.
Download and inspect the resulting tarball, manifests and crate. Resolve all
platform failures before release. A local Windows pass does not verify the other
five targets.

### Publish later

After release approval, create the matching tag, such as `v0.1.1`. Run **Release
packages** manually **on that tag**, selecting `publish_npm`, `publish_cargo`, or
both. The workflow rejects a publication request from a branch or a mismatched
tag. All build and installation gates run again before the environment approval
and publication jobs.

Registry publication is immutable and the two registries are not transactional.
If only one succeeds, do not republish its version; rerun with only the remaining
registry enabled. Check package metadata and perform fresh registry installations
on supported platforms afterward. Confirm the release notes and installation
instructions match the versions actually available on each registry.

If npm publication fails after every build and installation gate passes, the
`verified_npm_run` input can recover the tested package. Run the workflow from
`main`, enable only `publish_npm`, and enter the original tagged run ID. Recovery
verifies the original workflow, unchanged release tag, source checks, all six
native and installation jobs, archive checksum and package metadata. It corrects
only the casing of `repository.url` to match GitHub's canonical owner name for
provenance verification. Repacking must preserve the entry list and every other
file byte for byte. The corrected archive receives a new checksum, an installation
smoke test and a separate workflow artifact. Recovery does not rebuild binaries,
move the tag or publish Cargo again. Use this only for an
already reviewed release; it cannot bypass failed build or installation tests.

The `binary-release` artifact contains the six native executables, shell and
PowerShell installers, and `BINARY-SHA256SUMS`. It is assembled from the same
checksum-verified binaries bundled in npm. Installation tests cover refusal to
overwrite, corrupt-download rejection, explicit replacement and backup on the
six native runners. Upload these files together as GitHub Release assets to enable
direct installation. Keep binary checksums separate from npm/crate archive checksums.

This workflow does not create a GitHub Release or publish the documentation site.
Those are separate publication actions.

## Local preparation

```sh
npm ci --prefix ui
npm run build --prefix ui
cargo package --allow-dirty --locked
cargo build --release --locked
node scripts/check-release.mjs
node scripts/smoke-release.mjs target/release/selfhost
```

On Windows, use `target/release/selfhost.exe`. The smoke test creates temporary
state, verifies the embedded dashboard and login boundary, then removes only its
own state and process. It does not start application containers.

Stage one platform for local package tests:

```sh
node packages/selfhost/scripts/prepare-platform.mjs --binary target/release/selfhost --output .local/platform
npm pack ./.local/platform --pack-destination .local
```

The staging directory must be new and outside tracked source. This single-platform
artifact is for testing; the production npm workflow assembles all six targets.
`node scripts/assemble-npm.mjs INPUT_DIRECTORY NEW_OUTPUT_DIRECTORY` validates
the six downloaded platform directories before assembling a publishable package.

## Updates

Official Cargo installations support reviewed staging, activation and recovery
through `selfhost update`. npm users receive version-specific npx instructions or
can update their global package with their package manager. Source installations
are not overwritten. See [updates and recovery](updates.md).
