# Start using Selfhost

Use Selfhost once to set up an app, return to its CLI when you need help, or keep
your services in a workspace. The dashboard is optional.

## Choose how you want to work

| What you want                                      | Where to start                  |
| -------------------------------------------------- | ------------------------------- |
| Set up an app and keep its ordinary Compose files  | [Standalone CLI](standalone.md) |
| Use the CLI for projects, backups and integrations | [Command-line guide](cli.md)    |
| Use a guided terminal interface                    | [Terminal interface](tui.md)    |
| Manage services in a browser                       | [Dashboard guide](dashboard.md) |

Running containers continue after Selfhost exits. The dashboard and its background
service are needed only for features you choose to keep running, such as Selfhost
schedules. You can also schedule ordinary Compose or CLI commands yourself.

## Install or run directly

::: info Version 0.1.1
These commands target version 0.1.1. Check the [release page](https://github.com/Obiente/selfhost/releases/tag/v0.1.1)
for publication status and release notes. You do not need to clone the repository
when installing a published release.
:::

Choose one installation method. The dashboard and service catalog are included in
the CLI, but you choose which command to run. Docker with Compose is needed to run
containers; generating a standalone setup does not require a running engine.

### Install a binary directly

No Node.js, npm, Cargo or compiler is required. Linux, macOS and Windows are
supported on x64 and ARM64.

Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/Obiente/selfhost/releases/latest/download/install.sh | sh -s -- --version 0.1.1
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod https://github.com/Obiente/selfhost/releases/latest/download/install.ps1))) -Version 0.1.1
```

The installer checks the download's SHA-256 checksum and reported version before
installing. It uses `~/.local/bin` on Linux/macOS or `%LOCALAPPDATA%\Selfhost\bin`
on Windows. It needs no administrator access and starts no services. The shell
installer prints PATH guidance; the Windows installer adds its directory to your
user PATH. Open a new terminal if needed, then run `selfhost --help`.

You can download and inspect the installer before running it, or download the
binary directly from [the release assets](https://github.com/Obiente/selfhost/releases/tag/v0.1.1)
and verify it against `BINARY-SHA256SUMS`. Rename it to `selfhost` on Linux/macOS
or keep the platform's executable extension on Windows. On Linux/macOS, make it
executable with `chmod +x selfhost` and put it on PATH.

For a custom location use `--bin-dir DIRECTORY` or PowerShell `-BinDir DIRECTORY`.
PowerShell `-NoModifyPath` leaves PATH unchanged. See [updates](updates.md#direct-binary-installations)
for replacement and rollback. These downloads have checksums but are not yet
signed or notarized; checksums from the same release do not independently
authenticate its publisher.

### npx

With Node.js 22 or newer:

```sh
npx selfhost@0.1.1 --help
```

### pnpm or pnpx

```sh
pnpm dlx selfhost@0.1.1 --help
```

The equivalent short command is `pnpx selfhost@0.1.1 --help`. No install scripts
or separate Rust installation are required. The npm package includes native
executables for Windows, Linux and macOS, on x64 and ARM64.

### Install with npm or pnpm

```sh
npm install --global selfhost@0.1.1
```

Or use `pnpm add --global selfhost@0.1.1`. Use `selfhost --help` to choose a command.

### Install with Cargo

With Rust 1.98 or newer and your platform's native build tools:

```sh
cargo install selfhost --locked --version 0.1.1
```

Use `selfhost --help` to choose a command. Cargo compiles the CLI locally. Its source package
includes the built dashboard, so you do not need Node.js or a repository checkout.

Select your command method above to adapt the remaining examples. For a specific
release, pin its version as shown here. See [distribution](distribution.md) for
platform requirements, artifact verification and release preparation.

## Open the dashboard

Open the link printed in your terminal. It contains a one-use local sign-in valid
for ten minutes. The dashboard and API use port `8372` by default. It comes from
**SH** in ASCII: **S = 83**, **H = 72**, joined as **8372**.

Choose another port when needed:

```sh
selfhost serve --port 8797
```

Keep the process open while using the dashboard. Scheduled tasks run while it is
running. Stopping Selfhost does not stop your application containers.

## Run persistently or use a domain

Use `selfhost dashboard install` and `selfhost dashboard start` to keep Selfhost
running after the terminal closes. See [dashboard hosting](dashboard-hosting.md)
for startup at sign-in or boot, private logs and using your existing HTTPS proxy.

## Choose where Selfhost stores its configuration

By default, Selfhost uses your operating system's application-data directory.
Choose your own location with the global `--data-dir` option:

```sh
selfhost --data-dir ./selfhost-data serve
```

Use the **same data directory** for dashboard and CLI commands. Running a command
with another directory opens another workspace. Protect this directory: it
contains credentials, configuration, connection records and backups.

## Your first project

1. Open **App catalog** and choose an app.
2. If multiple deployment methods are available, choose the one you want.
3. Give the project a name and choose its server.
4. Review ports, storage, credentials and any special requirements.
5. Open the project and choose **Start project**.

Creating a project prepares its configuration. Starting it runs its containers.
Apps bind to the selected server's loopback address by default. A loopback URL on
a remote server is not accessible from your browser without a tunnel or proxy.

## Already running services?

Open **Existing apps** and choose **Link app**. Start with the app's URL, then add
a Docker connection if you want verified container status or declared actions.
See [existing apps](existing-apps.md). Linking does not move data or transfer
ownership of the deployment to Selfhost.

## Set up sign-in

Open **Access**, choose **Connect an existing identity provider**, and follow the
guided setup. You can use a localhost address first and move to a domain later.
See [identity setup](identity-setup.md).

For remote access, use a dedicated HTTPS origin and trusted proxy connection.
Read the [security guide](security.md) before exposing the dashboard.

## Next steps

- [Learn the dashboard](dashboard.md).
- [Use the CLI](cli.md).
- [Create a custom setup](setups-and-databases.md).
- [Plan backup and removal](removal.md).

## Common first-run problems

**The local link no longer works:** restart `selfhost serve` and use its new link.
Used links cannot be replayed. An existing provider can also sign you in.

**Docker is offline:** run `selfhost doctor`. Check that the Docker engine and
selected context are available to the same operating-system account running
Selfhost. Preparing configuration does not require a running engine.

**The port is already in use:** choose another port with `serve --port`. Update
the identity provider's exact callback URL if you use OIDC at that address.

**The CLI cannot find a project shown in the dashboard:** verify `--data-dir`
and run `selfhost list`. Workspaces are separate directories.

**An SSH connection refuses an unknown host key:** verify the fingerprint through
a trusted channel. Do not disable host-key verification to get past the error.

## Build from source (contributors)

Building the unreleased checkout requires Git, Node.js and Rust:

```sh
git clone https://github.com/obiente/selfhost.git
cd selfhost
npm ci --prefix ui
npm run build --prefix ui
cargo install --path . --locked
```

Run `selfhost --help` after installation. This development workflow is separate
from installing a published package.
