# Custom setups and databases

You can also create an app directly in your own directory through the
[standalone CLI](standalone.md). Workspace exports are not a prerequisite for
using Selfhost without its dashboard or ongoing project management.

## Custom setups

Choose **Custom setup** in the app catalog. Paste Compose YAML or JSON, add environment variables and configuration files, then save. Existing projects expose the same editor through their settings. Saving creates a configuration snapshot and does not restart containers. Start the project to apply the saved configuration.

Use prebuilt container images. Mount persistent data as named volumes and declared configuration files as `./files/NAME:/container/path:ro`. Setup scripts are ordinary declared files run through a container command or initialization service. No host shell script is executed by the recipe loader.

File mounts currently require the local Docker engine. On Docker Desktop, share the project or exported files directory in Settings > Resources > File Sharing before starting a setup with file mounts. Include, extends, build contexts, arbitrary host mounts and env_file are rejected so exported setups remain self-contained. Compose configuration is limited to 64 services and the complete setup document to 1 MiB.

```sh
selfhost create-setup Example --compose compose.yaml --environment environment.json --files config-files
selfhost setup PROJECT
selfhost env PROJECT --set API_KEY
selfhost config PROJECT app.yaml --file app.yaml
selfhost run PROJECT start
selfhost export PROJECT example.tar
```

`environment.json` is a JSON object of variable names and string values. `env --set` reads its value from stdin. Values are hidden by default; `--reveal` explicitly prints credentials. `setup --file setup.json` replaces the complete saved setup. Use `setup --reveal` to obtain that document privately. A custom Compose plan can itself contain inline credentials.

## Leaving selfhost

Extract the exported tar into a private directory and run:

```sh
docker compose -f compose.json up -d
```

The archive contains Compose, `.env`, declared files and instructions. No selfhost server, account or process is needed. Keep the Compose project name to reuse its named volumes on the same Docker engine. The archive includes credentials but **does not contain application or database data**. Transfer volume data separately when changing servers. External databases and networks remain external dependencies. Automated migration currently rejects custom setups; use export and an explicit data transfer.

## Engines and placement

The Databases page manages PostgreSQL, MySQL, MariaDB and MongoDB connection sources. `selfhost database-engines` lists the bundled drivers. Each app offers only engines declared by its recipe; a project containing several database adapters must choose an engine supported by all of them. A custom Compose setup can select any engine explicitly and map its connection variables itself. Project setup settings select placement before the app is started:

- **Dedicated**: a database container and persistent volume belonging to the project.
- **Shared**: a managed database server or a supplied server with database creation permissions. Provisioning creates a separate database and account per project. PostgreSQL revokes public CONNECT on that database; MySQL/MariaDB grant privileges only on that database; MongoDB grants readWrite and dbAdmin only within that database.
- **External**: an existing database and account supplied by the user. selfhost does not provision or change that database.

A managed shared server publishes no host port. Its projects join a named Docker network on the same engine. Start the database source, configure the project, then select **Create database and user**. App startup is blocked while shared provisioning is pending. Source administrator passwords are excluded from app setups and exports.

### Guided database setup

Run `selfhost database-setup` and choose a project. The terminal asks which supported engine to use and whether the database should be dedicated, shared, or external. It offers compatible saved sources or guides you through adding one. Password entry is hidden.

```sh
selfhost database-setup
selfhost database-add
selfhost database-edit
selfhost database-remove
```

For a managed shared source, the guided flow creates and starts its database container, waits for its health check, verifies its connection, and provisions the project's isolated database and user after confirmation. External sources are tested without changing their database contents. Tests run a temporary native database client on the selected Docker host, so that host needs permission to create the short-lived client container. The user does not prepare JSON or copy revisions between commands.

A cancelled project attachment can leave a newly created, explicitly confirmed source available for later use. If provisioning fails, repeat `database-setup` to resume an unstarted step. An uncertain administrative write is never repeated automatically.

`database-edit` verifies replacement connection settings before saving and keeps a private recovery copy. Referenced sources cannot be redirected to a different database or TLS policy. Shared administrator credentials can be replaced with credentials already set on the database, because project accounts are separate. External credentials used by projects cannot be changed independently of their app configurations. Editing a saved password does not rotate the server account.

`database-remove` refuses referenced sources and removes only the saved connection. Its private recovery copy, any hosted database project, containers and data remain intact. Use the project's separate removal flow to remove a hosted database after backing it up.

Automation can still provide a source file to `database-add FILE`, or use the explicit `database-attach`, `database-start` and `database-provision` operations. They are not required for interactive setup. Source fields include the engine, placement, host, port, account and TLS mode. MongoDB also uses an authentication database, normally `admin` for supplied accounts. See [MongoDB details](mongodb.md).

Setups receive DATABASE_HOST, DATABASE_PORT, DATABASE_NAME, DATABASE_USER, DATABASE_PASSWORD, DATABASE_SSL_MODE and DATABASE_AUTH_DATABASE. DATABASE_URL provides an engine-specific URI with separately percent-encoded credentials and database name. PostgreSQL includes `sslmode`; MongoDB includes `authSource` and `tls`. MySQL/MariaDB use `mysql://` without driver-specific TLS parameters. DATABASE_GO_MYSQL_DSN is generated when a MySQL/MariaDB recipe explicitly references it; it preserves the Go driver credential syntax and requires a username without a colon. DATABASE_MYSQL_SSL_MODE provides `DISABLED`, `REQUIRED` or `VERIFY_IDENTITY` for adapters that accept that enum. DATABASE_SSL_ENABLED is `false` for `disable` and `true` otherwise. DATABASE_SSL_REJECT_UNAUTHORIZED is `true` for `verify-full`; other modes do not request certificate identity verification. Recipes can map these into application-specific environment variables. Custom Compose services must reference them explicitly. Changing placement after installation requires a data migration; changing environment values alone does not migrate a database or rotate an existing database password.

Adapters declare their supported TLS modes and reject unsupported combinations instead of silently dropping encryption. MySQL requires encrypted connections for `require`; MariaDB and MongoDB clients verify server certificates for both `require` and `verify-full`. Install the appropriate trust chain when using an external certificate authority. Arbitrary CA file upload is not provided by this source form.

A failed or interrupted shared provisioning attempt is marked uncertain before the first write. Selfhost refuses to repeat it automatically. Inspect the saved project credentials and the server account/database before manual recovery. It never silently adopts an existing SQL account or resets its password on collision.

Automatic server credential rotation is not implemented. Configuration snapshots and source recovery copies are not database backups.

## Database backups and restore

Use **Back up database** in project setup settings, then **Download** to keep a copy elsewhere. Backups contain database data and must be treated as private. They are stored under the project's `database-backups` directory, independently of configuration snapshots and setup exports.

```sh
selfhost database-backup PROJECT
selfhost database-backups PROJECT
selfhost database-backup-export PROJECT BACKUP database.dump
selfhost database-restore PROJECT BACKUP --confirm-project PROJECT
```

Archives use native tools from the selected driver: PostgreSQL custom-format (`pg_dump`/`pg_restore`), SQL dumps for MySQL/MariaDB (`mysqldump`/`mysql` and `mariadb-dump`/`mariadb`), and MongoDB archive streams (`mongodump`/`mongorestore`). Driver metadata selects the client image. Use a compatible client version for external servers. Dumps cover one database, not server accounts, app uploads or other Docker volumes. There is no automatic retention policy or off-device copy.

MySQL/MariaDB dumps take table read locks and can temporarily block writes. MongoDB standalone archives do not provide a transactionally consistent cross-collection snapshot during concurrent writes; stop writers before backing up data that requires that consistency. MySQL/MariaDB and MongoDB restores are not atomic and can partially apply before a failure.

Selfhost verifies SHA-256 before download/export and restore. Restore requires the original project, engine, archive format, server and database destination and refuses to proceed while that project's app containers are running. Keep the dedicated database container running. Stop external writers yourself. A fresh safety backup must complete before restore starts.

PostgreSQL restore uses `pg_restore --clean --if-exists --single-transaction --exit-on-error --no-owner --no-privileges`. It replaces objects present in the archive; objects created afterward that are absent from the archive can remain. Ownership and grants are not replayed. Apps remain stopped after restore. Incompatible schemas, active external writers and insufficient permissions can cause restore to fail. The safety archive remains available if it fails.

Only archives created for the same project are accepted by this workflow. For cross-server or third-party archives, review the destination and archive before using the appropriate engine tools directly. SQL restores drop/recreate tables represented in the archive, and MongoDB restores are restricted to the project database namespace with `--drop`; unrelated tables/collections absent from the archive can remain. See the PostgreSQL [pg_dump](https://www.postgresql.org/docs/17/app-pgdump.html) and [pg_restore](https://www.postgresql.org/docs/17/app-pgrestore.html) documentation.

## Contributor database adapters

A recipe declares its default `database.engine` and `database.environment`. Optional `database.engines` lists alternate engines. Each alternative needs its own complete `database.engine_environment.ENGINE` mapping. These replace the default mapping. `database.engine_ssl_modes.ENGINE` lists allowed source TLS modes. Unsupported engines or TLS modes are refused before the setup is saved. Declare conservative TLS support until the actual application driver has been checked. The database driver documents under `catalog/databases` provide display names, image versions, ports, health checks and persistent data paths.
