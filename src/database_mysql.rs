//! MySQL-family clients. Secrets enter private container option files, never argv.
use crate::database::{ClientCommand, Connection};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn go_dsn(connection: &Connection) -> Result<String> {
    ensure!(
        !connection.username.contains([':', '\0', '\r', '\n']),
        "Go MySQL adapters cannot represent a username containing a colon or line break"
    );
    let uri = crate::database::connection_uri(connection)?;
    let url = reqwest::Url::parse(&uri)?;
    let host = url.host_str().context("Missing database host")?;
    let database = connection
        .database
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    Ok(format!(
        "{}:{}@tcp({host}:{})/{database}?charset=utf8mb4&parseTime=true&loc=UTC&tls={}",
        connection.username,
        connection.password,
        connection.port,
        connection.ssl_mode != "disable"
    ))
}

pub(crate) fn service(
    driver: &Value,
    user: &str,
    password: &str,
    database: &str,
    root_password: Option<&str>,
) -> Result<Value> {
    let engine = driver["engine"]
        .as_str()
        .context("Missing database engine")?;
    ensure!(
        ["mysql", "mariadb"].contains(&engine),
        "Unsupported SQL engine"
    );
    let prefix = if engine == "mysql" {
        "MYSQL"
    } else {
        "MARIADB"
    };
    let mut environment = serde_json::Map::new();
    let root = root_password.unwrap_or(password);
    environment.insert(
        format!("{prefix}_ROOT_PASSWORD"),
        json!(format!("${{{root}:?Set database administrator password}}")),
    );
    // The official image grants the bootstrap root account access on the private
    // source network. It is never copied into a consuming application's project.
    if root_password.is_none() {
        environment.insert(format!("{prefix}_ROOT_HOST"), json!("%"));
    }
    environment.insert(
        format!("{prefix}_DATABASE"),
        json!(format!("${{{database}:?Set database name}}")),
    );
    if root_password.is_some() {
        environment.insert(
            format!("{prefix}_USER"),
            json!(format!("${{{user}:?Set database user}}")),
        );
        environment.insert(
            format!("{prefix}_PASSWORD"),
            json!(format!("${{{password}:?Set database password}}")),
        );
    }
    Ok(
        json!({"image":driver["image"],"restart":"unless-stopped","environment":environment,"volumes":[format!("database-data:{}",driver["data_path"].as_str().context("Missing database data path")?)],"healthcheck":driver["healthcheck"]}),
    )
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use std::{
        io::Write,
        process::{Command, Output, Stdio},
        thread,
        time::Duration,
    };
    fn docker(args: &[String], env: &BTreeMap<String, String>, input: Option<&[u8]>) -> Output {
        let mut child = Command::new("docker")
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = input {
            child.stdin.take().unwrap().write_all(input).unwrap();
        } else {
            drop(child.stdin.take());
        }
        child.wait_with_output().unwrap()
    }
    fn plain(args: &[&str]) -> Output {
        docker(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &BTreeMap::new(),
            None,
        )
    }
    struct Fixture {
        name: String,
        volume: String,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let check = plain(&[
                "inspect",
                "--format",
                "{{index .Config.Labels \"org.obiente.selfhost.fixture\"}}",
                &self.name,
            ]);
            if String::from_utf8_lossy(&check.stdout).trim() == self.name {
                plain(&["rm", "-f", &self.name]);
                plain(&["volume", "rm", &self.volume]);
            }
        }
    }
    fn execute(name: &str, c: &Connection, operation: &str, input: Option<&[u8]>) -> Output {
        let client = client(c, operation, None).unwrap();
        let mut args = vec!["exec".into(), "-i".into()];
        for key in client.environment.keys() {
            args.extend(["--env".into(), key.clone()]);
        }
        args.push(name.into());
        args.extend(client.argv);
        docker(&args, &client.environment, input)
    }
    #[test]
    #[ignore = "Starts isolated local MySQL and MariaDB containers with their own volumes"]
    fn mysql_and_mariadb_init_isolation_archive_restore_live() {
        let context: Value =
            serde_json::from_slice(&plain(&["context", "inspect"]).stdout).unwrap();
        let host = context[0]["Endpoints"]["docker"]["Host"].as_str().unwrap();
        assert!(
            host.starts_with("npipe:") || host.starts_with("unix:"),
            "Local Docker only"
        );
        for engine in ["mysql", "mariadb"] {
            let driver = crate::catalog::database_driver(engine).unwrap();
            let name = format!("selfhost-sql-fixture-{}", crate::core::token(6).unwrap());
            let fixture = Fixture {
                name: name.clone(),
                volume: format!("{name}-data"),
            };
            let root_password = crate::core::token(24).unwrap();
            let password = crate::core::token(24).unwrap();
            let vars = BTreeMap::from([
                ("APP_USER", "u_fixture".to_owned()),
                ("APP_PASSWORD", password.clone()),
                ("APP_DB", "p_fixture".to_owned()),
                ("ROOT_PASSWORD", root_password.clone()),
            ]);
            let service = service(
                &driver,
                "APP_USER",
                "APP_PASSWORD",
                "APP_DB",
                Some("ROOT_PASSWORD"),
            )
            .unwrap();
            let mut env = BTreeMap::new();
            for (key, value) in service["environment"].as_object().unwrap() {
                let value = value.as_str().unwrap();
                let resolved =
                    if let Some(key) = value.strip_prefix("${").and_then(|s| s.split(':').next()) {
                        vars[key].clone()
                    } else {
                        value.to_string()
                    };
                env.insert(key.clone(), resolved);
            }
            let mut args = vec![
                "run".into(),
                "-d".into(),
                "--name".into(),
                name.clone(),
                "--label".into(),
                format!("org.obiente.selfhost.fixture={name}"),
                "--volume".into(),
                format!("{}:/var/lib/mysql", fixture.volume),
            ];
            for key in env.keys() {
                args.extend(["--env".into(), key.clone()]);
            }
            args.push(driver["image"].as_str().unwrap().into());
            assert!(docker(&args, &env, None).status.success(), "Start {engine}");
            let app = Connection {
                engine: engine.into(),
                host: "127.0.0.1".into(),
                port: 3306,
                username: "u_fixture".into(),
                password,
                database: "p_fixture".into(),
                ssl_mode: "disable".into(),
                auth_database: String::new(),
            };
            let admin = Connection {
                engine: engine.into(),
                host: "127.0.0.1".into(),
                port: 3306,
                username: "root".into(),
                password: root_password,
                database: "mysql".into(),
                ssl_mode: "disable".into(),
                auth_database: String::new(),
            };
            let mut ready = false;
            let mut last_error = String::new();
            for _ in 0..100 {
                let result = execute(&name, &app, "restore", Some(b"SELECT 1;"));
                if result.status.success() {
                    ready = true;
                    break;
                }
                last_error =
                    String::from_utf8_lossy(&result.stderr).replace(&app.password, "[redacted]");
                thread::sleep(Duration::from_secs(1));
            }
            assert!(ready, "Initialized scoped {engine} account: {last_error}");
            assert!(execute(&name,&app,"restore",Some(b"CREATE TABLE fixture (marker varchar(32)); INSERT INTO fixture VALUES ('retained');")).status.success());
            assert!(
                !execute(&name, &app, "restore", Some(b"SELECT * FROM mysql.user;"))
                    .status
                    .success(),
                "No administrator access"
            );
            let sql =
                provisioning_sql("u_second", "p_second", &crate::core::token(24).unwrap()).unwrap();
            assert!(
                execute(&name, &admin, "provision", Some(sql.as_bytes()))
                    .status
                    .success(),
                "Create isolated second database"
            );
            assert!(
                !execute(&name, &admin, "provision", Some(sql.as_bytes()))
                    .status
                    .success(),
                "Refuse collision instead of resetting credentials"
            );
            assert!(
                !execute(
                    &name,
                    &app,
                    "restore",
                    Some(b"CREATE TABLE p_second.forbidden (id int);")
                )
                .status
                .success(),
                "No cross-project writes"
            );
            let dump = execute(&name, &app, "dump", None);
            assert!(
                dump.status.success() && dump.stdout.len() > 20,
                "Scoped {engine} dump: {}",
                String::from_utf8_lossy(&dump.stderr)
            );
            assert!(
                execute(&name, &app, "restore", Some(b"DROP TABLE fixture;"))
                    .status
                    .success()
            );
            assert!(
                execute(&name, &app, "restore", Some(&dump.stdout))
                    .status
                    .success(),
                "Restore {engine}"
            );
            assert!(
                String::from_utf8_lossy(
                    &execute(&name, &app, "restore", Some(b"SELECT marker FROM fixture;")).stdout
                )
                .contains("retained")
            );
            assert!(plain(&["restart", &name]).status.success());
            let mut retained = false;
            for _ in 0..60 {
                if String::from_utf8_lossy(
                    &execute(&name, &app, "restore", Some(b"SELECT marker FROM fixture;")).stdout,
                )
                .contains("retained")
                {
                    retained = true;
                    break;
                }
                thread::sleep(Duration::from_secs(1));
            }
            assert!(retained, "Persistent {engine} data");
        }
    }
}

fn quote_option(value: &str) -> Result<String> {
    ensure!(
        !value.contains(['\0', '\n', '\r']),
        "Database options must be single lines"
    );
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

pub(crate) fn client(
    connection: &Connection,
    operation: &str,
    input: Option<Vec<u8>>,
) -> Result<ClientCommand> {
    let maria = connection.engine == "mariadb";
    ensure!(
        maria || connection.engine == "mysql",
        "Invalid SQL client engine"
    );
    let driver = crate::catalog::database_driver(&connection.engine)?;
    let mut config = format!(
        "[client]\nprotocol=tcp\nhost={}\nport={}\nuser={}\npassword={}\n",
        quote_option(&connection.host)?,
        connection.port,
        quote_option(&connection.username)?,
        quote_option(&connection.password)?
    );
    if operation != "dump" {
        config.push_str("connect-timeout=15\n");
    }
    if maria {
        config.push_str(match connection.ssl_mode.as_str() {
            "disable" => "skip-ssl\n",
            // MariaDB has no equivalent of MySQL's TLS-without-verification mode
            // that we can safely rely on across clients. Require verified TLS.
            "require" | "verify-full" => "ssl\nssl-verify-server-cert\n",
            _ => anyhow::bail!("Invalid database TLS mode"),
        });
    } else {
        if connection.ssl_mode == "disable" {
            config.push_str("get-server-public-key=1\n");
        }
        config.push_str(&format!(
            "ssl-mode={}\n",
            match connection.ssl_mode.as_str() {
                "disable" => "DISABLED",
                "require" => "REQUIRED",
                "verify-full" => "VERIFY_IDENTITY",
                _ => anyhow::bail!("Invalid database TLS mode"),
            }
        ));
    }
    let program = match (maria, operation) {
        (true, "dump") => "mariadb-dump",
        (false, "dump") => "mysqldump",
        (true, _) => "mariadb",
        (false, _) => "mysql",
    };
    // A static shell wrapper writes the option file from one quoted env value.
    // Positional arguments are forwarded without shell interpretation.
    let mut argv=vec!["sh".into(),"-ec".into(),"umask 077; printf '%s' \"$SELFHOST_SQL_OPTIONS\" > /tmp/selfhost-client.cnf; unset SELFHOST_SQL_OPTIONS; exec \"$@\"".into(),"selfhost-sql-client".into(),program.into(),"--defaults-extra-file=/tmp/selfhost-client.cnf".into()];
    if operation == "dump" {
        argv.extend(
            [
                "--lock-tables",
                "--quick",
                "--routines",
                "--events",
                "--triggers",
                "--hex-blob",
                "--no-tablespaces",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        if !maria {
            argv.push("--set-gtid-purged=OFF".into());
        }
        argv.extend(["--".into(), connection.database.clone()]);
    } else {
        argv.extend([
            "--batch".into(),
            "--binary-mode".into(),
            format!("--database={}", connection.database),
        ]);
    }
    Ok(ClientCommand {
        image: driver["image"]
            .as_str()
            .context("Missing database image")?
            .into(),
        argv,
        environment: BTreeMap::from([("SELFHOST_SQL_OPTIONS".into(), config)]),
        input,
    })
}

pub(crate) fn provisioning_sql(user: &str, database: &str, password: &str) -> Result<String> {
    ensure!(
        crate::setup::slug(user) && crate::setup::slug(database),
        "Invalid generated database identity"
    );
    ensure!(
        !password.contains(['\0', '\n', '\r']),
        "Invalid generated database password"
    );
    // CREATE intentionally fails on collisions. An interrupted operation is
    // journalled centrally and cannot silently reset or adopt an existing user.
    Ok(format!(
        "SET SESSION sql_mode='NO_BACKSLASH_ESCAPES';\nCREATE USER '{user}'@'%' IDENTIFIED BY '{}';\nCREATE DATABASE `{database}` CHARACTER SET utf8mb4;\nGRANT ALL PRIVILEGES ON `{database}`.* TO '{user}'@'%';\n",
        password.replace('\'', "''")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provisioning_never_resets_existing_users_and_quotes_password() {
        let sql = provisioning_sql("u_fixture", "p_fixture", "'\\$secret").unwrap();
        assert!(sql.contains("IDENTIFIED BY '''\\$secret'"));
        assert!(!sql.contains("ALTER USER"));
        assert!(!sql.contains("IF NOT EXISTS"));
        assert!(provisioning_sql("u;bad", "p_good", "x").is_err());
    }
    #[test]
    fn passwords_are_only_in_private_option_file_environment() {
        let c = Connection {
            engine: "mysql".into(),
            host: "db.example.test".into(),
            port: 3306,
            username: "user".into(),
            password: "p'$\\\"secret".into(),
            database: "app".into(),
            ssl_mode: "verify-full".into(),
            auth_database: String::new(),
        };
        let command = client(&c, "dump", None).unwrap();
        assert!(!command.argv.join(" ").contains(&c.password));
        assert!(command.environment["SELFHOST_SQL_OPTIONS"].contains("ssl-mode=VERIFY_IDENTITY"));
        assert!(command.argv.contains(&"--lock-tables".into()));
    }
}
