# Automatic app setup

Some apps need more than a running container. A Selfhost recipe can describe
first-run initialization, account creation, an API connection and service links.
Review the plan before applying it. The same workflow is available from the
dashboard or a single CLI command sequence; a running Selfhost dashboard is
optional.

## Supported workflows

| App       | Automatic setup                                                                                                                                  |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Homarr    | First administrator, private API key, private board and selected service links; or connect with an existing administrator API key                |
| Nextcloud | Recipe initialization with a generated local administrator; native `occ` actions and a ZITADEL OIDC connection                                   |
| Grafana   | Recipe initialization with a generated local administrator; ZITADEL OIDC configuration and an optional verified human organization administrator |

Initialization and identity setup are separate operations. Homarr's first-run
administrator uses its local login. The Homarr onboarding profile does not
configure OIDC. See [native settings and identity connections](app-integrations.md)
for Nextcloud and Grafana.

New projects retain the onboarding profile from the recipe used to create them.
Projects created before that profile was available do not gain it automatically;
Selfhost does not silently upgrade their frozen recipes. Homarr's existing-installation
mode applies to the recorded Homarr deployment that has this profile. Linking an
arbitrary existing app for discovery does not attach or enable this setup workflow.

## Set up Homarr from the dashboard

Start the Homarr service, then open its setup section in the project.

1. Choose **Initialize a fresh installation**, or **Connect and populate an
   existing installation** if you already completed Homarr's setup.
2. For a fresh installation, choose the administrator username and password. For
   an existing installation, supply an administrator API key created in Homarr.
3. Choose a private board name. Review the suggested services, edit their URLs,
   remove unwanted entries or add your own links.
4. Select **Review setup**, check the actions, then apply.

Workspace suggestions include project services with an app hostname and linked
existing apps with an HTTP or HTTPS URL. They are suggestions, not a network scan
or connectivity guarantee. Use addresses that the dashboard's users can reach.
In particular, `localhost` always refers to the computer running their browser.

Fresh setup creates the local administrator, signs in temporarily, generates and
privately saves an API key, disables Homarr analytics and search-engine indexing,
finishes initialization, and creates the private board and reviewed links. The
existing-installation mode creates the board and links without repeating account
initialization or changing those initial privacy settings.

## Use the CLI

For a workspace project:

```sh
selfhost app-setup PROJECT homarr
```

For an independent app directory:

```sh
selfhost app --directory ./home setup homarr
```

The wizard asks for the setup mode, required fields, and application links.
Passwords and API keys are hidden while you enter them. Workspace projects offer
existing service links as suggestions; standalone directories remain independent
of the global workspace. Review the actions and answer y/n. Selfhost retains
revision checks internally, so no request file or revision copying is required.

Use `setup homarr --inspect` (or `app-setup PROJECT homarr --inspect`) for read-only
status. Repeating setup after completion offers to add application links.

## Reuse the API connection

In the dashboard, reopen the completed setup section and select additional links.
From an independent directory:

```sh
selfhost app --directory ./home sync homarr
```

The wizard suggests known services and lets you add names and URLs individually.
It reuses the saved private API key and board after review and confirmation.

These commands add both app entries and items to the saved private board. A URL
already linked through this workflow, or repeated in the request, is rejected.
Inspect the board before removing or changing a previous link yourself.

In a workspace, use `app-setup-plan` and `app-setup-apply` with a request containing
`"mode": "sync"`, `"inputs": {}` and the new `apps` array. This reuses the saved
connection and follows the same reviewed workflow.

The older `selfhost sync PROJECT homarr` command and dashboard **Sync services**
action also accept the saved API key. That operation adds missing app entries for
other services in the same project, matching their URL; it does not place items
on the board. For that operation, an optional `SELFHOST_DASHBOARD_API_KEY`
environment variable supplies a different key for the call, and `--key-env
VARIABLE_NAME` selects another variable. An explicitly supplied key is not saved.

The password and temporary sign-in cookies are not saved in the onboarding
receipt. The API key is saved privately for later sync and is omitted from setup
results and public status. Filesystem permissions protect that file; the file is
not encrypted.

## Deployment and recovery boundaries

HTTP onboarding currently requires Selfhost to run on the app's local Docker
host. The service must be running with its expected image, Compose ownership
labels and published IPv4 port. Selfhost verifies that destination before sending
credentials. For a remote server, run the standalone CLI there. An existing remote
server connection alone does not enable HTTP onboarding through a tunnel.

Fresh bootstrap refuses an app whose initialization has already begun. Once a
Selfhost onboarding attempt starts, bootstrap and connect cannot be rerun
automatically, even if a later step fails. After successful completion, reviewed
sync can add more links. Each remote operation is journaled before dispatch so a lost
response cannot silently create duplicate accounts, keys or boards.

Inspect the saved status and the app itself after an interruption. The private
project receipt is `onboarding/SERVICE.json` under that project's metadata. It
records the pending step, completed step indices and captured resource IDs when
responses were received. It can also contain the API key. Do not delete it and
retry blindly: an unacknowledged remote write may have succeeded. Automatic
rollback and partial setup resume are not implemented.

Applications, accounts, boards and links continue working without Selfhost. Keep
the private metadata only when you want its helper operations and recovery
records. Back up application data separately.

## Add a contributor workflow

Onboarding contracts live in `catalog/onboarding/APP_ID.json`, beside the app's
TOML recipe. They declare fields, modes, initial-state inspection, HTTP steps,
response captures and per-app loops. App API paths and payloads belong in these
contracts. Projects retain the profile they were created with, so updating a
catalog does not silently change an existing project's setup procedure.

Profiles are trusted executable configuration. Test a new workflow on a disposable
installation, including interrupted writes, credential handling and an already
configured instance, before offering it to users. Support depends on the app and
profile version; the presence of an HTTP API alone does not imply automatic setup.

## File-based automation

Scripts may still use `app-setup-plan` and `app-setup-apply` (or standalone
`setup-plan` and `setup-apply`) with an explicit request file and reviewed revision.
That request contains a `mode`, a map of `inputs`, and an `apps` list. Secrets in
input files can use `env:VARIABLE_NAME`. The guided commands are the normal
human workflow and require neither files nor environment variables.
