# Productivity and content profiles

These profiles are owned Selfhost configuration, built around each application's native installation, configuration and management interfaces. The catalogue discovery starting points were [DockerComposeMaker](https://compose.ajnart.dev/) and [awesome-selfhosted](https://github.com/awesome-selfhosted/awesome-selfhosted). Deployment definitions are portable Compose, private environment files and, where needed, ordinary files in `files/`.

## Capability coverage

| App and pinned version | Initial setup                                                                                           | Database                                                                        | Identity                                                                                                  | Native actions and settings                                                                                                                  |
| ---------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Miniflux 2.3.3         | Database migrations and selected first administrator with generated password                            | Bundled PostgreSQL; shared/external PostgreSQL mapping                          | Existing OIDC client, discovery and callback; local login preserved                                       | Health, version; polling, metrics, workers, proxy and feed settings                                                                          |
| Wallabag 2.6.14        | Image installs schema; explicit action resets initial account password before exposure                  | SQLite with separate article-image persistence; manual native database settings | No native OIDC contract in this version                                                                   | Create user, promote admin, password change, cache clear, migrations, command listing; SMTP, registration, database and Redis settings       |
| Wiki.js 2.5.315        | Native first-run API creates selected administrator and site URL                                        | Bundled PostgreSQL; PostgreSQL/MySQL/MariaDB bindings                           | Reviewed existing-client OIDC action preserves all existing strategies and local login                    | Inspect identity strategies, add OIDC strategy, version; database environment                                                                |
| BookStack v26.09       | Portable init hook replaces image's initial administrator with selected identity and generated password | Bundled MariaDB; MariaDB/MySQL bindings on private networks                     | OIDC credentials staged, explicit administrator subject mapping and reviewed authentication-method switch | Admin creation, external admin creation, URL migration, search/permissions/reference rebuild; SMTP, groups and native environment            |
| Paperless-ngx 3.2.1    | Native first administrator; PostgreSQL, Redis and English OCR configured                                | Bundled PostgreSQL; shared/external PostgreSQL mapping                          | Structured django-allauth OIDC configuration; regular login retained                                      | Document export, sanity check, index, classifier, renamer, thumbnails; OCR, consumption, SMTP, Redis, optional conversion services           |
| n8n 2.40.7             | Native owner setup API                                                                                  | SQLite or PostgreSQL deployment; shared/external PostgreSQL mapping             | SSO depends on upstream edition/license; no implied free OIDC                                             | Workflow listing, workflow and encrypted credential exports, version; editor/webhook, proxy, SMTP, execution retention and database settings |

No application branch was added to the Rust engine for these profiles. Profiles, native command argv, setup HTTP steps and bindings live in catalogue data. Wiki.js needs a small app-specific JavaScript helper because its native authentication mutation replaces the entire strategy collection. That helper is a portable mounted file, and the smoke test verifies its inline stack copy matches its canonical catalogue source.

## CLI and dashboard use

Each app's generated website guide lists its deployment methods, input fields, actions and both dashboard and CLI steps. `selfhost app --directory ./my-app init APP` uses the declared default method. Dependencyful applications select their complete stack. n8n also offers a PostgreSQL method.

Use the dashboard's app settings or the CLI native `plan` and `apply` commands to review configuration changes. Environment changes take effect when the container is recreated. Existing volumes remain separate. Use `setup-plan` and `setup-apply` for first-owner workflows on Wiki.js and n8n. Native actions use `app action SERVICE ACTION --inputs FILE` when input values are required.

A direct CLI setup does not require the Selfhost server, dashboard or ongoing management. Keep the generated Compose file, environment and files directory; ordinary Docker Compose can continue running it. Back up database and application volumes as well as configuration.

## Identity and access boundaries

- Miniflux keeps local login available and does not grant new OIDC users administrative rights.
- Wiki.js inspection returns a revision of existing identity configuration. Adding a new OIDC provider requires that revision and a new provider ID, preserves current strategies, and disables automatic registration/group assignment. Use Wiki.js administration for MFA-protected local accounts. Avoid simultaneous provider edits: upstream offers no atomic conditional mutation.
- BookStack uses OIDC as its primary authentication method. The connection operation stages credentials only. Map a verified external subject to an administrator first, then explicitly switch `AUTH_METHOD`. Revert that environment setting through Compose if necessary. A new client configuration is not proof that a browser login works.
- Paperless receives structurally encoded provider JSON instead of string interpolation. Existing local login remains available. Review native social account linking and roles before exposure.
- The initial Wallabag account must have its password changed before public routing. This requirement is shown in the deployment acknowledgement and guide. The action invokes its supported account-management command.
- n8n community operation does not imply access to licensed SSO. Selfhost does not bypass application licensing.

## Database and domain changes

Database source mappings cover the declared engines only. BookStack bindings explicitly allow only TLS mode disable and reject sources that require TLS; use a private network or configure a certificate-aware custom deployment manually. Wiki.js enables certificate verification whenever TLS is enabled. Paperless v3 uses driver options rather than the removed DBSSLMODE variables. Alternate-engine mappings are based on native documented settings; the default stacks receive the full live acceptance test.

A connection change never transfers existing database data. Export and migrate data using the app's documented procedure before pointing an installation at a different database. Rebinding a generated stack does not remove its former database or volumes automatically.

BookStack requires both `APP_URL` changes and a native stored-content URL migration. Other profiles expose their native canonical URL settings. Reverse-proxy routes and identity-provider redirect registrations need corresponding updates. Keep n8n secure cookies enabled once HTTPS is used and configure only the actual number of trusted proxy hops.

Preserve encryption secrets, especially BookStack `APP_KEY` and n8n `N8N_ENCRYPTION_KEY`. Paperless document exports and n8n workflow exports aid portability but are not full database/configuration backups.

## Upstream contracts

- [Miniflux configuration](https://miniflux.app/docs/configuration.html), [command line](https://miniflux.app/docs/cli.html), [API](https://miniflux.app/docs/api.html).
- [Wallabag Docker image](https://github.com/wallabag/docker/tree/2.6.14), [entrypoint](https://github.com/wallabag/docker/blob/2.6.14/root/entrypoint.sh), [Wallabag documentation](https://doc.wallabag.org/).
- [Wiki.js 2.5.315 setup implementation](https://github.com/Requarks/wiki/blob/v2.5.315/server/setup.js), [authentication schema](https://github.com/Requarks/wiki/blob/v2.5.315/server/graph/schemas/authentication.graphql), [resolver](https://github.com/Requarks/wiki/blob/v2.5.315/server/graph/resolvers/authentication.js), [OIDC strategy](https://github.com/Requarks/wiki/blob/v2.5.315/server/modules/authentication/oidc/definition.yml).
- [BookStack commands](https://www.bookstackapp.com/docs/admin/commands/), [OIDC authentication](https://www.bookstackapp.com/docs/admin/oidc-auth/), [LinuxServer image configuration](https://docs.linuxserver.io/images/docker-bookstack/).
- [Paperless configuration](https://docs.paperless-ngx.com/configuration/), [administration](https://docs.paperless-ngx.com/administration/), [REST API](https://docs.paperless-ngx.com/api/).
- [n8n Docker documentation source](https://github.com/n8n-io/n8n-docs/blob/main/docs/deploy/host-n8n/install-options/install-using-docker-compose.md), [owner setup implementation](https://github.com/n8n-io/n8n/blob/n8n%402.40.7/packages/cli/src/controllers/owner.controller.ts), [CLI commands](https://docs.n8n.io/hosting/cli-commands/).

## Verification

Run the opt-in local Docker acceptance suite:

```sh
node scripts/smoke-productivity.mjs ./path/to/selfhost --render-only
node scripts/smoke-productivity.mjs ./path/to/selfhost miniflux wallabag wiki-js bookstack paperless-ngx n8n
```

The suite creates random owned Compose projects and loopback ports, checks that dependencies publish no ports, and cleans up only its own containers and volumes. Live checks cover selected administrator authentication, native actions, document processing, effective environment settings and persistent volume identity across container recreation. Failures are recorded only in ignored `.local/` diagnostics. External identity-provider browser flows are not exercised by this local suite.

The live suite passed the six default apps and n8n PostgreSQL. Paperless explicitly checks Django reports PostgreSQL, consumes a PDF and verifies its native export manifest. On Docker Desktop hosts without file sharing, the suite seeds the exact portable helper files into an isolated test volume; normal bind mounts are checked during rendering. Identity-provider browser round trips and alternate MySQL/MariaDB app migrations require separately configured test providers and databases.

## Database bindings for existing catalogue apps

The same declarative engine selector now covers Nextcloud (single-image deployment), Gitea, Forgejo, Grafana, Gotify, Homarr, Vaultwarden and Memos. Existing PostgreSQL settings are retained. MySQL and MariaDB use separate native mappings, rather than substituting a different image behind PostgreSQL settings.

| App             | Native MySQL/MariaDB mapping                                                                        | TLS policy                                              |
| --------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| Nextcloud       | MYSQL_HOST, MYSQL_DATABASE, MYSQL_USER, MYSQL_PASSWORD; opposite-engine bootstrap variables removed | Private network, disable only; PostgreSQL TLS preserved |
| Gitea / Forgejo | Native database section environment, DB_TYPE=mysql                                                  | Boolean TLS verifies server certificate                 |
| Grafana         | GF_DATABASE_TYPE=mysql and native host/name/user/password                                           | Boolean TLS verifies server certificate                 |
| Gotify          | GOTIFY_DATABASE_DIALECT=mysql with Go driver DSN; PostgreSQL URI also available                     | Go driver verified TLS                                  |
| Homarr          | DB_DRIVER=mysql2, DB_DIALECT=mysql, DB_URL                                                          | Private network, disable only; PostgreSQL TLS preserved |
| Vaultwarden     | Native mysql:// DATABASE_URL with explicit ssl_mode=disabled                                        | Private network, disable only; PostgreSQL TLS preserved |
| Memos           | MEMOS_DRIVER=mysql with native Go driver DSN                                                        | Go driver verified TLS                                  |

Go DSNs retain raw passwords as required by the driver, validate usernames, escape the database path and bracket IPv6 hosts. They are separate from percent-encoded URL connection strings. For adapters using verified boolean TLS, `require` is implemented more strictly: the server must have a trusted certificate. Unsupported TLS modes fail configuration rather than silently weakening the source's requirement. These mappings are source-audited; they are not a claim that every application/engine/TLS combination has been exercised live.

Contracts: [Nextcloud image variables](https://github.com/nextcloud/docker#auto-configuration-via-environment-variables), [Gitea database configuration](https://docs.gitea.com/administration/config-cheat-sheet/#database-database), [Forgejo database configuration](https://forgejo.org/docs/latest/admin/config-cheat-sheet/#database-database), [Grafana database configuration](https://grafana.com/docs/grafana/latest/setup-grafana/configure-grafana/#database), [Gotify configuration](https://gotify.net/docs/config), [Homarr MySQL driver](https://github.com/homarr-labs/homarr/blob/v1.77.2/packages/core/src/infrastructure/db/drivers/mysql.ts), [Vaultwarden MariaDB backend](https://github.com/dani-garcia/vaultwarden/wiki/Using-the-MariaDB-%28MySQL%29-Backend), [Memos MySQL driver](https://github.com/usememos/memos/blob/v0.31.0/store/db/mysql/mysql.go), [Go MySQL DSN format](https://github.com/go-sql-driver/mysql#dsn-data-source-name).

The final n8n PostgreSQL acceptance check also created a synthetic credential, verified the native credential export remains encrypted, and checked the workflow exists in the PostgreSQL table. BookStack and n8n version records intentionally omit automated release discovery because their release tag formats do not fit the current numeric-stream adapter. Version selection remains available through explicit tested images and reviewed overrides.

A separate opt-in consumer test exercises the actual Selfhost database binding flow:

```sh
node scripts/smoke-database-consumer.mjs ./path/to/selfhost mysql
```

Gotify 3.1.1 with a dedicated MySQL database passed generated Go DSN binding, native administrator authentication, a query of the real MySQL users table using the limited application account, scoped grants and persistent recreation. The fixture's containers, network and volumes were removed after the test. This consumer proof does not extend to every alternate engine or TLS handshake.
