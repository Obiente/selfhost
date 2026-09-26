# selfhost

Set up self-hosted services from the CLI, use it again when you need help, or manage
your services through an optional dashboard or terminal interface.

Start with the [getting started guide](docs/getting-started.md), [dashboard guide](docs/dashboard.md), or [CLI guide](docs/cli.md). The documentation website is built from these guides; see [website development](docs/website.md).

## Install

Install a native binary without Node.js or Rust:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/Obiente/selfhost/releases/latest/download/install.sh | sh -s -- --version 0.1.3
```

On Windows, use PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod https://github.com/Obiente/selfhost/releases/latest/download/install.ps1))) -Version 0.1.3
```

The installers verify checksums, need no administrator privileges and refuse to
replace an existing binary unless explicitly requested. You can inspect the
scripts or manually download a binary from the
[release assets](https://github.com/Obiente/selfhost/releases/tag/v0.1.3).
See [installation options](docs/getting-started.md) for custom directories and PATH.

These instructions target **0.1.3**. See the [release page](https://github.com/Obiente/selfhost/releases/tag/v0.1.3)
for its publication status and release notes.

Run directly with Node.js 22+ or pnpm:

```sh
npx selfhost@0.1.3 --help
pnpm dlx selfhost@0.1.3 --help
# Also supported: pnpx selfhost@0.1.3 --help
```

Or install a command globally:

```sh
npm install --global selfhost@0.1.3
# Alternatively: pnpm add --global selfhost@0.1.3
selfhost --help
```

Cargo users need Rust 1.98+ and platform build tools:

```sh
cargo install selfhost --locked --version 0.1.3
selfhost --help
```

The executable contains the dashboard and catalog. npm bundles native binaries;
Cargo builds its source package. Neither installation requires cloning the
repository. Docker with Compose is needed to run container services.

Open the one-use local sign-in link printed by `selfhost serve`. Use `selfhost tui`
for the terminal interface or `selfhost --help` for command-line usage.

The dashboard/API defaults to **8372**: **S = 83** and **H = 72** in ASCII,
joined as **8372**. Override it with `selfhost serve --port PORT`.

For development, see [building from source](docs/getting-started.md#build-from-source-contributors).

## Use as much or as little as you need

The [standalone CLI](docs/standalone.md) creates ordinary Compose, environment and
configuration files in your own directory. Start an app and close Selfhost. You can
use Docker Compose directly, or return to the CLI for updates, native app settings
and supported identity-provider integrations. No dashboard, Selfhost account or
background process is required.

Workspace commands, the TUI and the dashboard can also share a project store.
Application data directories come from the operating system; use `--data-dir` to
choose another workspace. Runtime state, secrets, snapshots and Compose files
belong outside this repository.

## Servers and workload moves

The dashboard's **Infrastructure** page connects Docker over SSH, existing Docker contexts, and Proxmox over SSH. It supports server groups, cluster inventory, backend-enforced read-only connections, Docker project data moves with verification/recovery, and native offline Proxmox VM/LXC migration within a cluster.

See [infrastructure.md](docs/infrastructure.md) for supported cases, migration safeguards, testing and current limits. Live Proxmox migration is not yet verified on a real guest.

## Service recipes

The app catalog uses [TOML manifests and JSON profiles](catalog/). Ports, image references, container data paths, environment settings, generated secrets, deployment methods and optional integration workflows are declared there. Generic Rust drivers validate and execute the supported configuration formats.

Add a manifest, validate it, and submit it to [obiente/selfhost](https://github.com/obiente/selfhost). See [CONTRIBUTING.md](CONTRIBUTING.md) for the format and [app setup notes](docs/catalog-apps.md) for initial configuration.

Private catalogs work without rebuilding:

```sh
selfhost --catalog-dir ./my-recipes catalog
selfhost --catalog-dir ./my-recipes serve
```

Matching IDs in a private catalog replace bundled recipes for **new projects only**. Every project stores its resolved manifest, so a catalog update cannot silently change an existing project's ports, storage layout, or secret definitions. Image tags can still be mutable; immutable image digests are supported for reproducible pulls.

## Current capabilities

- Create and configure multiple projects using catalog recipes.
- Start, stop, and restart project services through Docker Compose.
- Refresh images using the configured tags or digests.
- Save and restore configuration snapshots, including generated secrets.
- Schedule configuration snapshots and image refreshes while `selfhost serve` is running.
- See action history and notifications in the dashboard.
- Sync app links to dashboards that declare an API recipe. Homarr is included.
- Manage projects in the CLI and TUI as well as the browser.
- Choose a deployment method, including Nextcloud's single-image and AIO setups.
- Link existing applications separately from Selfhost-managed projects.
- Connect an existing identity provider, or host one with a declarative stack.
- Review removals, choose a backup, archive project records and reconcile interrupted operations.

Apps bind to localhost by default. Choosing local network access binds their ports to all interfaces. The Selfhost management server always binds to loopback. Its startup link is a one-use local login valid for ten minutes; authenticated sessions expire and can be revoked from the account menu. OpenID Connect can also use the exact local address, with a separate browser proof for both HTTP and HTTPS loopback sessions. HTTPS cookies remain Secure. Remote dashboard access requires a trusted HTTPS proxy and explicitly authorized identities. Read the [identity setup guide](docs/identity-setup.md) and [security boundaries](docs/security.md) before exposing it remotely.

Generated secrets are stored outside source control. Snapshots contain those secrets and should be treated as private. **Configuration snapshots do not back up application data in Docker volumes.** Restoring a snapshot restores desired configuration; starting the project applies it. Images are not automatically rolled back after a failed refresh.

Scheduled jobs are processed by the dashboard server. Missed runs are coalesced into one run on startup. Schedules are not a background OS service, and an abrupt process shutdown can interrupt an action. Image refreshes pull the configured image reference; they do not automatically select a new version tag.

Declarative app setup can initialize supported applications, create their administrator account and connect their API. Homarr's setup creates a private board containing reviewed service links and keeps its API key in private local configuration. Identity connections can create a dedicated provider project; supported app profiles can map a reviewed human account to an app administrator role. These helpers also work from the standalone CLI. See the [app setup guide](docs/app-onboarding.md) for supported flows and recovery boundaries.

## Verification

```sh
npm run build --prefix ui
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo package --allow-dirty
```

The product direction and remaining work are tracked in [docs/roadmap.md](docs/roadmap.md). Distribution is described in [docs/distribution.md](docs/distribution.md).

Service details include live CPU/memory/I/O stats, recent logs, and per-service start, stop, restart and image updates. Extra actions are declared as command arrays in service recipes. See CONTRIBUTING.md for the schema and execution behavior. Stats are current samples; historical charts and log retention are not implemented.

## Custom setups and database sources

Native app integrations add typed settings, reviewed changes and restoration, app workflows, schedules, and guided identity connections to the dashboard and CLI. Nextcloud and ZITADEL are the first profiles. See [native app integrations](docs/app-integrations.md) for commands, contributor contracts, tested versions, and current limits.

Create multi-service Compose setups, edit environment variables and mounted configuration files in the dashboard or CLI, and export them for ordinary Docker Compose. PostgreSQL can use a dedicated project container, a shared server with separate project databases and roles, or an existing external database. See [setups and databases](docs/setups-and-databases.md) for commands, supported paths and data backup requirements.

## Networking and access

Connect reverse proxies, record private networks, configure Selfhost OIDC login, or create your own identity-provider project from the dashboard and CLI. See [Networking and login](docs/networking-and-login.md) for setup, supported drivers, and current limits.

## Existing apps, deployment choices and removal

Use **Existing apps** to link a Nextcloud AIO, ZITADEL or another supported service you already run. Linking starts read-only and does not adopt its runtime or storage. See [existing apps and deployment methods](docs/existing-apps.md).

Removal defaults to archiving the Selfhost record while containers and data remain. Container deletion requires a reviewed ownership check and exact-name confirmation. Configuration snapshots, database archives and application data have different coverage; choose and review the backup before applying. AIO's child lifecycle stays with AIO, so its Selfhost record can be archived but its runtime must be removed through AIO's own controls. See [removal and recovery](docs/removal.md).

The [release workflow](.github/workflows/release.yml) builds and tests npm and Cargo
artifacts. Publication is opt-in and requires a matching version tag and registry
setup. See [distribution](docs/distribution.md).

## Updating Selfhost

The dashboard Updates button, terminal UI and `selfhost update` can check releases and show the appropriate method for this installation. Official Cargo releases support reviewed staging and activation with executable recovery. Source builds and npm-managed copies retain their existing installation workflow. See [updates and recovery](docs/updates.md), including how to retrieve a new private sign-in link after a dashboard restart.
