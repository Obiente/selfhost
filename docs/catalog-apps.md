# Bundled application recipes

Browse the [full catalogue](/catalogue) to search apps and filter by supported
setup, identity, configuration and management features.

Recipes are native selfhost TOML manifests. Image, port, persistent data path, environment, secrets, database mappings and actions are declared in the recipe. Matching private manifests can override new installations without rebuilding selfhost.

## Additional app setup

| App           | Initial setup and scope                                                                                                                                                                                                                                                                                                                                               |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Gotify        | Sign in as `admin`. The password is generated per project; reveal GOTIFY_DEFAULTUSER_PASS through the project's environment editor or CLI setup output. Changing that variable after initialization does not rotate the existing account password.                                                                                                                    |
| Beszel Hub    | Create the hub account and add separately installed Beszel agents. This recipe installs the hub only. Set APP_URL to the address used by your agents if the host or port changes. A Check hub health action is included.                                                                                                                                              |
| Readeck       | Complete account creation in the app. Its database, settings and saved article assets live in the declared data volume.                                                                                                                                                                                                                                               |
| SilverBullet  | Complete the first-run account and space setup. The current image stores server configuration and spaces under /data.                                                                                                                                                                                                                                                 |
| Forgejo       | Complete the installer. The recipe enables HTTP Git access and disables SSH because no SSH port is published. SQLite is the default; attach PostgreSQL before initialization to use a dedicated, shared or external database. Configure the public URL in Forgejo when moving beyond localhost.                                                                       |
| Trilium Notes | Complete the initial password/database setup in the app. The note database, configuration and attachments use its declared data directory.                                                                                                                                                                                                                            |
| Grafana       | Sign in as `admin` with the generated `GF_SECURITY_ADMIN_PASSWORD`. SQLite is the default. Attach PostgreSQL before initialization to choose a dedicated, shared or external database. The recipe also generates Grafana's encryption key; retain it with backups. Changing the bootstrap password environment variable does not change an existing account password. |
| Prometheus    | Starts with the upstream image's sample configuration, which scrapes Prometheus itself. Metrics persist in `/prometheus`. Add real scrape targets using a custom setup and a mounted `prometheus.yml`; the single-container recipe does not discover your hosts automatically. Configuration validation and readiness actions are included.                           |
| Tautulli      | Complete its setup wizard and connect your existing Plex server. Configuration, statistics and backups live in `/config`. Plex itself is not installed, and host log directories are not mounted. A local health action is included.                                                                                                                                  |
| MeTube        | Downloads and queue state share the `/downloads` volume. Private-address downloads, directory indexing and per-download arbitrary yt-dlp overrides are disabled. There is no built-in account in this recipe; add proxy authentication before allowing other users to reach it. The storage action reports downloaded data usage.                                     |
| Node-RED      | Create flows in the editor. `/data` keeps flows, credentials, settings and installed nodes. Projects are enabled. Configure `adminAuth` in `settings.js` before exposing the editor; localhost has no login by default. A settings syntax action checks JavaScript syntax, not flow behavior.                                                                         |
| code-server   | Sign in with the generated `PASSWORD`. `/home/coder` persists configuration, extensions and workspace files in one volume. The editor does not receive host directories or the Docker socket. Use HTTPS and a WebSocket-capable proxy for remote access. An action lists installed extensions.                                                                        |
| Heimdall      | Add links and enable user/password protection before sharing access. `/config` persists the app and its web-server configuration. Requests to private addresses remain disabled until deliberately enabled for a trusted deployment. Only HTTP is published on loopback; terminate HTTPS at your proxy. An action validates the bundled Nginx configuration.          |

Selfhost publishes web ports on loopback by default. Complete initial account setup before exposing an app. Editing an image or host port does not automatically change an application's own public URL settings.

Pinned image tags stay on that version when updated. Floating tags track their upstream tag. Review release notes before changing a saved image version. An app's setup wizard, integrations and authentication remain part of that app.

## Configuring the new recipes

These recipes work through the CLI without starting the Selfhost dashboard. The
generated Compose, environment file and named volumes work directly with Docker
Compose too. Use the [CLI guide](cli.md) for setup, environment editing, logs and
service actions.

Grafana reads native `GF_*` environment variables. Set `GF_SERVER_ROOT_URL` when
giving it a domain. For an existing identity provider, configure Grafana's
[Generic OAuth settings](https://grafana.com/docs/grafana/latest/setup-grafana/configure-access/configure-authentication/generic-oauth/)
through its native environment configuration, including the provider's client
registration, authorization/token/user-info URLs and allowed organization or role
rules. Keep local administrator access until a separate browser login succeeds.
This recipe does not register OAuth clients automatically.

Prometheus scrape configuration belongs in a `prometheus.yml` file mounted
read-only at `/etc/prometheus/prometheus.yml` through a custom Compose setup. Keep
the `/prometheus` data volume. Its native `promtool` validation action can check
the file before a restart. Retain authentication on scrape targets; keep the query
API private or protect it at the proxy.

For Node-RED, follow its [editor security guide](https://nodered.org/docs/user-guide/runtime/securing-node-red)
to configure native `adminAuth` and protect HTTP-in nodes separately when needed.
The generated named volume keeps `settings.js` with the flows, so an ordinary
Docker Compose recreation preserves it. Adding a USB device, host mount or
additional port is a deliberate custom setup change.

### Upstream references

Images, paths and native actions are based on the application's own installation
documentation. These are Selfhost-authored recipes, not imported installer scripts.

- [Grafana Docker configuration](https://grafana.com/docs/grafana/latest/setup-grafana/configure-docker/)
  and [database configuration](https://grafana.com/docs/grafana/latest/setup-grafana/configure-grafana/#database)
- [Prometheus Docker installation](https://prometheus.io/docs/prometheus/latest/installation/)
  and [promtool commands](https://prometheus.io/docs/prometheus/latest/command-line/promtool/)
- [Tautulli Docker installation](https://github.com/Tautulli/Tautulli/wiki/Installation#docker)
- [MeTube storage and environment options](https://github.com/alexta69/metube#readme)
- [Node-RED Docker installation](https://nodered.org/docs/getting-started/docker)
- [code-server Docker installation](https://coder.com/docs/code-server/install#docker)
- [Heimdall image configuration](https://docs.linuxserver.io/images/docker-heimdall/)

The new recipes use explicit release tags. Their installation, declared read-only
actions and volume persistence were checked with Linux containers on an x86-64
Docker engine. Generated Grafana and code-server passwords were verified after
container recreation. Real Plex connections, downloaded media, automation flows,
external PostgreSQL and end-to-end OAuth sign-in still need validation in the
intended deployment. Other container architectures have not been exercised.

## Adding recipes

See [CONTRIBUTING.md](contributing.md). Verify the image publisher, container port and every required persistent path against the app's installation instructions. Do not squeeze an app needing several services, volumes or privileged host access into a single-volume recipe. Such apps require an appropriate declarative recipe extension or a custom Compose setup first.
