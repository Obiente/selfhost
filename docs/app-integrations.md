# Native app integrations

App recipes can describe native settings, setup steps, management actions, schedules, and identity connections. The same profile powers the dashboard and CLI. App-specific paths, field names, container users, commands, and API payloads live in `catalog/integrations/*.json`.

For an app created outside a Selfhost workspace, use the equivalent
`selfhost app --directory DIR` commands in the [standalone guide](standalone.md).
Neither app configuration nor app-to-provider login requires a running dashboard.
For guided first-run initialization, API connections and dashboard population, see
[automatic app setup](app-onboarding.md).

## Using an integration

Open a service's **App settings** tab. Basic settings appear first; **Advanced** adds operational settings, arbitrary declared command arguments, and configuration restoration. A setting change follows preview, apply, readback verification. The preview includes a revision of the configuration and deployment: if either changes, review again.

```sh
selfhost app-config PROJECT nextcloud --edit
selfhost app-action PROJECT nextcloud update-apps
selfhost schedule PROJECT app:nextcloud:update-apps --hours 24
selfhost app-backups PROJECT nextcloud
```

Select settings from a menu and enter typed values. Action inputs use the same
profile-driven prompts, with hidden secrets and individual list or argument
entries. A readable review and y/n confirmation replace request files and copied
revisions. Explicit `app-plan`/`app-apply` files remain available for automation.

Backups save previous managed values before writes. Restore also previews changes, checks the revision, saves the current values, and verifies the result. A partially failed command is not automatically rolled back. Configuration backups do not replace application-data backups.

## Nextcloud

The profile reads JSON through `occ config:list system` and sends a managed patch to `occ config:import` over stdin. It runs as `www-data`, preserves unrelated settings, and supports Nextcloud's null-to-delete behavior when restoring originally absent settings. It does not parse or execute PHP configuration files on the host.

Available workflows include installation status, app listing, app installation and updates, file scans, repair, cron, and advanced `occ` arguments. Cron selects the cron backend and executes `cron.php`. Scheduled actions use the same workflow, write activity outcomes, and run while `selfhost serve` is active. Interval schedules currently have a minimum of one hour; use an external five-minute cron for normal Nextcloud background processing.

Fresh Nextcloud recipes generate an administrator password and initialize a small SQLite installation automatically. Attach PostgreSQL **before first start** to use an external, shared, or dedicated database. The database recipe removes the SQLite bootstrap variable so PostgreSQL is actually selected. Existing instances need a deliberate database migration.

The native command workflow was exercised with Nextcloud 34.0.4 and `user_oidc` 8.11.0. Recipes are snapshotted in projects; changing a catalog does not automatically upgrade an installed integration. Runtime version negotiation is not yet implemented.

## ZITADEL YAML

The ZITADEL profile manages runtime YAML fields such as external URL settings, TLS, port, and logging. Add `x-selfhost-integration: zitadel` to a custom Compose service, mount the declared file, and pass it explicitly:

```yaml
services:
  identity:
    image: ghcr.io/zitadel/zitadel:YOUR_TESTED_VERSION
    x-selfhost-integration: zitadel
    command: [start, --config, /config/zitadel.yaml]
    volumes:
      - ./files/zitadel.yaml:/config/zitadel.yaml:ro
```

This is an integration attachment example, not a complete ZITADEL installation. Supply the database, master key, login service, TLS routing, and initialization required by your ZITADEL version. Keep initialization `--steps` separate from runtime `--config`.

Declare `zitadel.yaml` in the setup's `files` mapping. Edits operate on this saved file, preserve unrecognized keys and values, and require starting the project to materialize and apply them. YAML comments, anchors, and formatting are not preserved. A single explicit configuration file is supported. Known environment overrides and `env_file` are rejected because they can shadow the edited YAML. Image-baked overrides are outside this saved-file adapter's inspection; check the image configuration before attaching it. This adapter does not claim to read effective runtime settings from ZITADEL.

Mounted files currently require a local Docker engine and suitable Docker Desktop file sharing. ZITADEL runtime startup has not been verified in the local Windows test environment.

## Guided identity connections

Nextcloud and Grafana can connect to an existing ZITADEL provider. Enter the provider URL, app URL and a temporary provider token. Selfhost creates a dedicated provider project named for the app by default, discovers the token's organization, and registers one OIDC authorization-code client. ZITADEL permissions must allow `project.create` and `project.app.write`. Review the project, destination and callback before applying.

Provider and app URLs accept HTTPS domains or HTTP loopback development addresses such as `http://localhost:3000`. Use origins without subpaths. A localhost callback is reached by the browser, so it must point to the app on that browser's computer. Provider discovery and API calls must also be reachable from the machine running Selfhost.

```sh
selfhost app-connect PROJECT nextcloud
```

Choose the provider and enter its URL, the app URL and the temporary token when
prompted. Token entry is hidden. Supported administrator mapping offers a human
account lookup followed by an explicit authorization question. Machine credentials
never receive app administrator access automatically. Review the complete plan
and answer y/n; no JSON request or revision copying is needed.

Advanced options let you reuse an existing provider project, select an organization,
load a private PEM CA bundle, or explicitly replace existing app login settings.
The app's own runtime trust store still needs that CA when applicable. Explicit
file-based `app-connect-plan`/`app-connect` commands with `--token-env` remain
available for automation.

The temporary token is used in memory and not persisted. Client credentials and
recovery receipts are retained privately. HTTPS certificate verification is enabled
and HTTP redirects are not followed. Filesystem permissions restrict the store.

Connection progress is written before project and client creation. If a request outcome is uncertain, automatic creation retries are blocked. Inspect the recorded project and application IDs at the provider and the private response, if present, before manual recovery. If the client exists but app configuration failed, **Reapply app connection** reuses it. Configuration, deployment or server changes require a new review before recovery. One managed connection per service is supported; provider deletion, secret rotation and automatic domain changes for an existing connection are not implemented.

### Grafana administrator selection

Use the same commands with service `grafana` and its app URL. Setting `administrator_subject` to the verified human account configures an exact OIDC subject match for **Grafana organization administrator**. Other users admitted through this OIDC connection receive Viewer. This does not grant Grafana server administrator or identity-provider administrator permissions.

The profile writes ordinary `GF_*` Compose environment settings, keeps local login enabled and disables automatic redirection to the provider. It **does not restart Grafana**. Recreate the service from its updated Compose configuration when ready, then test sign-in. Native settings use the `compose_environment` driver; existing interpolated fields such as `${PUBLIC_URL}` must be edited through `.env` or Compose directly so their interpolation is preserved.

### Nextcloud administrator selection

Nextcloud's declared workflow configures the OIDC client through `occ`. It does not automatically map the provider account to a Nextcloud administrator. Provider project ownership is also not an app administrator grant. Keep your existing Nextcloud administrator and assign application permissions deliberately after the first successful provider login.

Keep a local administrator login until a separate browser session successfully signs in. The final workflow status is **Ready for a sign-in test**. Provider registration and configuration tests do not establish that a real browser login works for your provider, DNS, TLS and app combination.

## Contributing a profile

Add a JSON file named after the recipe ID next to the bundled profiles. Additional catalogs may provide `integrations/RECIPE_ID.json` beside their TOML manifest. Custom Compose setups can attach these loaded profiles by ID too. No central list or app-specific Rust branch is required.

Use schema `1`, a display name, documentation URL, a driver, typed fields, and optional actions. See the Nextcloud and ZITADEL profiles for complete examples.

- Fields declare `id`, `label`, `kind`, and a vector `path`. Types: `string`, `secret`, `integer`, `boolean`, `string_list`, `arguments`. Optional `choices`, `minimum`, `maximum`, `default`, `advanced`, and `environment` describe controls and validation. Secret settings are omitted from reads and masked in previews.
- A `command` driver declares JSON read and stdin JSON write commands, a root `scope`, and whether null deletes settings. It must implement a merge, not overwrite unmanaged configuration.
- A `yaml` driver declares `file`, container `mount`, and the CLI `argument` selecting the file. Fields map to nested YAML paths. Unknown values survive semantic serialization.
- A `compose_environment` driver maps each field to one environment key. It preserves unrelated values, escapes literal dollar signs for Compose, and requires service recreation to activate changes. Interpolated declared fields must be edited through their original source.
- Actions declare typed `inputs` and ordered `steps`, each with `label` and a `command`. Commands have an `argv` array, optional container `user`, and optional environment templates. `skip_if` skips a step when its check exits successfully. Only mark actions `schedulable` when every input has a default and repeated execution is appropriate.
- `{{input-id}}` substitutions occur once. Arrays expand to separate arguments only when occupying an entire argument. Values never become shell syntax or further templates. Put secrets in command environment or stdin, not argv.
- `oidc_client` defines callback path and either a configuration action with input mappings or a Compose `environment` mapping. Optional `administrator_environment` and `administrator_role` describe an exact, verified human-account mapping. `oidc_provider` defines the create endpoint, headers, JSON request template, and client credential JSON pointers. The provider must accept the requested unique application ID. API and callback details belong in these contracts.

Commands execute only inside the selected service container. Profiles are trusted executable configuration and must be reviewed. Actions enforce server write permissions, have a five-minute wait limit and a 1 MiB output limit. A timed-out container command may continue, so inspect app state before retrying. Step output is shown in the active request and is not placed in activity history; explicitly declared input secrets are redacted.

## Portability

Settings remain in the application's native files or database. Compose exports retain mounted files and environment. Nextcloud `occ`, ZITADEL YAML, ordinary Compose, and external scheduling continue to work without selfhost. Application volumes must be backed up separately when exporting or changing engines. The private connection record supports recovery but is not a runtime dependency.

References: [Nextcloud occ](https://docs.nextcloud.com/server/latest/admin_manual/occ_command.html), [Nextcloud user_oidc](https://github.com/nextcloud/user_oidc), [ZITADEL configuration](https://zitadel.com/docs/self-hosting/manage/configure/configure), [ZITADEL Create Application](https://zitadel.com/docs/reference/api/application/zitadel.application.v2.ApplicationService.CreateApplication).
