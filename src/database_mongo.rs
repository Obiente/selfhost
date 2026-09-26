//! MongoDB native connection, scoped initialization and portable archive commands.
//! Credential-bearing data travels through private stdin/config/environment, never argv.
use crate::database::Connection;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

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
fn database_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 63
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            && !["admin", "local", "config"].contains(&value),
        "Use an isolated MongoDB application database name"
    );
    Ok(())
}
pub(crate) fn uri(connection: &Connection) -> Result<String> {
    ensure!(connection.port > 0, "Invalid MongoDB port");
    ensure!(
        !connection.username.is_empty() && !connection.password.is_empty(),
        "MongoDB credentials are required"
    );
    ensure!(
        ["disable", "require", "verify-full"].contains(&connection.ssl_mode.as_str()),
        "Invalid MongoDB TLS mode"
    );
    ensure!(
        !connection.database.is_empty() && !connection.auth_database.is_empty(),
        "MongoDB database and authentication database are required"
    );
    let mut url = reqwest::Url::parse("mongodb://localhost")?;
    url.set_host(Some(&connection.host))
        .map_err(|_| anyhow::anyhow!("Invalid MongoDB host"))?;
    url.set_port(Some(connection.port))
        .map_err(|_| anyhow::anyhow!("Invalid MongoDB port"))?;
    url.set_username(&component(&connection.username))
        .map_err(|_| anyhow::anyhow!("Invalid MongoDB username"))?;
    url.set_password(Some(&component(&connection.password)))
        .map_err(|_| anyhow::anyhow!("Invalid MongoDB password"))?;
    url.set_path(&format!("/{}", component(&connection.database)));
    url.query_pairs_mut()
        .append_pair("authSource", &connection.auth_database)
        .append_pair(
            "tls",
            if connection.ssl_mode == "disable" {
                "false"
            } else {
                "true"
            },
        )
        .append_pair("serverSelectionTimeoutMS", "15000");
    // TLS never enables invalid certificate or hostname bypasses, even in require mode.
    Ok(url.to_string())
}

pub(crate) fn provisioning_script(
    connection: &Connection,
    user: &str,
    database: &str,
    password: &str,
) -> Result<String> {
    database_name(database)?;
    ensure!(
        !user.is_empty()
            && user.len() <= 63
            && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "Invalid MongoDB project username"
    );
    ensure!(!password.is_empty(), "MongoDB project password is required");
    let request =
        json!({"uri":uri(connection)?,"username":user,"database":database,"password":password});
    Ok(format!(
        r#"const request = {request};
const target = new Mongo(request.uri).getDB(request.database);
const existing = target.getUser(request.username);
const roles = [{{role: 'readWrite', db: request.database}}, {{role: 'dbAdmin', db: request.database}}];
if (existing) {{
  const owned = existing.customData && existing.customData.selfhostDatabase === request.database && existing.customData.selfhostUser === request.username;
  const scoped = existing.roles.length === 2 && existing.roles.every(r => r.db === request.database && ['readWrite', 'dbAdmin'].includes(r.role));
  if (!owned || !scoped) throw new Error('Existing MongoDB user is not owned by this project');
  target.updateUser(request.username, {{pwd: request.password}});
}} else {{
  if (target.getCollectionNames().length) throw new Error('Refusing to adopt a populated MongoDB database');
  target.createUser({{user: request.username, pwd: request.password, roles, customData: {{selfhostDatabase: request.database, selfhostUser: request.username}}}});
}}
"#
    ))
}

pub(crate) fn provisioning_command() -> Vec<String> {
    // The official image drops UID when its first argument is mongosh. A pipe
    // opened by Docker remains owned by root, so /dev/stdin then cannot be opened.
    // Keep the ephemeral client UID stable; the database account stays scoped.
    [
        "sh",
        "-ec",
        "exec mongosh \"$@\"",
        "selfhost-mongo-client",
        "--nodb",
        "--quiet",
        "--file",
        "/dev/stdin",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub(crate) fn archive_config(connection: &Connection) -> Result<String> {
    database_name(&connection.database)?;
    // JSON is valid YAML and prevents scalar/newline injection in the tools configuration.
    Ok(serde_json::to_string(&json!({"uri":uri(connection)?}))?)
}

pub(crate) fn archive_command(restore: bool, database: &str) -> Result<Vec<String>> {
    database_name(database)?;
    let command = if restore {
        // Explicit namespace prevents an archive from modifying another database.
        format!(
            "mongorestore --config=\"$config\" --archive --nsInclude='{database}.*' --drop --stopOnError --quiet"
        )
    } else {
        format!("mongodump --config=\"$config\" --db='{database}' --archive --quiet")
    };
    Ok(vec![
        "sh".into(),
        "-ec".into(),
        format!(
            "umask 077\nconfig=$(mktemp /tmp/selfhost-mongo.XXXXXXXX)\ntrap 'rm -f \"$config\"' EXIT HUP INT TERM\nprintf '%s' \"$SELFHOST_MONGO_CONFIG\" > \"$config\"\nunset SELFHOST_MONGO_CONFIG\n{command}"
        ),
    ])
}

pub(crate) fn service(
    driver: &Value,
    user_env: &str,
    password_env: &str,
    database_env: &str,
    root_password_env: Option<&str>,
) -> Result<Value> {
    for key in [user_env, password_env, database_env]
        .into_iter()
        .chain(root_password_env)
    {
        ensure!(
            !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
            "Invalid MongoDB environment reference"
        );
    }
    let reference = |key: &str| format!("${{{key}:?Set MongoDB credential}}");
    let mut result = json!({"image":driver["image"],"restart":"unless-stopped","volumes":[format!("database-data:{}",driver["data_path"].as_str().unwrap_or("/data/db"))],"healthcheck":driver["healthcheck"]});
    if let Some(root) = root_password_env {
        result["environment"] = json!({"MONGO_INITDB_ROOT_USERNAME":"selfhostroot","MONGO_INITDB_ROOT_PASSWORD":reference(root),"MONGO_INITDB_DATABASE":reference(database_env),"SELFHOST_APP_USER":reference(user_env),"SELFHOST_APP_PASSWORD":reference(password_env),"SELFHOST_APP_DATABASE":reference(database_env)});
        // Only static code is written; process.env handles credentials without shell interpolation.
        result["entrypoint"] = json!([
            "sh",
            "-ec",
            r#"cat > /docker-entrypoint-initdb.d/selfhost-app.js <<'SELFHOST_INIT'
const name = process.env.SELFHOST_APP_DATABASE;
const username = process.env.SELFHOST_APP_USER;
const target = db.getSiblingDB(name);
target.createUser({user: username, pwd: process.env.SELFHOST_APP_PASSWORD, roles: [{role: 'readWrite', db: name}, {role: 'dbAdmin', db: name}], customData: {selfhostDatabase: name, selfhostUser: username}});
SELFHOST_INIT
chmod 644 /docker-entrypoint-initdb.d/selfhost-app.js
exec /usr/local/bin/docker-entrypoint.sh mongod
"#
        ]);
    } else {
        result["environment"] = json!({"MONGO_INITDB_ROOT_USERNAME":reference(user_env),"MONGO_INITDB_ROOT_PASSWORD":reference(password_env),"MONGO_INITDB_DATABASE":reference(database_env)});
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connection() -> Connection {
        Connection {
            engine: "mongodb".into(),
            host: "db.example.test".into(),
            port: 27017,
            username: "a@b".into(),
            password: "p%:/?#'é".into(),
            database: "project_db".into(),
            ssl_mode: "verify-full".into(),
            auth_database: "admin".into(),
        }
    }
    #[test]
    fn uri_preserves_credentials_and_validates_tls() {
        let result = uri(&connection()).unwrap();
        assert!(result.contains("a%40b:p%25%3A%2F%3F%23%27%C3%A9@"));
        assert!(result.contains("authSource=admin&tls=true"));
        assert!(!result.contains("AllowInvalid"));
        let mut bad = connection();
        bad.host = "db.test/path".into();
        assert!(uri(&bad).is_err());
    }
    #[test]
    fn provision_is_scoped_and_does_not_adopt_other_users() {
        let script = provisioning_script(
            &connection(),
            "u_project",
            "project_db",
            "secret';\nthrow 1;//",
        )
        .unwrap();
        assert!(script.contains("selfhostDatabase"));
        assert!(script.contains("getCollectionNames().length"));
        assert!(!script.contains("readWriteAnyDatabase"));
        assert!(!script.contains("role: 'root'"));
        assert!(provisioning_script(&connection(), "user", "admin", "secret").is_err());
    }
    #[test]
    fn archive_credentials_stay_out_of_arguments_and_restore_is_scoped() {
        let command = archive_command(true, "project_db").unwrap();
        assert!(command[2].contains("--nsInclude='project_db.*'"));
        assert!(!command.join(" ").contains(&connection().password));
        assert!(command[2].contains("umask 077"));
        assert!(archive_command(true, "db'; echo unsafe").is_err());
        let config: Value = serde_json::from_str(&archive_config(&connection()).unwrap()).unwrap();
        assert_eq!(config["uri"], uri(&connection()).unwrap());
    }
    #[test]
    fn dedicated_credentials_are_separate_from_root() {
        let value = service(
            &json!({"image":"mongo:7.0.43","data_path":"/data/db"}),
            "APP_USER",
            "APP_PASSWORD",
            "APP_DB",
            Some("ROOT_PASSWORD"),
        )
        .unwrap();
        assert_eq!(
            value["environment"]["MONGO_INITDB_ROOT_USERNAME"],
            "selfhostroot"
        );
        assert_ne!(
            value["environment"]["MONGO_INITDB_ROOT_PASSWORD"],
            value["environment"]["SELFHOST_APP_PASSWORD"]
        );
        assert!(value.get("ports").is_none());
        assert!(service(&json!({}), "EVIL-KEY", "P", "D", None).is_err());
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        io::Write,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    fn docker(
        args: &[&str],
        env: &BTreeMap<String, String>,
        input: Option<&[u8]>,
    ) -> std::process::Output {
        let mut child = Command::new("docker")
            .args(args)
            .envs(env)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Docker process");
        if let Some(bytes) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(bytes)
                .expect("Docker input");
        }
        child.wait_with_output().expect("Docker exit")
    }
    fn shell(container: &str, script: &str) -> std::process::Output {
        docker(
            &[
                "exec",
                "-i",
                container,
                "mongosh",
                "--nodb",
                "--quiet",
                "--file",
                "/dev/stdin",
            ],
            &BTreeMap::new(),
            Some(script.as_bytes()),
        )
    }
    struct Fixture {
        name: String,
        volume: String,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let marker = docker(
                &[
                    "inspect",
                    "--format",
                    "{{index .Config.Labels \"org.obiente.selfhost.fixture\"}}",
                    &self.name,
                ],
                &BTreeMap::new(),
                None,
            );
            if String::from_utf8_lossy(&marker.stdout).trim() == self.name {
                let _ = docker(&["rm", "-f", &self.name], &BTreeMap::new(), None);
                let _ = docker(&["volume", "rm", &self.volume], &BTreeMap::new(), None);
            }
        }
    }
    #[test]
    #[ignore = "Runs an isolated local MongoDB container; requires Docker and pulls mongo:7.0.43"]
    fn mongo_scoped_init_provision_archive_restore_live() {
        let context = docker(&["context", "inspect"], &BTreeMap::new(), None);
        let context: Value = serde_json::from_slice(&context.stdout).unwrap();
        let host = context[0]["Endpoints"]["docker"]["Host"].as_str().unwrap();
        assert!(
            host.starts_with("npipe:") || host.starts_with("unix:"),
            "Local Docker only"
        );
        let name = format!("selfhost-mongo-fixture-{}", crate::core::token(6).unwrap());
        let fixture = Fixture {
            name: name.clone(),
            volume: format!("{name}-data"),
        };
        let root_password = crate::core::token(24).unwrap();
        let app_password = crate::core::token(24).unwrap();
        let vars: BTreeMap<String, String> = BTreeMap::from([
            ("APP_USER".into(), "u_fixture".into()),
            ("APP_PASSWORD".into(), app_password.clone()),
            ("APP_DB".into(), "p_fixture".into()),
            ("ROOT_PASSWORD".into(), root_password.clone()),
        ]);
        let service = service(
            &json!({"image":"mongo:7.0.43","data_path":"/data/db"}),
            "APP_USER",
            "APP_PASSWORD",
            "APP_DB",
            Some("ROOT_PASSWORD"),
        )
        .unwrap();
        let mut env = BTreeMap::new();
        for (key, value) in service["environment"].as_object().unwrap() {
            let value = value.as_str().unwrap();
            let resolved = if let Some(variable) =
                value.strip_prefix("${").and_then(|v| v.split(':').next())
            {
                vars[variable].clone()
            } else {
                value.into()
            };
            env.insert(key.clone(), resolved);
        }
        let mut args = vec![
            "run".to_string(),
            "-d".into(),
            "--name".into(),
            name.clone(),
            "--label".into(),
            format!("org.obiente.selfhost.fixture={name}"),
            "--volume".into(),
            format!("{}:/data/db", fixture.volume),
            "--entrypoint".into(),
            "sh".into(),
        ];
        for key in env.keys() {
            args.extend(["--env".into(), key.clone()]);
        }
        args.extend([
            "mongo:7.0.43".into(),
            "-ec".into(),
            service["entrypoint"][2].as_str().unwrap().into(),
        ]);
        let started = docker(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            &env,
            None,
        );
        assert!(started.status.success(), "Mongo fixture start");
        let admin = Connection {
            engine: "mongodb".into(),
            host: "127.0.0.1".into(),
            port: 27017,
            username: "selfhostroot".into(),
            password: root_password,
            database: "admin".into(),
            ssl_mode: "disable".into(),
            auth_database: "admin".into(),
        };
        let app = Connection {
            engine: "mongodb".into(),
            host: "127.0.0.1".into(),
            port: 27017,
            username: "u_fixture".into(),
            password: app_password,
            database: "p_fixture".into(),
            ssl_mode: "disable".into(),
            auth_database: "p_fixture".into(),
        };
        let connect = format!(
            "const db = new Mongo({}).getDB('p_fixture');\n",
            serde_json::to_string(&uri(&app).unwrap()).unwrap()
        );
        let ready_script = format!(
            "const admin = new Mongo({}).getDB('admin'); if(admin.runCommand({{getCmdLineOpts:1}}).parsed.security?.authorization !== 'enabled') quit(3);\n{connect}if(db.runCommand({{ping:1}}).ok!==1) quit(2);",
            serde_json::to_string(&uri(&admin).unwrap()).unwrap()
        );
        let mut ready = false;
        for _ in 0..90 {
            if shell(&name, &ready_script).status.success() {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
        assert!(ready, "Scoped Mongo account initialized");
        let driver: Value =
            serde_json::from_slice(&std::fs::read("catalog/databases/mongodb.json").unwrap())
                .unwrap();
        let mut health_args = vec!["exec", name.as_str()];
        health_args.extend(
            driver["healthcheck"]["test"].as_array().unwrap()[1..]
                .iter()
                .map(|v| v.as_str().unwrap()),
        );
        assert!(
            docker(&health_args, &BTreeMap::new(), None)
                .status
                .success(),
            "Declared Mongo readiness check"
        );
        assert!(shell(&name,&format!("{connect}db.fixture.insertOne({{marker:'retained'}});let denied=false;try{{db.getSiblingDB('admin').createUser({{user:'forbidden',pwd:'forbidden',roles:['root']}})}}catch(error){{denied=error.code===13}}if(!denied)quit(9);")).status.success(),"App account cannot administer server");
        let second_password = crate::core::token(24).unwrap();
        let provision =
            provisioning_script(&admin, "u_second", "p_second", &second_password).unwrap();
        assert!(
            shell(&name, &provision).status.success(),
            "Provision second isolated account"
        );
        assert!(
            shell(&name, &provision).status.success(),
            "Owned provisioning is idempotent"
        );
        assert!(shell(&name,&format!("{connect}let denied=false;try{{db.getSiblingDB('p_second').fixture.findOne()}}catch(error){{denied=error.code===13}}if(!denied)quit(9);")).status.success(),"Cross-project reads denied");
        let admin_connect = format!(
            "const db = new Mongo({}).getDB('occupied');",
            serde_json::to_string(&uri(&admin).unwrap()).unwrap()
        );
        assert!(
            shell(
                &name,
                &format!("{admin_connect}db.fixture.insertOne({{owned:false}})")
            )
            .status
            .success()
        );
        assert!(
            !shell(
                &name,
                &provisioning_script(&admin, "u_occupied", "occupied", "secret").unwrap()
            )
            .status
            .success(),
            "Populated database cannot be adopted"
        );
        let backup_env = BTreeMap::from([(
            "SELFHOST_MONGO_CONFIG".into(),
            archive_config(&app).unwrap(),
        )]);
        let archive = |restore: bool, input: Option<&[u8]>| {
            let command = archive_command(restore, "p_fixture").unwrap();
            let mut args = vec![
                "exec",
                "-i",
                "--env",
                "SELFHOST_MONGO_CONFIG",
                name.as_str(),
            ];
            args.extend(command.iter().map(String::as_str));
            docker(&args, &backup_env, input)
        };
        let dump = archive(false, None);
        assert!(
            dump.status.success() && dump.stdout.len() > 20,
            "Scoped mongodump archive"
        );
        assert!(
            shell(&name, &format!("{connect}db.fixture.drop();"))
                .status
                .success()
        );
        assert!(
            archive(true, Some(&dump.stdout)).status.success(),
            "Scoped mongorestore archive"
        );
        assert!(
            shell(
                &name,
                &format!("{connect}if(db.fixture.findOne().marker!=='retained')quit(9);")
            )
            .status
            .success(),
            "Restored marker"
        );
        assert!(
            docker(&["restart", &name], &BTreeMap::new(), None)
                .status
                .success()
        );
        for _ in 0..45 {
            if shell(
                &name,
                &format!("{connect}if(db.fixture.findOne().marker!=='retained')quit(9);"),
            )
            .status
            .success()
            {
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
        panic!("Mongo persisted account/data after restart");
    }
}
