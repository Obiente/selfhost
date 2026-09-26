//! Database source ownership and project isolation. Administrative credentials
//! stay in the private source store and are never exported with client projects.
use crate::{
    core::{Store, atomic_write, token},
    setup::Setup,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, time::Duration};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default)]
    pub provisioning_uncertain: bool,
    pub mode: String,
    pub source_id: Option<String>,
    pub provisioned: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default)]
    pub auth_database: String,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl_mode: String,
    pub network: Option<String>,
    pub managed_project: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewSource {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default)]
    pub auth_database: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub managed: bool,
    #[serde(default = "crate::infrastructure::local_id")]
    pub server_id: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub database: String,
    #[serde(default = "ssl_default")]
    pub ssl_mode: String,
}
pub(crate) fn default_engine() -> String {
    "postgres".into()
}
pub(crate) struct Connection {
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl_mode: String,
    pub auth_database: String,
}
pub(crate) struct ClientCommand {
    pub image: String,
    pub argv: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub input: Option<Vec<u8>>,
}
impl Source {
    fn connection(&self) -> Connection {
        Connection {
            engine: self.engine.clone(),
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            password: self.password.clone(),
            database: self.database.clone(),
            ssl_mode: self.ssl_mode.clone(),
            auth_database: self.auth_database.clone(),
        }
    }
}
pub(crate) fn setup_connection(setup: &Setup) -> Result<Connection> {
    let binding = setup.database.as_ref().context("No database attached")?;
    let value = |key: &str| {
        setup
            .environment
            .get(key)
            .cloned()
            .context("Missing database connection value")
    };
    Ok(Connection {
        engine: binding.engine.clone(),
        host: value("DATABASE_HOST")?,
        port: value("DATABASE_PORT")?.parse()?,
        username: value("DATABASE_USER")?,
        password: value("DATABASE_PASSWORD")?,
        database: value("DATABASE_NAME")?,
        ssl_mode: value("DATABASE_SSL_MODE")?,
        auth_database: setup
            .environment
            .get("DATABASE_AUTH_DATABASE")
            .cloned()
            .unwrap_or_default(),
    })
}
fn ssl_default() -> String {
    "require".into()
}

fn postgres_uri(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
    ssl: &str,
) -> Result<String> {
    fn component(value: &str) -> String {
        value
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }
    let mut uri = reqwest::Url::parse("postgresql://localhost")?;
    uri.set_host(Some(host))
        .map_err(|_| anyhow::anyhow!("Invalid database host"))?;
    uri.set_port(Some(port))
        .map_err(|_| anyhow::anyhow!("Invalid database port"))?;
    uri.set_username(&component(user))
        .map_err(|_| anyhow::anyhow!("Invalid database user"))?;
    uri.set_password(Some(&component(password)))
        .map_err(|_| anyhow::anyhow!("Invalid database password"))?;
    uri.set_path(&format!("/{}", component(database)));
    uri.query_pairs_mut().append_pair("sslmode", ssl);
    Ok(uri.to_string())
}

fn provisioning_sql(user: &str, db: &str, password: &str) -> String {
    // Password never appears inside a dollar-quoted body. Identifiers are generated
    // slugs and checked before this function is called.
    format!(
        "SET standard_conforming_strings = on;\nCREATE ROLE \"{user}\" LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD '{}';\nCREATE DATABASE \"{db}\" OWNER \"{user}\";\nREVOKE CONNECT ON DATABASE \"{db}\" FROM PUBLIC;\nGRANT CONNECT ON DATABASE \"{db}\" TO \"{user}\";\n",
        password.replace('\'', "''")
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(default)]
    pub engine: Option<String>,
    pub mode: String,
    pub source_id: Option<String>,
}

fn postgres_service(driver: &Value, user: &str, password: &str, database: &str) -> Value {
    json!({"image":driver["image"],"restart":"unless-stopped",
        "environment":{"POSTGRES_USER":format!("${{{user}:?Set database user}}"),"POSTGRES_PASSWORD":format!("${{{password}:?Set database password}}"),"POSTGRES_DB":format!("${{{database}:?Set database name}}")},
        "volumes":[format!("database-data:{}",driver["data_path"].as_str().unwrap_or_default())],"healthcheck":driver["healthcheck"]})
}

fn database_service(
    driver: &Value,
    user: &str,
    password: &str,
    database: &str,
    root: Option<&str>,
) -> Result<Value> {
    match driver["engine"].as_str().unwrap_or_default() {
        "postgres" => {
            let mut service = postgres_service(driver, user, password, database);
            if let Some(root) = root {
                service["environment"] = json!({"POSTGRES_USER":"selfhostroot","POSTGRES_PASSWORD":format!("${{{root}:?Set database administrator password}}"),"POSTGRES_DB":"postgres","SELFHOST_APP_USER":format!("${{{user}}}"),"SELFHOST_APP_PASSWORD":format!("${{{password}}}"),"SELFHOST_APP_DATABASE":format!("${{{database}}}")});
                service["entrypoint"] = json!([
                    "sh",
                    "-ec",
                    r#"cat > /docker-entrypoint-initdb.d/selfhost-app.sh <<'SELFHOST_INIT'
#!/bin/sh
set -eu
psql --username "$$POSTGRES_USER" --dbname postgres --no-psqlrc --set ON_ERROR_STOP=1 <<'SELFHOST_SQL'
\getenv app_user SELFHOST_APP_USER
\getenv app_password SELFHOST_APP_PASSWORD
\getenv app_database SELFHOST_APP_DATABASE
CREATE ROLE :"app_user" LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD :'app_password';
CREATE DATABASE :"app_database" OWNER :"app_user";
REVOKE CONNECT ON DATABASE :"app_database" FROM PUBLIC;
GRANT CONNECT ON DATABASE :"app_database" TO :"app_user";
SELFHOST_SQL
SELFHOST_INIT
chmod 755 /docker-entrypoint-initdb.d/selfhost-app.sh
exec /usr/local/bin/docker-entrypoint.sh postgres
"#
                ]);
            }
            Ok(service)
        }
        "mysql" | "mariadb" => {
            crate::database_mysql::service(driver, user, password, database, root)
        }
        "mongodb" => crate::database_mongo::service(driver, user, password, database, root),
        _ => anyhow::bail!("Database engine is not implemented"),
    }
}
pub(crate) fn connection_uri(c: &Connection) -> Result<String> {
    match c.engine.as_str() {
        "postgres" => postgres_uri(
            &c.host,
            c.port,
            &c.username,
            &c.password,
            &c.database,
            &c.ssl_mode,
        ),
        "mysql" | "mariadb" => {
            let uri = postgres_uri(
                &c.host,
                c.port,
                &c.username,
                &c.password,
                &c.database,
                &c.ssl_mode,
            )?;
            // App adapters also receive explicit TLS variables: URI TLS query
            // parameters are not portable between MySQL drivers.
            Ok(uri
                .split('?')
                .next()
                .unwrap()
                .replacen("postgresql:", "mysql:", 1))
        }
        "mongodb" => crate::database_mongo::uri(c),
        _ => anyhow::bail!("Unsupported database engine"),
    }
}
pub(crate) fn postgres_client(
    c: &Connection,
    argv: Vec<String>,
    input: Option<Vec<u8>>,
) -> Result<ClientCommand> {
    Ok(ClientCommand {
        image: crate::catalog::database_driver("postgres")?["image"]
            .as_str()
            .context("Missing database image")?
            .into(),
        argv,
        input,
        environment: BTreeMap::from([
            ("PGHOST".into(), c.host.clone()),
            ("PGPORT".into(), c.port.to_string()),
            ("PGUSER".into(), c.username.clone()),
            ("PGPASSWORD".into(), c.password.clone()),
            ("PGDATABASE".into(), c.database.clone()),
            ("PGSSLMODE".into(), c.ssl_mode.clone()),
            ("PGCONNECT_TIMEOUT".into(), "15".into()),
        ]),
    })
}

impl Store {
    fn database_sources_lock(&self) -> Result<std::fs::File> {
        use fs2::FileExt;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("database-sources.lock"))?;
        file.try_lock_exclusive()
            .context("Another database source operation is running")?;
        Ok(file)
    }
    pub fn database_source_references(&self, id: &str) -> Result<Vec<String>> {
        let mut references = Vec::new();
        for project in self.read()?.projects {
            let setup = self.setup(&project.id)?;
            if setup
                .database
                .as_ref()
                .is_some_and(|b| b.source_id.as_deref() == Some(id))
            {
                references.push(project.name);
            }
        }
        Ok(references)
    }
    fn backup_database_source(&self, source: &Source) -> Result<()> {
        let directory = self.root.join("database-source-recovery");
        crate::core::private_dir(&directory)?;
        atomic_write(
            &directory.join(format!("{}-{}.json", source.id, token(8)?)),
            &serde_json::to_vec_pretty(source)?,
        )
    }
    pub fn rename_database_source(&self, id: &str, name: &str) -> Result<()> {
        let _lock = self.database_sources_lock()?;
        ensure!(
            !name.trim().is_empty() && name.len() <= 64,
            "Choose a source name up to 64 characters"
        );
        let mut source = self.database_source(id)?;
        self.backup_database_source(&source)?;
        source.name = name.trim().into();
        atomic_write(
            &self
                .root
                .join("database-sources")
                .join(format!("{id}.json")),
            &serde_json::to_vec_pretty(&source)?,
        )
    }
    pub fn remove_database_source(&self, id: &str) -> Result<()> {
        let _lock = self.database_sources_lock()?;
        let source = self.database_source(id)?;
        ensure!(
            self.database_source_references(id)?.is_empty(),
            "This database source is still referenced by projects"
        );
        self.backup_database_source(&source)?;
        fs::remove_file(
            self.root
                .join("database-sources")
                .join(format!("{id}.json")),
        )?;
        // This removes only a connection record. Hosted database projects and volumes remain intact.
        Ok(())
    }
    pub async fn edit_database_source(&self, id: &str, mut replacement: Source) -> Result<()> {
        let _lock = self.database_sources_lock()?;
        let old = self.database_source(id)?;
        ensure!(
            old.managed_project.is_none(),
            "Use source rename for a managed database; changing its saved password does not rotate the database account"
        );
        if replacement.password.is_empty() {
            replacement.password = old.password.clone();
        }
        ensure!(
            replacement.id == old.id
                && replacement.engine == old.engine
                && replacement.kind == old.kind
                && replacement.server_id == old.server_id
                && replacement.network == old.network
                && replacement.managed_project == old.managed_project,
            "Source identity, engine, placement and ownership cannot change"
        );
        ensure!(
            !replacement.name.trim().is_empty() && replacement.name.len() <= 64,
            "Choose a source name up to 64 characters"
        );
        ensure!(
            replacement.port > 0
                && !replacement.host.is_empty()
                && !replacement.username.is_empty()
                && !replacement.database.is_empty()
                && !replacement.password.is_empty(),
            "Complete the database connection"
        );
        ensure!(
            [
                &replacement.host,
                &replacement.username,
                &replacement.database,
                &replacement.password,
                &replacement.auth_database
            ]
            .iter()
            .all(|v| !v.contains(['\0', '\r', '\n'])),
            "Database values must be single lines"
        );
        ensure!(
            ["disable", "require", "verify-full"].contains(&replacement.ssl_mode.as_str()),
            "Invalid TLS mode"
        );
        if !self.database_source_references(id)?.is_empty() {
            ensure!(
                old.host == replacement.host
                    && old.port == replacement.port
                    && old.database == replacement.database
                    && old.ssl_mode == replacement.ssl_mode
                    && old.auth_database == replacement.auth_database,
                "Projects use this source. Changing its database destination or TLS settings needs a reviewed data migration"
            );
            if old.kind == "external" {
                ensure!(
                    old.username == replacement.username && old.password == replacement.password,
                    "Projects use these credentials. Rotate their database account and app connections together before changing the source"
                );
            }
        }
        connection_uri(&replacement.connection())?;
        self.test_database_connection(&replacement).await?;
        self.backup_database_source(&old)?;
        atomic_write(
            &self
                .root
                .join("database-sources")
                .join(format!("{id}.json")),
            &serde_json::to_vec_pretty(&replacement)?,
        )
    }
    pub async fn test_database_source(&self, id: &str) -> Result<()> {
        self.test_database_connection(&self.database_source(id)?)
            .await
    }
    async fn test_database_connection(&self, source: &Source) -> Result<()> {
        let connection = source.connection();
        let client=match source.engine.as_str() {
            "postgres"=>postgres_client(&connection,vec!["psql".into(),"-X".into(),"-v".into(),"ON_ERROR_STOP=1".into()],Some(b"SELECT 1;\n".to_vec()))?,
            "mysql"|"mariadb"=>crate::database_mysql::client(&connection,"query",Some(b"SELECT 1;\n".to_vec()))?,
            "mongodb"=>ClientCommand {
                image:crate::catalog::database_driver("mongodb")?["image"].as_str().context("Missing client image")?.into(),
                argv:crate::database_mongo::provisioning_command(),environment:BTreeMap::new(),
                input:Some(format!("const target=new Mongo({}).getDB({}); if(target.runCommand({{ping:1}}).ok!==1) throw new Error('Connection failed');",serde_json::to_string(&crate::database_mongo::uri(&connection)?)?,serde_json::to_string(&connection.database)?).into_bytes()),
            },
            _=>anyhow::bail!("Unsupported database engine"),
        };
        let server = self.server(&source.server_id)?;
        let name = format!("selfhost-db-check-{}", token(6)?);
        let mut command = server.docker_command(true)?;
        command.args(["run", "--rm", "-i", "--name", &name]);
        if let Some(network) = &source.network {
            command.args(["--network", network]);
        }
        for (key, value) in &client.environment {
            command.args(["--env", key]);
            command.env(key, value);
        }
        command
            .arg(&client.image)
            .args(&client.argv)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let result = async {
            let mut child = command.spawn()?;
            child
                .stdin
                .take()
                .context("Missing database input")?
                .write_all(client.input.as_deref().unwrap_or_default())
                .await?;
            let status = tokio::time::timeout(Duration::from_secs(120), child.wait()).await??;
            ensure!(
                status.success(),
                "Database connection failed. Check its host, credentials, TLS and host access"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let mut cleanup = server.docker_command(true)?;
        cleanup.args(["rm", "-f", &name]);
        let _ = crate::core::run(cleanup, 15).await;
        result
    }
    pub(crate) fn check_database_ready(&self, id: &str) -> Result<()> {
        if let Some(database) = self.setup(id)?.database {
            ensure!(
                database.provisioned,
                "Create the project database on its shared server before starting services"
            );
        }
        Ok(())
    }
    pub(crate) fn database_source(&self, id: &str) -> Result<Source> {
        ensure!(crate::setup::slug(id), "Invalid database source id");
        Ok(serde_json::from_slice(&fs::read(
            self.root
                .join("database-sources")
                .join(format!("{id}.json")),
        )?)?)
    }
    pub fn database_sources(&self) -> Result<Vec<Value>> {
        let dir = self.root.join("database-sources");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut sources = vec![];
        for file in fs::read_dir(dir)? {
            let path = file?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let source: Source = serde_json::from_slice(&fs::read(path)?)?;
                sources.push(json!({"id":source.id,"name":source.name,"engine":source.engine,"auth_database":source.auth_database,"kind":source.kind,"server_id":source.server_id,"host":source.host,"port":source.port,"database":source.database,"username":source.username,"ssl_mode":source.ssl_mode,"managed_project":source.managed_project}));
            }
        }
        sources.sort_by_key(|s| s["name"].as_str().unwrap_or_default().to_lowercase());
        Ok(sources)
    }
    pub fn add_database_source(&self, input: NewSource) -> Result<String> {
        ensure!(
            !input.name.trim().is_empty() && input.name.len() <= 64,
            "Choose a source name"
        );
        ensure!(
            ["external", "shared"].contains(&input.kind.as_str()),
            "Choose external or shared"
        );
        ensure!(
            ["disable", "require", "verify-full"].contains(&input.ssl_mode.as_str()),
            "Invalid TLS mode"
        );
        let driver = crate::catalog::database_driver(&input.engine)?;
        let default_port = driver["port"].as_u64().context("Missing database port")? as u16;
        let id = format!("db-{}", token(5)?);
        let mut source = Source {
            engine: input.engine,
            auth_database: input.auth_database,
            id: id.clone(),
            name: input.name,
            kind: input.kind,
            server_id: input.server_id,
            host: input.host,
            port: if input.port == 0 {
                default_port
            } else {
                input.port
            },
            username: input.username,
            password: input.password,
            database: input.database,
            ssl_mode: input.ssl_mode,
            network: None,
            managed_project: None,
        };
        if input.managed {
            ensure!(
                source.kind == "shared",
                "Managed sources are shared database servers"
            );
            source.host = id.clone();
            source.port = default_port;
            source.username = match source.engine.as_str() {
                "mysql" | "mariadb" => "root",
                "mongodb" => "selfhostroot",
                _ => "selfhostadmin",
            }
            .into();
            source.password = token(32)?;
            source.database = match source.engine.as_str() {
                "mysql" | "mariadb" => "mysql",
                "mongodb" => "admin",
                _ => "postgres",
            }
            .into();
            if source.engine == "mongodb" {
                source.auth_database = "admin".into();
            }
            source.ssl_mode = "disable".into();
            source.network = Some(id.clone());
            let mut service = database_service(
                &driver,
                "DB_ADMIN_USER",
                "DB_ADMIN_PASSWORD",
                "DB_ADMIN_DATABASE",
                None,
            )?;
            service["networks"] = json!({"database":{"aliases":[id]}});
            let setup = Setup {
                database: None,
                compose: json!({"services":{source.engine.clone():service},"volumes":{"database-data":{}},"networks":{"database":{"name":id}}}),
                environment: BTreeMap::from([
                    ("DB_ADMIN_USER".into(), source.username.clone()),
                    ("DB_ADMIN_PASSWORD".into(), source.password.clone()),
                    ("DB_ADMIN_DATABASE".into(), source.database.clone()),
                ]),
                files: BTreeMap::new(),
            };
            let project = self.create_setup(
                &format!("{} database", source.name),
                &source.server_id,
                setup,
            )?;
            source.managed_project = Some(project.id);
        } else {
            ensure!(
                !source.host.trim().is_empty() && !source.host.starts_with('-') && source.port > 0,
                "Set a database host and port"
            );
            ensure!(
                !source.username.is_empty()
                    && !source.database.is_empty()
                    && !source.password.is_empty(),
                "Set database name, user and password"
            );
            ensure!(
                ![
                    &source.host,
                    &source.username,
                    &source.database,
                    &source.password,
                    &source.auth_database
                ]
                .iter()
                .any(|v| v.contains(['\0', '\r', '\n'])),
                "Database values must be single lines"
            );
        }
        if source.engine == "mongodb" && source.auth_database.is_empty() {
            source.auth_database = "admin".into();
        }
        connection_uri(&source.connection())?;
        let dir = self.root.join("database-sources");
        crate::core::private_dir(&dir)?;
        atomic_write(
            &dir.join(format!("{id}.json")),
            &serde_json::to_vec_pretty(&source)?,
        )?;
        Ok(id)
    }
    pub async fn start_database_source(&self, id: &str) -> Result<()> {
        let source = self.database_source(id)?;
        self.action(
            source
                .managed_project
                .as_deref()
                .context("This database is externally hosted")?,
            "start",
        )
        .await
    }
    pub fn configure_database(&self, id: &str, selection: Selection) -> Result<()> {
        let _sources = self.database_sources_lock()?;
        let _operation = self.project_lock(id)?;
        ensure!(
            ["dedicated", "external", "shared"].contains(&selection.mode.as_str()),
            "Choose dedicated, external or shared"
        );
        let project = self.project(id)?;
        ensure!(
            !self.read()?.activities.iter().any(|a| a.project_id == id
                && a.status == "succeeded"
                && (matches!(a.action.as_str(), "start" | "refresh")
                    || a.action.ends_with(": start")
                    || a.action.ends_with(": update"))),
            "This project has already been started. Back up and migrate its existing data before changing database settings."
        );
        let mut setup = self.setup(id)?;
        ensure!(
            setup.database.is_none(),
            "A database is already attached. Changing database placement requires transferring its data; edit a copy or export the setup first."
        );
        let source = if selection.mode == "dedicated" {
            None
        } else {
            Some(
                self.database_source(
                    selection
                        .source_id
                        .as_deref()
                        .context("Choose a database source")?,
                )?,
            )
        };
        let recipes = project
            .services
            .iter()
            .filter_map(|s| s.definition.database.as_ref())
            .collect::<Vec<_>>();
        ensure!(
            !recipes.is_empty() || selection.engine.is_some(),
            "Choose an explicit engine for this custom setup and map its DATABASE_* variables in Compose"
        );
        let engine = selection
            .engine
            .clone()
            .or_else(|| source.as_ref().map(|s| s.engine.clone()))
            .or_else(|| recipes.first().map(|r| r.engine.clone()))
            .context("Choose a database engine")?;
        for recipe in &recipes {
            ensure!(
                recipe.engine == engine || recipe.engines.contains(&engine),
                "Database engine {engine} is not supported by every app in this project"
            );
            if recipe.engine != engine {
                ensure!(
                    recipe.engine_environment.contains_key(&engine),
                    "Missing database adapter for {engine}"
                );
            }
        }
        if let Some(source) = &source {
            ensure!(
                source.engine == engine,
                "Database source engine does not match the selection"
            );
        }
        let driver = crate::catalog::database_driver(&engine)?;
        let dbname = id.replace('-', "_");
        let username = format!("u_{}", token(6)?);
        let password = token(32)?;
        let mut host = "selfhost-database".to_string();
        let mut port = driver["port"].as_u64().context("Missing database port")? as u16;
        let mut user = username;
        let mut pass = password;
        let mut database = dbname;
        let mut ssl = "disable".to_string();
        let mut auth_database = if engine == "mongodb" {
            database.clone()
        } else {
            String::new()
        };
        if selection.mode == "dedicated" {
            ensure!(
                setup.compose["services"].get("selfhost-database").is_none(),
                "Service name selfhost-database is already in use"
            );
            ensure!(
                setup.compose["volumes"].get("database-data").is_none(),
                "Volume name database-data is already in use"
            );
            setup
                .environment
                .insert("DATABASE_ROOT_PASSWORD".into(), token(32)?);
            setup.compose["services"]["selfhost-database"] = database_service(
                &driver,
                "DATABASE_USER",
                "DATABASE_PASSWORD",
                "DATABASE_NAME",
                Some("DATABASE_ROOT_PASSWORD"),
            )?;
            if setup.compose.get("volumes").is_none() {
                setup.compose["volumes"] = json!({});
            }
            setup.compose["volumes"]["database-data"] = json!({});
        } else {
            let source = source.context("Choose a database source")?;
            ensure!(
                source.kind == selection.mode,
                "Database source type does not match"
            );
            if source.network.is_some() {
                ensure!(
                    source.server_id == project.server_id,
                    "Shared database and project must use the same Docker engine"
                );
            }
            host = source.host;
            port = source.port;
            ssl = source.ssl_mode;
            if selection.mode == "external" {
                user = source.username;
                pass = source.password;
                database = source.database;
                auth_database = source.auth_database;
            }
            if let Some(network) = source.network {
                if setup.compose.get("networks").is_none() {
                    setup.compose["networks"] = json!({});
                }
                ensure!(
                    setup.compose["networks"].get("database-source").is_none(),
                    "Network database-source already exists"
                );
                setup.compose["networks"]["database-source"] =
                    json!({"external":true,"name":network});
                // Preserve the existing app network while attaching the shared source network.
                for service in setup.compose["services"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                {
                    ensure!(
                        service.get("network_mode").is_none(),
                        "Database attachment needs Compose networks"
                    );
                    match service.get_mut("networks") {
                        Some(Value::Array(n)) => n.push(json!("database-source")),
                        Some(Value::Object(n)) => {
                            n.insert("database-source".into(), json!({}));
                        }
                        None => {
                            service["networks"] = json!(["default", "database-source"]);
                        }
                        _ => anyhow::bail!("Invalid service networks"),
                    }
                }
                if setup.compose["networks"].get("default").is_none() {
                    setup.compose["networks"]["default"] = json!({});
                }
            }
        }
        for recipe in &recipes {
            if let Some(modes) = recipe.engine_ssl_modes.get(&engine) {
                ensure!(
                    modes.contains(&ssl),
                    "This app adapter does not support the selected database TLS mode"
                );
            }
        }
        let mysql_ssl = match ssl.as_str() {
            "disable" => "DISABLED",
            "require" => "REQUIRED",
            _ => "VERIFY_IDENTITY",
        }
        .to_owned();
        let uri = connection_uri(&Connection {
            engine: engine.clone(),
            host: host.clone(),
            port,
            username: user.clone(),
            password: pass.clone(),
            database: database.clone(),
            ssl_mode: ssl.clone(),
            auth_database: auth_database.clone(),
        })?;
        let needs_go_dsn = recipes.iter().any(|r| {
            let mapping = if r.engine == engine {
                &r.environment
            } else {
                &r.engine_environment[&engine]
            };
            mapping
                .values()
                .any(|v| v.contains("${DATABASE_GO_MYSQL_DSN}"))
        }) || setup
            .compose
            .to_string()
            .contains("${DATABASE_GO_MYSQL_DSN}");
        if needs_go_dsn {
            ensure!(
                ["mysql", "mariadb"].contains(&engine.as_str()),
                "Go MySQL connection string requires a MySQL-compatible engine"
            );
            let dsn = crate::database_mysql::go_dsn(&Connection {
                engine: engine.clone(),
                host: host.clone(),
                port,
                username: user.clone(),
                password: pass.clone(),
                database: database.clone(),
                ssl_mode: ssl.clone(),
                auth_database: auth_database.clone(),
            })?;
            setup
                .environment
                .insert("DATABASE_GO_MYSQL_DSN".into(), dsn);
        }
        let ssl_enabled = (ssl != "disable").to_string();
        let ssl_verify = (ssl == "verify-full").to_string();
        for (key, value) in [
            ("DATABASE_HOST", host),
            ("DATABASE_PORT", port.to_string()),
            ("DATABASE_NAME", database),
            ("DATABASE_USER", user),
            ("DATABASE_PASSWORD", pass),
            ("DATABASE_SSL_MODE", ssl),
            ("DATABASE_URL", uri),
            ("DATABASE_AUTH_DATABASE", auth_database),
            ("DATABASE_MYSQL_SSL_MODE", mysql_ssl),
            ("DATABASE_SSL_ENABLED", ssl_enabled),
            ("DATABASE_SSL_REJECT_UNAUTHORIZED", ssl_verify),
        ] {
            setup.environment.insert(key.into(), value);
        }
        setup.database = Some(Binding {
            engine: engine.clone(),
            provisioning_uncertain: false,
            mode: selection.mode.clone(),
            source_id: selection.source_id,
            provisioned: selection.mode != "shared",
        });
        for service in &project.services {
            if let Some(recipe) = &service.definition.database {
                let target = &mut setup.compose["services"][&service.app];
                if target.get("environment").is_none() {
                    target["environment"] = json!({});
                }
                let environment = target["environment"]
                    .as_object_mut()
                    .context("Database mapping needs an environment mapping")?;
                for name in &recipe.unset_environment {
                    environment.remove(name);
                }
                let mapping = if recipe.engine == engine {
                    &recipe.environment
                } else {
                    &recipe.engine_environment[&engine]
                };
                for (name, value) in mapping {
                    environment.insert(name.clone(), json!(value));
                }
                if selection.mode == "dedicated" {
                    if target.get("depends_on").is_none() {
                        target["depends_on"] = json!({});
                    }
                    target["depends_on"]
                        .as_object_mut()
                        .context("Use a depends_on mapping")?
                        .insert(
                            "selfhost-database".into(),
                            json!({"condition":"service_healthy"}),
                        );
                }
            }
        }
        self.save_setup_locked(id, setup)?;
        Ok(())
    }
    pub async fn provision_database(&self, id: &str) -> Result<()> {
        let _sources = self.database_sources_lock()?;
        let _operation = self.project_lock(id)?;
        self.project_docker(&self.project(id)?, true)?;
        let setup = self.setup(id)?;
        let binding = setup.database.as_ref().context("Choose a database first")?;
        ensure!(
            binding.mode == "shared",
            "Only shared servers need database provisioning"
        );
        ensure!(!binding.provisioned, "Database is already provisioned");
        ensure!(
            !binding.provisioning_uncertain,
            "Previous provisioning may have changed the server. Inspect its user/database and recover manually before retrying; administrative writes are never repeated automatically."
        );
        let source =
            self.database_source(binding.source_id.as_deref().context("Missing source")?)?;
        let server = self.server(&source.server_id)?;
        let env = &setup.environment;
        let user = env.get("DATABASE_USER").context("Missing user")?;
        let db = env.get("DATABASE_NAME").context("Missing database")?;
        let password = env.get("DATABASE_PASSWORD").context("Missing password")?;
        ensure!(
            crate::setup::slug(user) && crate::setup::slug(db),
            "Invalid generated database identity"
        );
        ensure!(
            source.engine == binding.engine,
            "Database source engine changed"
        );
        let connection = source.connection();
        let client = match source.engine.as_str() {
            "postgres" => postgres_client(
                &connection,
                vec![
                    "psql".into(),
                    "-X".into(),
                    "-v".into(),
                    "ON_ERROR_STOP=1".into(),
                ],
                Some(provisioning_sql(user, db, password).into_bytes()),
            )?,
            "mysql" | "mariadb" => crate::database_mysql::client(
                &connection,
                "provision",
                Some(crate::database_mysql::provisioning_sql(user, db, password)?.into_bytes()),
            )?,
            "mongodb" => ClientCommand {
                image: crate::catalog::database_driver("mongodb")?["image"]
                    .as_str()
                    .context("Missing database image")?
                    .into(),
                argv: crate::database_mongo::provisioning_command(),
                environment: BTreeMap::new(),
                input: Some(
                    crate::database_mongo::provisioning_script(&connection, user, db, password)?
                        .into_bytes(),
                ),
            },
            _ => anyhow::bail!("Unsupported database engine"),
        };
        let mut pending = setup.clone();
        pending.database.as_mut().unwrap().provisioning_uncertain = true;
        self.save_setup_locked(id, pending)?;
        let name = format!("selfhost-db-init-{}", token(6)?);
        let mut cmd = server.docker_command(true)?;
        cmd.args(["run", "--rm", "-i", "--name", &name]);
        if let Some(network) = &source.network {
            cmd.args(["--network", network]);
        }
        for (key, value) in &client.environment {
            cmd.args(["--env", key]);
            cmd.env(key, value);
        }
        cmd.arg(&client.image).args(&client.argv);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let result=async {let mut child=cmd.spawn()?;child.stdin.take().context("Missing input")?.write_all(client.input.as_deref().unwrap_or_default()).await?;let status=tokio::time::timeout(Duration::from_secs(120),child.wait()).await??;ensure!(status.success(),"Database provisioning failed. Check the source connection and its database creation permissions.");Ok::<_,anyhow::Error>(())}.await;
        let mut cleanup = server.docker_command(true)?;
        cleanup.args(["rm", "-f", &name]);
        let _ = crate::core::run(cleanup, 15).await;
        result.context("Provisioning may be incomplete. Inspect the source and saved project credentials before manual recovery; automatic retries are disabled")?;
        let mut saved = self.setup(id)?;
        ensure!(
            saved.environment == setup.environment,
            "Setup changed during provisioning; review before retrying"
        );
        if let Some(binding) = &mut saved.database {
            binding.provisioned = true;
            binding.provisioning_uncertain = false;
        }
        self.save_setup_locked(id, saved)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    fn external_fixture(store: &Store) -> String {
        store
            .add_database_source(NewSource {
                engine: "postgres".into(),
                auth_database: String::new(),
                name: "Fixture database".into(),
                kind: "external".into(),
                managed: false,
                server_id: "local".into(),
                host: "database.example.test".into(),
                port: 5432,
                username: "fixture".into(),
                password: "synthetic-database-password".into(),
                database: "fixture".into(),
                ssl_mode: "verify-full".into(),
            })
            .unwrap()
    }
    #[test]
    fn source_removal_preserves_recovery_copy_and_rejects_references() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let id = external_fixture(&store);
        store
            .rename_database_source(&id, "Renamed database")
            .unwrap();
        assert_eq!(store.database_source(&id).unwrap().name, "Renamed database");
        let setup = Setup {
            compose: json!({"services":{"app":{"image":"example/app:1"}}}),
            environment: BTreeMap::new(),
            files: BTreeMap::new(),
            database: Some(Binding {
                engine: "postgres".into(),
                mode: "external".into(),
                source_id: Some(id.clone()),
                provisioned: true,
                provisioning_uncertain: false,
            }),
        };
        store.create_setup("Example app", "local", setup).unwrap();
        assert!(store.remove_database_source(&id).is_err());
        assert!(store.database_source(&id).is_ok());
        let unreferenced = external_fixture(&store);
        store.remove_database_source(&unreferenced).unwrap();
        assert!(store.database_source(&unreferenced).is_err());
        assert_eq!(
            fs::read_dir(directory.path().join("database-source-recovery"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(store.read().unwrap().projects.len(), 1);
    }
    #[tokio::test]
    async fn referenced_external_source_rejects_redirect_and_password_change_before_network() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let id = external_fixture(&store);
        let setup = Setup {
            compose: json!({"services":{"app":{"image":"example/app:1"}}}),
            environment: BTreeMap::new(),
            files: BTreeMap::new(),
            database: Some(Binding {
                engine: "postgres".into(),
                mode: "external".into(),
                source_id: Some(id.clone()),
                provisioned: true,
                provisioning_uncertain: false,
            }),
        };
        store.create_setup("Example app", "local", setup).unwrap();
        let original = store.database_source(&id).unwrap();
        let mut changed = original.clone();
        changed.host = "another.example.test".into();
        assert!(
            store
                .edit_database_source(&id, changed)
                .await
                .unwrap_err()
                .to_string()
                .contains("migration")
        );
        let mut changed = original.clone();
        changed.password = "different-synthetic-password".into();
        assert!(
            store
                .edit_database_source(&id, changed)
                .await
                .unwrap_err()
                .to_string()
                .contains("Rotate")
        );
        assert_eq!(
            store.database_source(&id).unwrap().password,
            original.password
        );
    }
    #[tokio::test]
    #[ignore = "Starts isolated local containers for all four database drivers and tests Store provisioning/backup/restore"]
    async fn all_engines_dedicated_shared_backup_restore_live() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::open(dir.path().into())?;
        let mut tracked = Vec::new();
        let result:Result<()> = async {
            for engine in ["postgres","mysql","mariadb","mongodb"] {
                if std::env::var("SELFHOST_DATABASE_TEST_ENGINE").is_ok_and(|selected|selected!=engine){continue;}
                for mode in ["dedicated","shared"] {
                    let source=if mode=="shared" {
                        let id=store.add_database_source(NewSource{engine:engine.into(),auth_database:String::new(),name:"Isolated fixture source".into(),kind:"shared".into(),managed:true,server_id:"local".into(),host:String::new(),port:0,username:String::new(),password:String::new(),database:String::new(),ssl_mode:"require".into()})?;
                        let source=store.database_source(&id)?;
                        let project=store.project(source.managed_project.as_deref().unwrap())?;
                        tracked.push(project.clone());
                        store.materialize_setup(&project)?;
                        store.docker(&project,&["up","-d","--wait",engine]).await?;
                        Some(id)
                    } else {None};
                    let project=store.create_setup("Isolated archive fixture","local",Setup{database:None,compose:json!({"services":{"client":{"image":"alpine:3.22","command":["sleep","600"]}}}),environment:BTreeMap::new(),files:BTreeMap::new()})?;
                    tracked.push(project.clone());
                    store.configure_database(&project.id,Selection{engine:Some(engine.into()),mode:mode.into(),source_id:source.clone()})?;
                    store.materialize_setup(&project)?;
                    if mode=="shared" {store.provision_database(&project.id).await?;} else {store.docker(&project,&["up","-d","--wait","selfhost-database"]).await?;}
                    let mut connection=setup_connection(&store.setup(&project.id)?)?;
                    let network=if let Some(source)=source {store.database_source(&source)?.network.unwrap()}else{connection.host="127.0.0.1".into();format!("container:{}",store.service_container(&project.id,"selfhost-database").await?.context("Missing fixture database")?)};
                    async fn execute(store:&Store,project:&crate::core::Project,c:&Connection,network:&str,script:&str)->Result<String>{
                        let client=match c.engine.as_str(){
                            "postgres"=>postgres_client(c,vec!["psql".into(),"-X".into(),"-v".into(),"ON_ERROR_STOP=1".into()],None)?,
                            "mysql"|"mariadb"=>crate::database_mysql::client(c,"restore",None)?,
                            _=>ClientCommand{image:crate::catalog::database_driver("mongodb")?["image"].as_str().unwrap().into(),argv:crate::database_mongo::provisioning_command(),environment:BTreeMap::new(),input:None},
                        };
                        let mut cmd=store.project_docker(project,true)?;
                        cmd.args(["run","--rm","-i","--network",network]);
                        for(key,value)in client.environment{cmd.args(["--env",&key]).env(key,value);}
                        cmd.arg(client.image).args(client.argv).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
                        let mut child=cmd.spawn()?;child.stdin.take().unwrap().write_all(script.as_bytes()).await?;
                        let output=tokio::time::timeout(Duration::from_secs(60),child.wait_with_output()).await??;
                        ensure!(output.status.success(),"Fixture {} query failed: {}",c.engine,String::from_utf8_lossy(&output.stderr).replace(&c.password,"[redacted]").replace(&connection_uri(c)?,"[private-uri]"));
                        Ok(String::from_utf8_lossy(&output.stdout).into())
                    }
                    let mongo=if engine=="mongodb"{format!("const db=new Mongo({}).getDB({});",serde_json::to_string(&connection_uri(&connection)?)?,serde_json::to_string(&connection.database)?)}else{String::new()};
                    let create=if engine=="mongodb"{format!("{mongo}db.fixture.insertOne({{marker:'retained'}});")}else{"CREATE TABLE fixture(marker varchar(32)); INSERT INTO fixture VALUES ('retained');".into()};
                    execute(&store,&project,&connection,&network,&create).await?;
                    let backup=store.backup_database(&project.id).await?;
                    ensure!(backup.engine==engine,"Wrong archive engine");
                    let change=if engine=="mongodb"{format!("{mongo}db.fixture.drop();")}else{"DROP TABLE fixture;".into()};
                    execute(&store,&project,&connection,&network,&change).await?;
                    store.restore_database(&project.id,&backup.id,&project.id).await?;
                    let query=if engine=="mongodb"{format!("{mongo}print(db.fixture.findOne().marker);")}else{"SELECT marker FROM fixture;".into()};
                    ensure!(execute(&store,&project,&connection,&network,&query).await?.contains("retained"),"Archive did not restore marker");
                    for project in tracked.drain(..).rev(){store.docker(&project,&["down","--volumes","--remove-orphans"]).await?;}
                }
            }
            Ok(())
        }.await;
        for project in tracked.iter().rev() {
            let _ = store
                .docker(project, &["down", "--volumes", "--remove-orphans"])
                .await;
        }
        result
    }
    #[test]
    fn engine_selection_is_explicit_for_custom_setups_and_credentials_are_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        for engine in ["postgres", "mysql", "mariadb", "mongodb"] {
            let project = store.create_setup("Custom database client", "local", Setup { database:None, compose:json!({"services":{"client":{"image":"alpine:3.22","environment":{"CONNECTION":"${DATABASE_URL}"}}}}), environment:BTreeMap::from([("DATABASE_URL".into(),"pending".into())]),files:BTreeMap::new() }).unwrap();
            assert!(
                store
                    .configure_database(
                        &project.id,
                        Selection {
                            engine: None,
                            mode: "dedicated".into(),
                            source_id: None
                        }
                    )
                    .is_err()
            );
            store
                .configure_database(
                    &project.id,
                    Selection {
                        engine: Some(engine.into()),
                        mode: "dedicated".into(),
                        source_id: None,
                    },
                )
                .unwrap();
            let setup = store.setup(&project.id).unwrap();
            assert_eq!(setup.database.as_ref().unwrap().engine, engine);
            assert_ne!(
                setup.environment["DATABASE_ROOT_PASSWORD"],
                setup.environment["DATABASE_PASSWORD"]
            );
            assert!(
                !setup.compose["services"]["client"]
                    .to_string()
                    .contains("DATABASE_ROOT_PASSWORD")
            );
            assert!(
                setup.compose["services"]["selfhost-database"]
                    .get("ports")
                    .is_none()
            );
            let connection = setup_connection(&setup).unwrap();
            assert_eq!(connection.engine, engine);
            let archive = crate::backups::archive_client(&connection, false).unwrap();
            assert!(!archive.argv.join(" ").contains(&connection.password));
            if engine != "postgres" {
                assert!(!archive.argv.iter().any(|a| a == "pg_dump"));
            }
        }
        let project = mealie(&store);
        let original = serde_json::to_string(&store.setup(&project.id).unwrap()).unwrap();
        assert!(
            store
                .configure_database(
                    &project.id,
                    Selection {
                        engine: Some("mongodb".into()),
                        mode: "dedicated".into(),
                        source_id: None
                    }
                )
                .is_err()
        );
        assert_eq!(
            original,
            serde_json::to_string(&store.setup(&project.id).unwrap()).unwrap()
        );
    }
    #[tokio::test]
    async fn uncertain_provisioning_cannot_repeat_administrative_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let project = mealie(&store);
        let source = store
            .add_database_source(NewSource {
                engine: "postgres".into(),
                auth_database: String::new(),
                name: "Fixture source".into(),
                kind: "shared".into(),
                managed: true,
                server_id: "local".into(),
                host: String::new(),
                port: 0,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl_mode: "require".into(),
            })
            .unwrap();
        store
            .configure_database(
                &project.id,
                Selection {
                    engine: None,
                    mode: "shared".into(),
                    source_id: Some(source),
                },
            )
            .unwrap();
        let mut setup = store.setup(&project.id).unwrap();
        setup.database.as_mut().unwrap().provisioning_uncertain = true;
        store.save_setup_locked(&project.id, setup).unwrap();
        let error = store
            .provision_database(&project.id)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("Previous provisioning may have changed"),
            "{error}"
        );
    }
    #[test]
    fn postgres_uri_encodes_each_component_without_changing_credentials() {
        let uri = super::postgres_uri(
            "db.example.test",
            5432,
            "a@b",
            "p%:/?#$'é",
            "db/name",
            "verify-full",
        )
        .unwrap();
        assert_eq!(
            uri,
            "postgresql://a%40b:p%25%3A%2F%3F%23%24%27%C3%A9@db.example.test:5432/db%2Fname?sslmode=verify-full"
        );
        assert!(super::postgres_uri("db.test/path", 5432, "u", "p", "db", "require").is_err());
    }
    use super::*;
    fn mealie(store: &Store) -> crate::core::Project {
        store
            .create(crate::core::CreateProject {
                name: "Recipe project".into(),
                server_id: "local".into(),
                apps: vec!["mealie".into()],
            })
            .unwrap()
    }
    #[test]
    fn dedicated_database_is_configured_from_recipe() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let project = mealie(&store);
        store
            .configure_database(
                &project.id,
                Selection {
                    engine: None,
                    mode: "dedicated".into(),
                    source_id: None,
                },
            )
            .unwrap();
        let setup = store.setup(&project.id).unwrap();
        assert_eq!(
            setup.compose["services"]["mealie"]["environment"]["DB_ENGINE"],
            "postgres"
        );
        assert_eq!(
            setup.compose["services"]["mealie"]["depends_on"]["selfhost-database"]["condition"],
            "service_healthy"
        );
        assert!(
            setup.compose["services"]["selfhost-database"]
                .get("ports")
                .is_none()
        );
        assert!(
            store
                .configure_database(
                    &project.id,
                    Selection {
                        engine: None,
                        mode: "dedicated".into(),
                        source_id: None
                    }
                )
                .is_err()
        );
    }
    #[test]
    fn shared_projects_have_distinct_credentials_and_no_admin_password() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let source = store
            .add_database_source(NewSource {
                engine: "postgres".into(),
                auth_database: String::new(),
                name: "Shared".into(),
                kind: "shared".into(),
                managed: true,
                server_id: "local".into(),
                host: String::new(),
                port: 5432,
                username: String::new(),
                password: String::new(),
                database: String::new(),
                ssl_mode: "require".into(),
            })
            .unwrap();
        let admin = store.database_source(&source).unwrap();
        let a = mealie(&store);
        let b = mealie(&store);
        for project in [&a, &b] {
            store
                .configure_database(
                    &project.id,
                    Selection {
                        engine: None,
                        mode: "shared".into(),
                        source_id: Some(source.clone()),
                    },
                )
                .unwrap();
            assert!(store.check_database_ready(&project.id).is_err());
            let setup = store.setup(&project.id).unwrap();
            assert!(
                !serde_json::to_string(&setup)
                    .unwrap()
                    .contains(&admin.password)
            );
            assert_eq!(
                setup.compose["networks"]["database-source"]["external"],
                true
            );
        }
        let a = store.setup(&a.id).unwrap();
        let b = store.setup(&b.id).unwrap();
        for key in ["DATABASE_NAME", "DATABASE_USER", "DATABASE_PASSWORD"] {
            assert_ne!(a.environment[key], b.environment[key]);
        }
        assert!(
            !serde_json::to_string(&store.database_sources().unwrap())
                .unwrap()
                .contains(&admin.password)
        );
    }
    #[test]
    fn sql_isolates_connect_permissions_and_quotes_passwords() {
        let sql = provisioning_sql("u_test", "p_test", "'quoted$body$");
        assert!(sql.contains("REVOKE CONNECT ON DATABASE \"p_test\" FROM PUBLIC"));
        assert!(sql.contains("NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION"));
        assert!(sql.contains("PASSWORD '''quoted$body$'"));
    }
}
