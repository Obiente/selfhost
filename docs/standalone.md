# Use Selfhost without a dashboard

Set up an app in a directory you own, run it, and close Selfhost. The app runs
through ordinary Docker Compose. No Selfhost server, account or background service
is required. You can return to the CLI for an update, a native setting change or a
supported integration whenever you need it.

## Create an app

```sh
selfhost catalog
selfhost app --directory ./cloud init nextcloud
```

Initialization creates a new directory containing `compose.yaml`, `.env`, any
declared files under `files/`, and private helper metadata under `.selfhost/`.
It does not start containers by default and does not need a running Docker engine.
An existing destination is never silently replaced.

Review the generated files, then start the app:

```sh
selfhost app --directory ./cloud start
```

For a recipe you have already reviewed, `init nextcloud --start` combines the two
steps. Use `--catalog-dir` to load your own contributor recipes.

You can also use Docker Compose directly:

```sh
cd cloud
docker compose -f compose.yaml up -d
docker compose -f compose.yaml logs --tail 100
```

Selfhost does not need to be installed for those Compose commands. Keep the
generated Compose `name` and volume declarations: they identify the app's existing
storage on that Docker engine even if you rename the directory.

## Return when you need help

From any directory, pass the same app location:

```sh
selfhost app --directory ./cloud status
selfhost app --directory ./cloud logs nextcloud
selfhost app --directory ./cloud update
selfhost app --directory ./cloud stop
selfhost app --directory ./cloud start
```

These are individual operations, not a persistent management process. `update`
asks which version to use for each recorded service, shows compatibility and
migration warnings, then asks for confirmation before saving and restarting.
It also allows keeping the current image references and refreshing those tags.
Configuration snapshots do not back up application data or database contents.

## Edit your own configuration

The files in the app directory are the source of truth. Edit Compose, `.env` and
declared app configuration files with your preferred editor. Selfhost reads those
files again when you invoke it. It does not continuously reconcile them or revert
your changes in the background.

Generated credentials are private. Keep `.env`, app files and `.selfhost/` out of
public repositories. Review output before sharing logs. Changing an initial
administrator password variable does not necessarily change an account that the
app has already created.

Recipes with native integration profiles support guided setting changes:

```sh
selfhost app --directory ./cloud config nextcloud --edit
selfhost app --directory ./cloud actions nextcloud
selfhost app --directory ./cloud action nextcloud update-apps
```

Choose a setting, answer its typed prompts and review the changes. Passwords are
hidden. Lists and command arguments are entered one item at a time. Selfhost
keeps the revision internally and asks for y/n confirmation before applying.
The same settings wizard is available through `apply nextcloud` without a file.

Use `backups SERVICE` to list native configuration backups, then
`restore SERVICE BACKUP` for a guided review and confirmation. These backups cover
native settings, not uploads, database contents or Docker volumes.

## Connect an app to an identity provider

App sign-in is separate from signing in to a Selfhost dashboard. You can configure
an app's supported identity connection entirely through the CLI.

Run the connection wizard:

```sh
selfhost app --directory ./cloud connect nextcloud
```

Choose the provider and enter the provider and app URLs. The temporary provider
API token is entered privately. Where the provider supports it, Selfhost creates
a dedicated project and client. Advanced options allow an existing project,
organization, private CA and deliberate replacement of existing login settings.
Where the app supports administrator mapping, Selfhost can look up the token's
human account and ask you to authorize it explicitly. No subject ID needs to be
copied from a separate command when that lookup succeeds.

Review the callback, administrator and changes, then confirm. Recovery receipts
stay in the private `.selfhost/` directory; the provider token is not persisted.
Run the same command again to inspect and resume a saved connection. An uncertain
provider response still requires inspection before retrying. App configuration
may need service recreation; Selfhost reports it rather than silently restarting.

These workflows cover the declared Nextcloud and Grafana connections to ZITADEL.
They do not imply automatic registration for every app or provider. Keep the app's
local administrator login and test SSO separately. Once configured, sign-in runs
between the app and identity provider without a Selfhost process. See
[identity connection options](app-integrations.md#guided-identity-connections)
for private CAs, localhost URLs, advanced settings and recovery limits.

## Initialize an app and populate its dashboard

Recipes can declare additional setup beyond starting a container. Homarr supports
creating its first administrator, a private API key and a private board containing
the service links you choose. An already configured Homarr can instead use an
existing administrator API key.

```sh
selfhost app --directory ./home init homarr --start
selfhost app --directory ./home setup homarr
```

The wizard asks for setup mode, administrator details, board name and app links.
See [automatic app setup](app-onboarding.md). Standalone setup uses this directory's
private metadata and does not discover an unrelated global workspace. Run it on
the Docker host for HTTP onboarding; remote onboarding through an authenticated
tunnel is not yet available.

Add more links using the saved private API connection:

```sh
selfhost app --directory ./home sync homarr
```

This adds the reviewed apps and items to the private board. Previously linked or
duplicate URLs are rejected. Keep `.selfhost/` if you want this saved connection.
You can also keep using Homarr directly after closing or uninstalling Selfhost.

## Deployment choices and stacks

List available choices before selecting a non-default deployment:

```sh
selfhost deployments
selfhost stacks
selfhost app --help
```

Use `init APP --method METHOD` for an app deployment choice, or
`init BLUEPRINT --stack` for a multi-service stack. Without an explicit method,
the wizard lists deployment choices. It asks for each declared input and explains
requirements before asking for confirmation. The same contributor profiles supply
these options to the CLI and dashboard. See [deployment choices](existing-apps.md).

### Automation

Explicit input files remain available for scripts: `--inputs`, `plan`/`apply`,
`setup-plan`/`setup-apply`, `connect-plan`/`connect`, and `sync-plan`/`sync`. File-based
apply commands require the matching reviewed `--revision`; interactive commands
keep that bookkeeping internal. Use `setup SERVICE --inspect` for read-only status.

Nextcloud AIO manages its child containers, backups and upgrades itself. Its
master-container Compose file does not replace AIO's own lifecycle controls.

## What stays after Selfhost leaves

| File or data                          | Purpose                                                                    |
| ------------------------------------- | -------------------------------------------------------------------------- |
| `compose.yaml`                        | Ordinary Docker Compose services, names, networks and storage declarations |
| `.env`                                | Environment variables used by Compose, including generated credentials     |
| `files/`                              | Declared native configuration and setup files                              |
| `.selfhost/`                          | Optional Selfhost recipes, operation records and configuration backups     |
| Docker volumes and external databases | Actual app data, backed up separately                                      |

Removing Selfhost itself or its helper metadata does not stop containers or remove
volumes. Keep `.selfhost/` if you want to reuse profile actions, reviewed changes or
connection recovery. It is not required by Docker Compose or the applications.
Do not discard recovery records while an operation is incomplete.

The standalone commands work with directories created by this workflow. They do
not silently take over arbitrary existing Compose deployments. You can continue
using Compose directly, or use [existing app connections](existing-apps.md) for
explicit read-only discovery and selected actions. Workspace exports also remain
usable independently; see [leaving Selfhost](setups-and-databases.md#leaving-selfhost).

Helpers retain the portable-setup validation rules: no `include`, `extends`, build
contexts, `env_file` or arbitrary host binds. Declared files use `./files/NAME`,
and container images remain explicit. If your changes need unsupported Compose
features or rename recorded services, use Docker Compose directly. Do not open
`.selfhost/` as a dashboard workspace; it contains helper records for this directory.

Copying this directory does not copy named-volume or external-database contents.
Back up and transfer those separately when moving engines. Keep native app backup,
restore and upgrade procedures available alongside the generated configuration.
