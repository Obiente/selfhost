# Updating Selfhost

Selfhost checks the registry that matches its installation and reports the
installed version, latest stable version and supported update method. Checking
does not install anything. The dashboard Updates button, terminal UI and
`selfhost update` share the same review and approval flow.

## Installation methods

| Installation                                           | Update behavior                                                                                                                                                 |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Official crates.io release installed by Cargo          | Build an exact release in private staging, review activation, then replace the executable after Selfhost exits.                                                 |
| Verified bundled npm launcher                          | Report the published launcher version and exact `npx --package=selfhost@VERSION selfhost` invocation. Existing npm packages and npx caches are not overwritten. |
| Debug build, source checkout or Cargo path/git install | Instructions to update the checkout and rebuild. Selfhost never overwrites these builds.                                                                        |
| Direct release binary                                  | Rerun the release installer with explicit replacement and a retained backup. Version 0.1.1 does not automate this through `selfhost update`.                    |
| Other standalone binary                                | Use its original installation method. No automatic binary download or replacement.                                                                              |

Cargo identification requires a release executable in the recorded installation's
`bin` directory, matching official crates.io installation metadata and the current
version. A binary filename alone does not enable updates. npm identification
checks the bundled manifest, package identity and executable checksum. The legacy
public npm package is not advertised as a new CLI launcher unless its package
metadata identifies the Obiente launcher.

## Direct binary installations

Stop Selfhost before replacing its executable, especially a persistent dashboard
service on Windows. Back up its private data before upgrading. Download the
installer again from the [latest release](https://github.com/Obiente/selfhost/releases/latest),
then explicitly allow replacement:

```sh
sh install.sh --version 0.1.1 --force
```

```powershell
./install.ps1 -Version 0.1.1 -Force
```

Choose the version you want; omit the version option to use the latest stable
release. Use the same custom installation directory if you originally chose one.
The installer verifies the checksum and executable version before replacing the
installed file, and keeps the previous binary beside it with a `.backup.` suffix.
A failed download or verification leaves the current installation intact. Restart
the dashboard afterward using its existing service or command.

To roll back, stop Selfhost and restore the printed backup path to the original
executable path. Binary rollback does not reverse application or configuration
changes. Removing the installed executable and its PATH entry uninstalls the CLI;
it does not remove your projects, containers or private data.

In version 0.1.1, `selfhost update` treats direct binaries as standalone installs
and directs you to the original installation method. Its staged activation flow
below applies to official Cargo installations.

## Review, stage and activate

Run the interactive flow:

```sh
selfhost update
```

Or separate each step:

```sh
selfhost update check
selfhost update plan
selfhost update stage --plan-id PLAN_ID --revision REVISION --confirm 'UPDATE SELFHOST TO VERSION'
selfhost update jobs
selfhost update activate --job JOB_ID --confirm 'ACTIVATE SELFHOST VERSION'
```

Use the exact identifiers, revision and confirmation text printed by your plan.
Plans expire after 15 minutes. Staging rechecks the running executable and its
Cargo record. It installs the approved version with Cargo's `--locked` flag into
a private job directory, verifies the resulting version and records its SHA-256
checksum. Build errors leave the current executable in place. Private `build.log`
and job status record the outcome. A failed stage requires a new reviewed plan.

Staging can take several minutes and runs release build scripts with your account
and trusted Rust toolchain. It continues if a browser disconnects. Do not terminate
the Selfhost process during a build. After an interrupted process, inspect the job
and create a new plan rather than reusing a partially built stage.

Activation has a separate typed confirmation. A copied native helper waits up to
30 minutes for the requesting Selfhost process to exit. The dashboard drains
ongoing requests and scheduled work before shutting down. The helper verifies the
installation and staged executable again, locks Cargo's two installation records,
preserves backups, replaces the executable and updates only Selfhost's records.
Unrelated Cargo installations remain in the metadata. The helper does not update
application containers or their images.

A dashboard activation restarts the dashboard using the same private data
directory, port and explicitly selected catalog directory. CLI and terminal UI activation exit after approval; start
Selfhost again yourself. Other independently running Selfhost processes are not
stopped and can prevent replacement on Windows.

Restart status is reported separately from executable activation. A process that
remains running after a short startup check is marked `started_unverified`, not
healthy. An immediate exit or launch error is recorded with instructions to
inspect the private restart log. If activation fails before replacement, or a
rollback succeeds, the helper can restart the checksum-verified original after
the requesting process has exited.

## Signing in after restart

Browser sessions may need a new sign-in after restarting. Existing identity
providers remain configured. Without an identity provider, the new process prints
a new one-use recovery sign-in link. Dashboard restarts save stdout and stderr in
the private job's `restart.log`, so this link is not lost. The activation result
and `selfhost update jobs` show the log path and recovery instructions. Open that
file locally on the host. Do not publish it or share its contents: it can contain
a sign-in credential. The API reports the path, not the log contents.

## Recovery and limits

Back up Selfhost's private data directory before an update. Executable recovery
does not reverse changes a newer release has already made to that directory or
to application data.

Ordinary replacement errors attempt to restore the original executable and both
Cargo records. The old executable is retained beside the installation, while
before/after metadata and job state are retained in the private update directory.
An interruption during replacement can require an explicit recovery:

```sh
selfhost update jobs
selfhost update recover --job JOB_ID --confirm 'RECOVER SELFHOST PREVIOUS_VERSION'
```

Use the exact recovery confirmation reported by the job. Recovery waits for the
requesting process to exit and refuses to overwrite an independently changed
executable or Cargo metadata. If another Cargo installation changed those files,
retain the backups and reconcile the records manually. If no runnable Selfhost
executable remains, use a known trusted copy to inspect the same private data
directory. Do not delete the sibling executable backup or private update job.

Windows replacement uses the operating system's replacement operation with a
backup; Unix replacement preserves the previous executable with a hard link and
renames the prepared replacement. Metadata writes are separately locked and
synced. Executable and both metadata files are not one filesystem transaction;
the journal and recovery checks cover interrupted transitions. A machine crash
or disk failure can still require manual recovery.

The updater uses fixed HTTPS registry endpoints, disables HTTP redirects for
version checks and accepts no executable path or download URL from API clients.
Cargo handles source downloads and crate verification using its installation
mechanism and your trusted local configuration. SHA-256 verifies that staged
files have not changed; it does not authenticate the publisher. Review the
release and its dependencies before approval. There is no unattended update,
signature-based release binary distribution or automatic npm installation.

Windows tests exercise native replacement, metadata preservation, interrupted
update recovery, parent-process waiting, dashboard restart and a blocked Cargo
metadata failure using disposable installations and synthetic local dashboards.
They do not update a real installation or publish a release. Linux and macOS
native activation still require platform verification before release.

See [Cargo install](https://doc.rust-lang.org/cargo/commands/cargo-install.html)
for exact versions, install roots and lockfile behavior, and
[distribution](distribution.md) for local source and npm packaging.
