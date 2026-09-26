# App setup and native profiles

Selfhost profiles describe the settings and operations an app actually supports. You can use them from the dashboard or from an ordinary Compose directory with `selfhost app`. An app keeps running independently after Selfhost finishes.

## Available workflows

| App               | Setup and configuration                                                                | Operations                                                         | Identity                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| Homarr            | Create the first administrator, save a private API key and create a private board      | Add reviewed links; optional reconciliation tasks add new projects | Existing app setup remains available in Homarr                                            |
| Jellyfin          | Complete a new server's wizard with your selected administrator and server name        | Local health check                                                 | Configure any additional authentication plugins in Jellyfin                               |
| FreshRSS          | Initialize SQLite and your selected administrator                                      | Create users, list users, back up user databases                   | Keep local login; no automatic IdP registration in this profile                           |
| Gitea and Forgejo | Public URL, registration policy, private content, installation lock                    | Create an administrator, list users and authentication sources     | Register a confidential OIDC client and create a source called `selfhost`                 |
| Linkding          | Trusted origins, specific allowed internal hosts, login options                        | Django configuration check                                         | Register OIDC with PKCE, verified TLS and local login retained                            |
| Gotify            | Registration, secure cookies and OIDC settings                                         | Initial admin is created by the image using the generated password | Register OIDC while retaining the local administrator                                     |
| Mealie            | Public URL, signup policy, OIDC discovery, client credentials and optional group rules | Complete first account setup in Mealie                             | Enter an existing provider's settings; automatic client creation is not advertised        |
| SearXNG           | Public URL, image proxy, request limiter and Valkey connection                         | Generated signing secret and persistent configuration              | Protect a public deployment through your authentication proxy                             |
| Grafana           | Public URL and native authentication settings                                          | Plugin listing                                                     | Register OIDC; optionally map one verified provider subject to organization administrator |
| Nextcloud         | Native `occ` settings                                                                  | App updates, cron and other declared `occ` workflows               | Register and configure the OIDC app                                                       |

An OIDC connection does not imply administrator access. Only profiles that explicitly describe a supported administrator mapping offer it. Linkding associates users by email; use a trustworthy provider with verified email claims. Review existing-account matching in the app before enabling login. Gotify's new OIDC accounts are not automatically administrators. Gitea and Forgejo retain their explicitly created local administrator.

## First-run examples

### Jellyfin

```sh
selfhost app --directory ./media init jellyfin --start
selfhost app --directory ./media setup jellyfin
selfhost app --directory ./media setup-plan jellyfin setup.json
selfhost app --directory ./media setup-apply jellyfin setup.json --revision REVISION
```

Use a private `setup.json`:

```json
{
  "mode": "bootstrap",
  "inputs": {
    "username": "admin",
    "password": "env:JELLYFIN_ADMIN_PASSWORD",
    "server": "Home media"
  },
  "apps": []
}
```

Set the password in your shell environment before running the command. The password must contain at least 12 characters. Bootstrap refuses a server whose initial setup is already complete, and uncertain writes block automatic retries.

This recipe persists configuration and cache in one volume. Add your own read-only media mounts using a custom Compose setup and configure libraries in Jellyfin. Hardware devices, transcoding and DLNA discovery are deliberately selected by the user rather than silently granting device or host-network access. Keep the published port private until the administrator is created. Remote access is enabled for Docker/proxy networking; Selfhost does not change router configuration.

### FreshRSS

```sh
selfhost app --directory ./feeds init freshrss --start
selfhost app --directory ./feeds action freshrss initialize --inputs administrator.json
selfhost app --directory ./feeds action freshrss list-users
selfhost app --directory ./feeds action freshrss backup-database
```

The initialization input is a JSON object containing `username`, `password`, `email` and `url`. This command creates the installation and selected account in two explicit steps. If a step fails, earlier successful steps remain applied. Read the result before retrying; an existing installation can use the separate `create-user` action. Do not delete its volume to retry setup.

The backup operation writes app-native database backups into FreshRSS's data volume. Copy them to independent storage for disaster recovery. A configuration snapshot alone is not a database backup.

### Gitea and Forgejo

Initialize the app, review `install-lock: "true"` with `app plan`, apply the returned revision and recreate it using `app start`. Then run `create-admin` with a private JSON input containing `username`, `password` and `email`. The action never resets an existing user's password or promotes an existing account. `list-users` shows its result.

The OIDC workflow creates an authentication source named `selfhost`, using `/user/oauth2/selfhost/callback` as its callback. A source with that name is not silently replaced. Keep local login available until you have tested a browser sign-in. Native CLI commands may expose their arguments to other administrators on the same Docker host; use a trusted host.

### SearXNG and Mealie

SearXNG has no built-in administrator login. The default recipe is a private instance with an independently generated signing secret. Do not expose it publicly without reviewing access controls and rate limiting. Enabling its limiter requires a separately configured Valkey connection. Signing secrets live in the portable `.env` file; use the environment editor when rotating them.

Mealie's current default-account values are private application constants, not supported environment settings. Finish the account setup in Mealie. Its profile exposes documented OIDC settings, including an optional confidential-client secret. Group-based administrator access is advanced and must match reviewed IdP group membership. No group is made an administrator by default.

## Maintaining a profile

- `catalog/APP.toml` owns the image, ports, data directory, generated secrets and simple container commands.
- `catalog/integrations/APP.json` owns typed native settings and reviewed operations. Environment profiles write ordinary Compose values; native YAML and command profiles retain the app's own configuration format.
- `catalog/onboarding/APP.json` owns first-run HTTP operations, accepted modes, startup-state detection, input validation and response captures.
- `catalog/versions/APP.json`, when present, owns the reviewed image choices and compatibility evidence. Pin a new image only after its relevant smoke checks pass.

No Rust app-name switch is needed for these profiles. Use `minimum_length` and `maximum_length` for string/secret action inputs. Declare a command as `schedulable` only when it has safe defaults for every input. A new app's JSON should describe only implemented workflows; do not copy another app's modes or administrator mapping.

HTTP onboarding supports string and boolean startup states. `modes` and `each_app` determine which controls appear. Bootstrap-only services do not display dashboard-link controls. Every remote write is journaled before dispatch; an interrupted or uncertain operation requires inspection. The selected running container's Compose labels, image and published port are verified before credentials are sent.

## Acceptance checks

Run the opt-in local Docker suite against a freshly built executable:

```sh
node scripts/smoke-profiles.mjs ./target/debug/selfhost
# Or select a smaller batch:
node scripts/smoke-profiles.mjs ./target/debug/selfhost jellyfin freshrss
```

The suite creates uniquely named Compose projects, uses random loopback ports, tests profile commands/settings through the CLI, recreates containers and verifies persistent state. It removes only its own containers, volumes and temporary files. It requires a local Docker context and downloads the selected recipe images if absent.

HTTP readiness and native configuration tests do not prove end-to-end OIDC sign-in, external feed fetching, media playback/transcoding or public-proxy behavior. Test those against the provider, hardware and network you actually intend to use before replacing a working deployment.

## Sources and selection

[Docker Compose Maker](https://compose.ajnart.dev/) and [Awesome Selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted) were used to discover useful candidates. The recipes and integrations here are maintained for Selfhost and use upstream configuration contracts:

- [Jellyfin container installation](https://jellyfin.org/docs/general/installation/container/) and [startup API source](https://github.com/jellyfin/jellyfin/blob/master/Jellyfin.Api/Controllers/StartupController.cs)
- [FreshRSS command line](https://github.com/FreshRSS/FreshRSS/blob/edge/cli/README.md)
- [Gitea administration](https://docs.gitea.com/administration/command-line/) and [Forgejo administration](https://forgejo.org/docs/latest/admin/command-line/)
- [Linkding options and identity setup](https://linkding.link/options/)
- [Gotify configuration](https://gotify.net/docs/config)
- [Mealie backend settings](https://mealie.io/documentation/getting-started/installation/backend-config/)
- [SearXNG container installation](https://docs.searxng.org/admin/installation-docker.html)
