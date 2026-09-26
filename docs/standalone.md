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
pulls the image references in your current Compose file and recreates services as
needed. It does not select a newer version tag, change a recipe, rotate secrets,
or replace your configuration with a newer catalogue entry. Review the app's
upgrade instructions before editing a pinned image tag. Updates can interrupt
service; a configuration backup is not an application-data backup.

## Edit your own configuration

The files in the app directory are the source of truth. Edit Compose, `.env` and
declared app configuration files with your preferred editor. Selfhost reads those
files again when you invoke it. It does not continuously reconcile them or revert
your changes in the background.

Generated credentials are private. Keep `.env`, app files and `.selfhost/` out of
public repositories. Review output before sharing logs. Changing an initial
administrator password variable does not necessarily change an account that the
app has already created.

Recipes with native integration profiles also support reviewed setting changes.
For Nextcloud, save this as `changes.json`:

```json
{ "default-phone-region": "NL", "maintenance-window": 3 }
```

```sh
selfhost app --directory ./cloud config nextcloud
selfhost app --directory ./cloud actions nextcloud
selfhost app --directory ./cloud plan nextcloud changes.json
selfhost app --directory ./cloud apply nextcloud changes.json --revision REVIEWED_REVISION
selfhost app --directory ./cloud action nextcloud update-apps
```

Replace `REVIEWED_REVISION` with the value returned by the plan. Native commands
run in the selected app container; they do not require the Selfhost API or browser.
Supported fields and actions come from that app's profile. Other configuration can
still be changed through the app's own tools.

Use `backups SERVICE` to list saved native configuration backups and
`restore SERVICE BACKUP` to preview a restore. Applying that restore requires its
returned `--revision`. These backups cover the profile's native settings, not
uploads, database contents or entire Docker volumes.

## Connect an app to an identity provider

App sign-in is separate from signing in to a Selfhost dashboard. You can configure
an app's supported identity connection entirely through the CLI.

For Nextcloud or Grafana with ZITADEL, save a connection request privately:

```json
{
  "provider": "zitadel",
  "issuer": "https://identity.example.com",
  "app_url": "https://files.example.com",
  "name": "Cloud login"
}
```

```sh
# Supply SELFHOST_IDP_TOKEN through your shell or secret manager.
selfhost app --directory ./cloud connect-account nextcloud connection.json
selfhost app --directory ./cloud connect-plan nextcloud connection.json
selfhost app --directory ./cloud connect nextcloud connection.json --revision REVIEWED_REVISION
selfhost app --directory ./cloud connection nextcloud
```

The default creates a dedicated provider project named **Selfhost**, then its
client and the app configuration. To reuse a project, set `create_project` to
`false` and supply `provider_project`. Its
receipt and client credentials stay in the private `.selfhost/` metadata for
recovery. The provider access token is not persisted. If client creation succeeded
but app configuration failed, `connection nextcloud --resume` reuses the recorded
client. An uncertain provider response requires inspection before retrying.

`connect-account` only looks up the credential's account. For Grafana, you may
copy its verified human `suggested_subject` into `administrator_subject` in the
request, then review a new plan. This grants Grafana organization Admin to that
exact subject; other users admitted through the connection receive Viewer.
Machine accounts cannot be selected, and Nextcloud has no automatic administrator
mapping. Grafana's Compose changes require service recreation; connecting does
not restart it automatically.

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
selfhost app --directory ./home setup-plan homarr onboarding.json
selfhost app --directory ./home setup-apply homarr onboarding.json --revision REVIEWED_REVISION
```

See [automatic app setup](app-onboarding.md) for the `onboarding.json` format,
environment-backed passwords and setup modes. Standalone setup uses this
directory's private metadata and the explicit links in your request. It does not
need or discover an unrelated global Selfhost workspace. Run the CLI on the Docker
host for HTTP onboarding; remote onboarding through an authenticated tunnel is
not yet available.

To add more links later, save an array of link objects as `links.json`, then use
the saved API connection and board:

```sh
selfhost app --directory ./home sync-plan homarr links.json
selfhost app --directory ./home sync homarr links.json --revision REVIEWED_REVISION
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
`init BLUEPRINT --stack` for a multi-service stack. Supply declared values through
`--inputs inputs.json` and explicitly acknowledge any listed requirements with
`--ack REQUIREMENT_ID`. The same profiles supply these options to the CLI and
dashboard. See [deployment choices](existing-apps.md).

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
