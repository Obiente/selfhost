# Native profiles for existing catalogue apps

Native app settings are available from the dashboard and from `selfhost app config`, `plan` and `apply`. Applying a configuration change saves a recoverable configuration snapshot. Environment and mounted-file edits require a separate container recreation. They do not restart services automatically.

The files remain ordinary Compose, environment and app configuration files. You can use Docker Compose without Selfhost. Native application data, uploads and databases need their own backups; a configuration snapshot is not a data backup.

## Examples

Create a private notification server:

```sh
selfhost app init ntfy
selfhost app start
selfhost app actions ntfy
selfhost app action ntfy create-user --inputs administrator.json
```

`administrator.json` contains `username`, `password` and `role` (`admin` or `user`). Keep this file private and remove it after use. The action sends the password through the container environment, rather than placing it in command arguments. Repeating the action for an existing username preserves that account's password and role. Anonymous topic access is denied by default; use the topic permission action for exceptions.

Create Node-RED with a protected editor:

```sh
selfhost app init node-red --method configured
selfhost app start
```

The generated `.env` contains the administrator password and a separate flow encryption key. `files/settings.js` configures the editor and Admin API, persistent context and the `/api` flow prefix. HTTP In nodes still need their own access controls. Keep the encryption key stable when recreating or moving this deployment.

Create Prometheus with editable scrape configuration:

```sh
selfhost app init prometheus --method configured
selfhost app config prometheus
selfhost app action prometheus check-config
```

The configured method mounts `files/prometheus.yml`. Its settings form supports intervals, labels, scrape jobs and rule files. Validate before recreating the service. The basic image method uses the image's own configuration and does not advertise these managed fields.

## Identity connections

Homarr, Vaultwarden, Actual Budget, Readeck and Trilium now have declarative OIDC mappings alongside the earlier application profiles. Provider registration and saving app configuration are separate from verifying browser login. A successful configuration apply does not prove an end-to-end sign-in.

- **Homarr:** local credentials remain available; OIDC account linking by email is disabled. Administrators manage local groups unless they choose IdP group synchronization.
- **Vaultwarden:** SSO still requires the vault master password. Settings saved in its `/admin` interface override environment values; reconcile those overrides before applying changes here.
- **Actual Budget:** restrict the provider application to the intended owner before enabling OIDC. The first OpenID user becomes the owner. Selfhost cannot safely infer or transfer that role.
- **Trilium:** the current user must link their account in Options. Disconnect that link before switching providers.
- **Readeck:** automatic account provisioning is initially disabled. The named provider does not grant administration rights.

## Connecting existing containers

The existing-app catalogue recognizes all original 27 application image repositories. Detection does not adopt their storage, restart them, change their credentials or replace their deployment. Read-only actions are exposed only where a declared command exists and after the user grants action access. An app with discovery support can still have no supported in-container action.

Native recipe features remain frozen when a project is created. An existing project's profile is not silently upgraded when this catalogue changes.

## Verification

`scripts/smoke-existing.mjs` runs explicitly selected disposable local Docker fixtures. It verifies native settings reach recreated containers, persistent volumes are retained, and application-specific contracts such as private ntfy ACLs, idempotent account creation, Vaultwarden SQLite snapshots and Node-RED editor login. It never targets a discovered or production container. The checkout's `.local` directory must be shared with Docker Desktop when testing mounted-file methods.

See [the capability audit](capability-coverage-existing.md) for exact coverage and remaining work.
