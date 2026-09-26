# Adding a service

Add one TOML file to `catalog/`. No Rust or frontend changes are required. You can also keep private files in a separate directory and load them with `--catalog-dir`.

```toml
schema = 1
id = "example-service"
name = "Example Service"
description = "A short description of what this app does."
category = "Productivity"
image = "example/service:1.0.0"
port = 8080
container_port = 8080
data_path = "/var/lib/example"
docs = "https://example.com/docs"
icon = "https://example.com/icon.svg"

[environment]
TZ = "UTC"

[[secrets]]
environment = "ENCRYPTION_KEY"
bytes = 32
```

`port` is the preferred host port; selfhost chooses the next free project port when another project already uses it. Actual host port conflicts are reported by Docker at start. `container_port` and `data_path` come from the app's installation documentation. A named volume is mounted at `data_path`. Images must specify a tag or digest.

`secrets` generates cryptographically random bytes encoded as hexadecimal. Values are stored in the project environment file and passed under the declared environment name. They are never baked into the recipe. The platform prefixes stored variables with the app ID so separate apps can use the same environment names.

The current recipe schema covers single-container services with one data volume and one public port. Multi-container stacks, multiple volumes, dependencies, typed user inputs, and migration steps belong in the next schema revision. Do not work around these limitations with shell scripts or hardcoded application branches in the backend.

## Validate a contribution

Recipes also power the standalone CLI. Verify that a new app can be generated
without a Docker engine, started with ordinary Compose, and used after removing
Selfhost's optional helper metadata. Preserve the generated Compose name and
volume identity when testing updates. Native app configuration and credentials
must remain usable with the application's own tools.

```sh
selfhost --catalog-dir ./catalog catalog
```

From source:

```sh
npm ci --prefix ui
npm run build --prefix ui
cargo run -- --catalog-dir ./catalog catalog
cargo test
```

The opt-in `node scripts/smoke-standalone.mjs target/debug/selfhost` test uses a
local Docker engine to verify directory renaming, file and credential preservation,
CLI updates, and ordinary Compose operation without helper metadata. It creates
and removes only its own temporary app and labelled Docker resources. On Windows,
pass the platform's native executable path.

Unknown fields, duplicate IDs, invalid port values, unsafe data paths, and invalid secret declarations fail validation. To try a private manifest, create a project with its ID and inspect `selfhost plan PROJECT_ID` before starting it. Verify initial setup, persistence after a restart, and app health with the upstream's documented image.

## Optional dashboard API recipe

Apps that accept links to other apps can declare a recipe:

```toml
[dashboard]
list_path = "/api/apps"
create_path = "/api/apps"
auth_header = "ApiKey"
match_field = "href"

[dashboard.body]
name = "{name}"
description = "{description}"
iconUrl = "{icon}"
href = "{url}"
pingUrl = ""
```

The list endpoint must return a JSON array. `match_field` identifies the URL field used to skip existing links. The create body supports `{name}`, `{description}`, `{icon}`, and `{url}`. The API key is supplied when syncing and is not persisted. Calls target only the dashboard's configured port on localhost, and redirects are disabled.

This adapter adds app links, not board layouts or widgets. More complex onboarding should be introduced through versioned declarative steps with explicit validation, retries, and idempotency rules.

## Repository hygiene

Keep runtime data, secrets, diagnostics, captures, and personal paths outside the repository. Use synthetic project names in tests. Inspect staged files before committing. Use conventional branch prefixes such as `feat/`, `fix/`, or `docs/`.

## Service stats, logs, and actions

Every installed service exposes current Docker CPU, memory, network I/O, block I/O, and process/thread counts. Logs show the latest 200 lines and refresh every five seconds while that view is open. Sampling is live, not historical monitoring. Log streams are capped to 256 KiB each and displayed as plain text; stdout and stderr are grouped separately. Logs and custom action output are not stored in activity history.

Start, stop, restart, and update are available per service. Update pulls the configured tag, saves a configuration snapshot, and recreates only that service as needed. It does not choose a new version when a tag is pinned.

Add service-specific commands to its TOML recipe:

```toml
[[actions]]
id = "data-usage"
label = "Data usage"
description = "Show the size of stored application data."
command = ["du", "-sh", "/var/lib/example"]
timeout_seconds = 15
confirm = false
```

`command` is an executable and argument array passed directly to `docker exec` inside the selected service container. No host shell is invoked and arguments are not interpolated. Contributors should use commands present in the documented image, explain their effects, and set `confirm = true` for actions that modify data. Review third-party recipes before installing them: a declared command has the same access to the service's data as its container user. Do not put secrets in command arguments.

IDs must be unique; `start`, `stop`, `restart`, and `update` are reserved. Timeouts are 1 to 300 seconds, default 30. A timeout stops waiting for Docker; an already-started command inside the container can continue. Check logs and application state before retrying a timed-out action.

Service definitions, including actions, are saved when a project is created. Editing the catalog does not silently change installed projects. The CLI exposes `selfhost stats PROJECT SERVICE`, `selfhost logs PROJECT SERVICE`, and `selfhost service PROJECT SERVICE ACTION`. Invoking a CLI action is an explicit request to execute it; the dashboard presents its description and an action button before running it.

## Commands and database mappings

For typed native settings, setup workflows, scheduled app actions, and identity-provider connections, see [native app integrations](docs/app-integrations.md). Database mappings can additionally declare `unset_environment` to remove conflicting bootstrap settings when a database is attached.

All bundled recipes are authored for selfhost. Research upstream installation instructions, but contribute a native manifest rather than embedding another catalog or linking to its installer. Container paths, ports and commands belong in the manifest.

An optional top-level `command = ["serve"]` overrides the image command. Custom setups support multiple services and declared setup-script files; see [custom setups](docs/setups-and-databases.md).

For apps that support PostgreSQL, declare the application's environment mapping:

```toml
[database]
engine = "postgres"

[database.environment]
APP_DB_HOST = "${DATABASE_HOST}"
APP_DB_PORT = "${DATABASE_PORT}"
APP_DB_NAME = "${DATABASE_NAME}"
APP_DB_USER = "${DATABASE_USER}"
APP_DB_PASSWORD = "${DATABASE_PASSWORD}"
```

Use the app's actual documented variable names and TLS settings. These mappings are applied only when the user attaches a database. Dedicated databases also add a healthy-service dependency. Database image, data path and health check are declared in `catalog/databases/postgres.json`. The PostgreSQL provisioning adapter currently requires Rust changes to support another database engine.

Networking adapters and identity stacks are covered in [Networking and login](docs/networking-and-login.md). Provider-specific paths, fields, certificate settings, and Compose belong in declarative catalog profiles.

## Formatting

Install the repository's formatter tools once with `npm ci --ignore-scripts`.
Run `npm run format` to format the repository, or `npm run format:check` in CI.

- Rust uses the toolchain's own `cargo fmt` / rustfmt. No third-party Rust formatter is used.
- Vue, JavaScript, TypeScript, CSS, HTML, Markdown, JSON and YAML use pinned Prettier.
- TOML manifests use pinned Taplo because Cargo has no TOML formatting command.
- Cargo.lock and npm lockfiles stay owned by their respective package managers.
- Generated output, dependencies, local state, private captures and vendored license text are excluded.

Editor defaults are in `.editorconfig`; formatter configuration is versioned in
`.prettierrc.json` and `taplo.toml`. Release preparation checks formatting before
building packages. Do not hand-format generated CLI reference text; regenerate it,
then run the repository formatter.
