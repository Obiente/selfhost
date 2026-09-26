# MongoDB sources and project isolation

MongoDB uses a separate authentication database (`authSource`) from the application database. Enter the existing account's authentication database when connecting an external source. A managed shared source creates its administrator in `admin`; provisioned project users authenticate against their own project database.

Dedicated deployments create an independent root password and a separate application account. Applications receive `readWrite` and `dbAdmin` on their own database, not a server-wide administrator. Managed shared provisioning uses the same scoped roles. It refuses to adopt a populated database or modify a pre-existing user without the exact Selfhost ownership marker and scoped roles.

Managed servers publish no host port by default. Their private Docker network is the connection boundary. External MongoDB supports explicit TLS: both `require` and `verify-full` enable certificate and hostname verification, while `disable` is an explicit unencrypted connection. No invalid-certificate bypass is generated. The current connection form describes one host and port; SRV discovery, replica-set topology configuration and custom CA mounts need additional configuration.

New managed servers use the supported `mongo:7.0.43` image. MongoDB 8.0.32 refused to start on the local Docker kernel because of its upstream allocator compatibility guard. [MongoDB's release notes](https://www.mongodb.com/docs/manual/release-notes/8.0/) describe the affected Linux 6.19 through 7.0.13 range and the kernel fix. Selfhost does not bypass that guard or change the host kernel. Never point a MongoDB 7 container at an existing MongoDB 8 data directory to work around this issue.

## Backups and restore

The native database tools produce a binary `mongodump` archive and restore through `mongorestore`. Secrets are passed in a private environment value, written to a mode 0600 temporary configuration file inside the disposable client, then removed. Credentials are never appended to command arguments. The application database is explicit and restore is restricted to its namespace.

Restore uses `--drop` for collections represented by the archive and stops on errors. It is not transactional. Stop application writes before taking a consistency-sensitive backup or restoring one. A single-database archive does not capture a cluster-wide oplog, other project databases or server administrator accounts. Match MongoDB major version and feature compatibility when restoring. Keep the independent volume and media backup requirements of each application in mind.

The MongoDB helper's opt-in local acceptance test starts an isolated authenticated container, checks scoped initialization and readiness, provisions a second isolated project, rejects cross-project access and populated-database adoption, then exercises dump, restore and persistence:

```sh
cargo test database_mongo::live_tests::mongo_scoped_init_provision_archive_restore_live -- --ignored --nocapture
```

It uses a random fixture label and only removes its own verified container and volume. Remote Docker contexts are rejected.

The scoped initialization, ownership, cross-project denial, archive roundtrip and restart checks passed against `mongo:7.0.43` on the local Docker engine. External TLS, remote Docker connections and replica-set behavior were not part of this fixture.

## References

- [Official Mongo Docker image initialization](https://hub.docker.com/_/mongo)
- [MongoDB user creation and roles](https://www.mongodb.com/docs/manual/reference/method/db.createUser/)
- [mongodump configuration and consistency](https://www.mongodb.com/docs/database-tools/mongodump/)
- [mongorestore options](https://www.mongodb.com/docs/database-tools/mongorestore/)
