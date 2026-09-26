//! Portable engine-specific database archives, kept in the private project store.
use crate::core::{Store, atomic_write, now, token};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};

#[derive(Serialize, Deserialize)]
pub struct Backup {
    #[serde(default = "crate::database::default_engine")]
    pub engine: String,
    #[serde(default = "postgres_format")]
    pub format: String,
    #[serde(default)]
    pub auth_database: String,
    pub id: String,
    pub project_id: String,
    pub server_id: String,
    pub created_at: u64,
    pub bytes: u64,
    pub sha256: String,
    pub database: String,
    pub host: String,
    pub port: String,
}
fn postgres_format() -> String {
    "postgres-custom".into()
}
fn archive_format(engine: &str) -> Result<&'static str> {
    match engine {
        "postgres" => Ok("postgres-custom"),
        "mysql" | "mariadb" => Ok("sql"),
        "mongodb" => Ok("mongodb-archive"),
        _ => anyhow::bail!("Unsupported database engine"),
    }
}
pub(crate) fn archive_client(
    connection: &crate::database::Connection,
    restore: bool,
) -> Result<crate::database::ClientCommand> {
    match connection.engine.as_str() {
        "postgres" => crate::database::postgres_client(
            connection,
            if restore {
                vec![
                    "pg_restore".into(),
                    "--dbname".into(),
                    connection.database.clone(),
                    "--clean".into(),
                    "--if-exists".into(),
                    "--single-transaction".into(),
                    "--exit-on-error".into(),
                    "--no-owner".into(),
                    "--no-privileges".into(),
                ]
            } else {
                vec![
                    "pg_dump".into(),
                    "--format=custom".into(),
                    "--no-owner".into(),
                    "--no-privileges".into(),
                ]
            },
            None,
        ),
        "mysql" | "mariadb" => crate::database_mysql::client(
            connection,
            if restore { "restore" } else { "dump" },
            None,
        ),
        "mongodb" => Ok(crate::database::ClientCommand {
            image: crate::catalog::database_driver("mongodb")?["image"]
                .as_str()
                .context("Missing database image")?
                .into(),
            argv: crate::database_mongo::archive_command(restore, &connection.database)?,
            environment: std::collections::BTreeMap::from([(
                "SELFHOST_MONGO_CONFIG".into(),
                crate::database_mongo::archive_config(connection)?,
            )]),
            input: None,
        }),
        _ => anyhow::bail!("Unsupported database engine"),
    }
}
fn digest(file: &mut fs::File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(format!("{:x}", hash.finalize()))
}
impl Store {
    fn backup_dir(&self, id: &str) -> Result<PathBuf> {
        self.project(id)?;
        let dir = self.project_dir(id)?.join("database-backups");
        crate::core::private_dir(&dir)?;
        Ok(dir)
    }
    fn backup_record(&self, id: &str, backup: &str) -> Result<Backup> {
        ensure!(
            backup.starts_with("b-") && crate::setup::slug(backup),
            "Invalid backup id"
        );
        let record: Backup = serde_json::from_slice(&fs::read(
            self.backup_dir(id)?.join(format!("{backup}.json")),
        )?)?;
        ensure!(
            record.id == backup && record.project_id == id,
            "Backup does not belong to this project"
        );
        Ok(record)
    }
    pub fn database_backups(&self, id: &str) -> Result<Vec<Backup>> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.backup_dir(id)?)? {
            let path = entry?.path();
            if path.extension().is_some_and(|v| v == "json") {
                let name = path
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .context("Invalid backup filename")?;
                records.push(self.backup_record(id, name)?);
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        Ok(records)
    }
    async fn database_archive_command(
        &self,
        id: &str,
        restore: bool,
        input: Option<fs::File>,
        output: Option<fs::File>,
    ) -> Result<()> {
        let project = self.project(id)?;
        let setup = self.setup(id)?;
        let binding = setup.database.as_ref().context("No database is attached")?;
        ensure!(binding.provisioned, "Provision this database first");
        let mut connection = crate::database::setup_connection(&setup)?;
        let mut network = None;
        if binding.mode == "dedicated" {
            let container = self
                .service_container(id, "selfhost-database")
                .await?
                .context("Start the database container first")?;
            network = Some(format!("container:{container}"));
            connection.host = "127.0.0.1".into();
        } else if let Some(source) = &binding.source_id {
            network = self.database_source(source)?.network;
        }
        let name = format!("selfhost-archive-{}", token(8)?);
        let mut command = self.project_docker(&project, true)?;
        command.args(["run", "--rm", "-i", "--name", &name]);
        if let Some(network) = network {
            command.args(["--network", &network]);
        }
        let client = archive_client(&connection, restore)?;
        for (key, val) in &client.environment {
            command.args(["--env", key]);
            command.env(key, val);
        }
        command.arg(&client.image).args(&client.argv);
        command.stdin(input.map(Stdio::from).unwrap_or_else(Stdio::null));
        command.stdout(output.map(Stdio::from).unwrap_or_else(Stdio::null));
        // Error output can include database data and credentials; do not persist it.
        command.stderr(Stdio::null()).kill_on_drop(true);
        let result=async {
            let mut child=command.spawn()?;
            let status=tokio::time::timeout(Duration::from_secs(600),child.wait()).await.context("Database operation timed out")??;
            ensure!(status.success(),"Database operation failed. Check database availability, credentials, permissions and database client compatibility.");
            Ok::<_,anyhow::Error>(())
        }.await;
        let mut cleanup = self.project_docker(&project, true)?;
        cleanup.args(["rm", "-f", &name]);
        let _ = crate::core::run(cleanup, 15).await;
        result
    }
    pub(crate) async fn backup_database_locked(&self, id: &str) -> Result<Backup> {
        let dir = self.backup_dir(id)?;
        let setup = self.setup(id)?;
        let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
        self.database_archive_command(id, false, None, Some(temp.reopen()?))
            .await?;
        temp.as_file_mut().sync_all()?;
        let bytes = temp.as_file().metadata()?.len();
        ensure!(bytes > 5, "Database archive is empty");
        let binding = setup.database.as_ref().context("No database attached")?;
        let record = Backup {
            engine: binding.engine.clone(),
            format: archive_format(&binding.engine)?.into(),
            auth_database: setup
                .environment
                .get("DATABASE_AUTH_DATABASE")
                .cloned()
                .unwrap_or_default(),
            id: format!("b-{}", token(8)?),
            project_id: id.into(),
            server_id: self.project(id)?.server_id,
            created_at: now(),
            bytes,
            sha256: digest(temp.as_file_mut())?,
            database: setup.environment["DATABASE_NAME"].clone(),
            host: setup.environment["DATABASE_HOST"].clone(),
            port: setup.environment["DATABASE_PORT"].clone(),
        };
        temp.persist_noclobber(dir.join(format!("{}.dump", record.id)))?;
        atomic_write(
            &dir.join(format!("{}.json", record.id)),
            &serde_json::to_vec_pretty(&record)?,
        )?;
        Ok(record)
    }
    pub fn database_backup_file(&self, id: &str, backup: &str) -> Result<fs::File> {
        let record = self.backup_record(id, backup)?;
        let mut file = fs::File::open(self.backup_dir(id)?.join(format!("{backup}.dump")))?;
        ensure!(
            file.metadata()?.len() == record.bytes && digest(&mut file)? == record.sha256,
            "Backup checksum mismatch"
        );
        Ok(file)
    }
    pub fn export_database_backup(
        &self,
        id: &str,
        backup: &str,
        path: &std::path::Path,
    ) -> Result<()> {
        let mut input = self.database_backup_file(id, backup)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(&mut input, output.as_file_mut())?;
        output.as_file_mut().sync_all()?;
        output.persist_noclobber(path)?;
        Ok(())
    }
    pub async fn backup_database(&self, id: &str) -> Result<Backup> {
        let _lock = self.project_lock(id)?;
        self.backup_database_locked(id).await
    }
    pub async fn restore_database(&self, id: &str, backup: &str, confirm: &str) -> Result<Backup> {
        let _lock = self.project_lock(id)?;
        ensure!(confirm == id, "Confirm restore by entering the project id");
        let project = self.project(id)?;
        self.project_docker(&project, true)?;
        let record = self.backup_record(id, backup)?;
        let setup = self.setup(id)?;
        ensure!(
            setup
                .database
                .as_ref()
                .is_some_and(|b| b.engine == record.engine)
                && record.format == archive_format(&record.engine)?
                && setup
                    .environment
                    .get("DATABASE_AUTH_DATABASE")
                    .cloned()
                    .unwrap_or_default()
                    == record.auth_database
                && project.server_id == record.server_id
                && setup.environment.get("DATABASE_NAME") == Some(&record.database)
                && setup.environment.get("DATABASE_HOST") == Some(&record.host)
                && setup.environment.get("DATABASE_PORT") == Some(&record.port),
            "Database destination changed since this backup; restore manually after reviewing the target"
        );
        let mut running = self.project_docker(&project, false)?;
        running.args([
            "ps",
            "--filter",
            &format!("label=com.docker.compose.project=selfhost-{id}"),
            "--format",
            "{{.Label \"com.docker.compose.service\"}}",
        ]);
        let active = crate::core::run(running, 15).await?;
        ensure!(
            active.lines().all(|line| line == "selfhost-database"),
            "Stop this project's app containers before restoring. Keep its database running."
        );
        let mut file = fs::File::open(self.backup_dir(id)?.join(format!("{backup}.dump")))?;
        ensure!(
            digest(&mut file)? == record.sha256,
            "Backup checksum mismatch; restore refused"
        );
        let safety = self.backup_database_locked(id).await?;
        self.database_archive_command(id, true, Some(file), None)
            .await
            .with_context(|| format!("Restore failed. Safety backup retained: {}", safety.id))?;
        Ok(safety)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_export_verifies_identity_checksum_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Backup test".into(),
                server_id: "local".into(),
                apps: vec!["homarr".into()],
            })
            .unwrap();
        let root = store.backup_dir(&project.id).unwrap();
        let id = "b-01234567";
        let path = root.join(format!("{id}.dump"));
        fs::write(&path, b"synthetic archive").unwrap();
        let record = Backup {
            engine: "postgres".into(),
            format: postgres_format(),
            auth_database: String::new(),
            id: id.into(),
            project_id: project.id.clone(),
            server_id: "local".into(),
            created_at: 1,
            bytes: 17,
            sha256: digest(&mut fs::File::open(&path).unwrap()).unwrap(),
            database: "test".into(),
            host: "test".into(),
            port: "5432".into(),
        };
        fs::write(
            root.join(format!("{id}.json")),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let output = dir.path().join("export.dump");
        store
            .export_database_backup(&project.id, id, &output)
            .unwrap();
        assert!(
            store
                .export_database_backup(&project.id, id, &output)
                .is_err()
        );
        assert!(
            store
                .database_backup_file(&project.id, "../b-01234567")
                .is_err()
        );
        fs::write(&path, b"modified archive!").unwrap();
        assert!(store.database_backup_file(&project.id, id).is_err());
        assert_eq!(fs::read(output).unwrap(), b"synthetic archive");
    }
}
