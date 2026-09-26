# Command-line guide

Every CLI command runs directly. Opening the dashboard, creating a Selfhost login,
and installing a background service are optional.

Use [standalone apps](standalone.md) to generate ordinary Compose files in a
directory you choose, start an app, and leave it running independently. Return to
that directory whenever you want to use Selfhost for an update or integration.

The project commands below use a Selfhost workspace, which can also be opened in
the dashboard. Include the same `--data-dir` when you use a custom workspace.

```sh
selfhost --help
selfhost identity --help
selfhost --data-dir ./selfhost-data list
```

## Interactive terminal

Run `selfhost tui` for guided setup, service management, connections and reviewed
removal. See the [terminal guide](tui.md) for navigation and feature coverage.

## Create and run a project

```sh
selfhost catalog
selfhost create "Home dashboard" --apps homarr,uptime-kuma
selfhost list
selfhost plan PROJECT_ID
selfhost run PROJECT_ID start
selfhost stats PROJECT_ID homarr
selfhost logs PROJECT_ID homarr
```

Replace `PROJECT_ID` with the identifier returned by `create` or `list`.
Inspect logs before sharing them; app output can contain private data.

## Deployment methods

```sh
selfhost deployments
selfhost deploy --file deployment.json
```

`deployment.json` selects the app, method, server and configuration. A deployment
creates a stopped project. Some methods require explicit acknowledgements, which
are listed by `deployments` and presented by the dashboard picker.

## Connect an identity provider from its directory

In your provider directory:

```sh
selfhost identity inspect . --selfhost-url http://localhost:8372
selfhost identity plan --directory .
selfhost identity apply --directory . --revision REVIEWED_REVISION --confirm-callbacks
```

The inspector reads configuration as data. It never sources `.env` or runs provider
scripts. The reviewed manifest supplies the registered client and exact allowed
subject IDs. Providers with a registration profile can create a client through
the reviewed registration command:

```sh
selfhost identity register-plan --file registration.json
selfhost identity register --file registration.json --revision REVIEWED_REVISION
```

The credential comes from `SELFHOST_IDP_TOKEN`, not a command-line argument. See
[identity setup](identity-setup.md) for fields and supported registration profiles.

## Link an existing app

```sh
selfhost existing profiles
selfhost existing link --file existing-app.json
selfhost existing list
selfhost existing inspect CONNECTION_ID
```

The JSON supplies the profile, display name, URL and optional connected Docker
server/container. An existing-app ID is not a managed project ID. Read-only is the
starting point. See [existing apps](existing-apps.md).

## Custom configuration

```sh
selfhost create-setup "My stack" --compose compose.yaml --environment environment.json
selfhost setup PROJECT_ID
selfhost env PROJECT_ID --set EXAMPLE_SETTING
selfhost config PROJECT_ID app.yaml --file app.yaml
selfhost export PROJECT_ID setup.tar
```

`env --set` reads its value from stdin, keeping it out of command history.
Exports include credentials. Store them privately. They exclude persistent volume
data; database archives and application backups are separate.

## Review a removal

```sh
selfhost removal plan PROJECT_ID --mode archive --backup configuration
selfhost removal apply PROJECT_ID --plan-id PLAN_ID --revision REVIEWED_REVISION --confirm "Exact project name" --acknowledge-preserved-data
selfhost removal archives
selfhost removal restore ARCHIVE_ID --confirm "Exact project name"
```

Use `--service SERVICE_ID` when planning removal of one app from a larger project.
Choose `remove_containers` only when you intend to stop and remove its owned
containers. Persistent volumes are preserved. Skipping backups requires an
additional `--acknowledge-no-backup` at apply time. See [removal](removal.md).

## Complete reference

The [command reference](cli-reference.md) is generated from the executable's
help output. For the installed version, `selfhost COMMAND --help` is authoritative.
