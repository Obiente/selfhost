# Media, monitoring and dashboard profiles

These recipes generate ordinary Compose projects and native application configuration. They work through `selfhost app` without the Selfhost dashboard or a persistent Selfhost process. Use `selfhost app --directory ./my-app init APP`, review its generated files, then start it. Native deployments are selected automatically where required. Dozzle additionally requires explicit `--ack docker-socket`.

## Coverage

| App            | Native settings and storage                                                                                                      | Initial setup                                              | Actions and connections                                                                       |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Navidrome      | YAML configuration, scanning, transcoding, metadata providers, backups, proxy authentication; persistent data and separate music | First administrator through the native setup API           | Version, account listing, confirmed library scan; external authentication proxy configuration |
| Audiobookshelf | Supported environment settings; config and metadata beneath `/data`                                                              | First root account through the native setup API            | Local health check; runtime libraries, OIDC and API tokens remain managed in Audiobookshelf   |
| Kavita         | Timezone; persistent `/kavita/config`                                                                                            | First administrator and email through the native setup API | API health check; runtime server settings and library creation remain managed in Kavita       |
| Gatus          | Native YAML endpoints, alerting, UI, storage, metrics, basic authentication and OIDC                                             | Starts with an internal health endpoint and SQLite         | Manual OIDC client settings and optional PostgreSQL connection in native configuration        |
| Dozzle         | Supported native environment options; explicit Docker socket acknowledgement                                                     | Defaults disable shell, container actions and MCP          | Health check; OIDC issuer/client mapping, remote host and agent settings                      |
| Glances        | Native command options and timezone; isolated web process                                                                        | No privileged host resources are added                     | Version check; full host monitoring requires deliberate PID, device or socket configuration   |
| Homepage       | Native services YAML root array; portable settings, bookmarks and widgets files                                                  | Empty dashboard, exact local host allowlist                | Supported service widgets through native structures; authenticated reverse proxy required     |
| Dashy          | Native page, layout, status, authentication, sections and pages configuration                                                    | Portable read-only configuration                           | Health check; manual native Keycloak settings, authenticated proxy recommended                |

Structured JSON fields are rendered into the application's native YAML structure. They are not strings containing JSON. Application documentation determines supported nested keys. Sensitive structured configuration, including widget tokens, should be treated as secrets even when the editor must display the structure to its authorized operator.

## Safe onboarding

Navidrome, Audiobookshelf and Kavita bootstrap profiles only accept a fresh application. Review the setup plan before applying it. Existing installations are not reset and their administrator accounts are not replaced. Passwords are provided in private request files, omitted from plan output, and are not stored in the onboarding receipt. Delete private request files after use.

Media mounts are independent from application data. Point applications at your own explicit library mounts; Selfhost does not import, move or delete the original media. Back up application data and media according to their separate recovery requirements.

## Authentication and networks

The generated listeners bind to loopback. Configure HTTPS and authentication before exposing private applications. A remote reverse proxy needs a deliberately reachable private network path; a localhost URL on another machine will not work.

Dozzle's OIDC integration uses `/api/auth/callback`. Configure provider roles and container filters according to Dozzle's documentation. OIDC replaces its local authentication flow. The Docker socket grants broad Docker API access even when mounted read-only.

Navidrome external authentication trusts a configured upstream identity header. The proxy must overwrite that header and direct untrusted access must be blocked. Gatus supports native OIDC fields, but this profile does not automatically register its provider client. Homepage has no built-in authentication. Dashy's browser authentication is not a replacement for protecting its HTTP resources through an authenticated proxy.

## Explicit remaining boundaries

- Audiobookshelf and Kavita runtime library management, provider registration and advanced API workflows are not automated yet.
- Homepage and Dashy accept their own service/widget configuration, but do not yet implement Selfhost's automatic dashboard link reconciliation contract.
- Homepage's typed profile covers `services.yaml`. Its other native files remain portable and editable through the ordinary configuration editor.
- Glances runs in an isolated container by default. It does not claim complete host statistics without the required resources.
- Changing settings writes portable configuration. Apply the application's documented restart or reload procedure where needed.
- Kavita's available Docker image is tagged `0.9.1`; upstream patch release names do not necessarily have corresponding image tags. Image selection remains explicit.

## Acceptance testing

Run the opt-in fixture suite with a local Docker engine:

```sh
node scripts/smoke-media-monitoring.mjs path/to/selfhost
```

It creates random isolated projects, validates Compose, roundtrips typed settings through the real CLI, checks HTTP startup, bootstraps supported first accounts, exercises read-only actions and recreates containers to verify volume persistence. It only removes its own checked Compose resources. Supply app IDs after the executable to run a subset.

For Hyper-V hosts without an approved shared host directory, `SELFHOST_SMOKE_VOLUME_CONFIGS=1` explicitly enables a test-only accommodation: generated file binds are validated normally, then their exact configuration bytes are copied into fixture volumes before startup. This verifies native configuration behavior and persistence, but does not verify host file sharing. It never changes Docker settings or product manifests. `SELFHOST_SMOKE_ROOT` optionally selects the fixture parent directory.

The pinned versions of all eight recipes passed this local Docker acceptance suite. The three media bootstrap flows also retained their initialized state after recreation. Version records carry this evidence and its limits; automatic version upgrades remain disabled. External identity-provider browser login, media-library ingestion, remote agents and upgrades between different application versions require their own acceptance tests.

## Upstream references

- [Navidrome installation](https://www.navidrome.org/docs/installation/docker/) and [configuration](https://www.navidrome.org/docs/usage/configuration/options/)
- [Audiobookshelf Docker installation](https://audiobookshelf.org/docs/documentation/install/docker/) and [server FAQ](https://audiobookshelf.org/docs/faq/server/)
- [Kavita Docker installation](https://wiki.kavitareader.com/installation/docker/) and [server settings](https://wiki.kavitareader.com/guides/admin-settings/general/)
- [Gatus configuration](https://github.com/TwiN/gatus)
- [Dozzle environment options](https://dozzle.dev/guide/supported-env-vars) and [OIDC](https://dozzle.dev/guide/authentication/oidc)
- [Glances Docker deployment](https://glances.readthedocs.io/en/latest/docker.html)
- [Homepage installation](https://gethomepage.dev/installation/docker/), [services](https://gethomepage.dev/configs/services/) and [settings](https://gethomepage.dev/configs/settings/)
- [Dashy deployment](https://dashy.to/docs/deployment/) and [configuration](https://dashy.to/docs/configuring/)
