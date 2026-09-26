//! Declarative app configuration and management. App-specific paths live in profiles.
use crate::core::{Store, atomic_write, token};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub minimum: Option<i64>,
    #[serde(default)]
    pub maximum: Option<i64>,
    #[serde(default)]
    pub minimum_length: usize,
    #[serde(default)]
    pub maximum_length: Option<usize>,
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub argv: Vec<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub label: String,
    pub command: Command,
    #[serde(default)]
    pub skip_if: Option<Command>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub description: String,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub schedulable: bool,
    #[serde(default)]
    pub inputs: Vec<Field>,
    pub steps: Vec<Step>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    ComposeEnvironment,
    Command {
        read: Command,
        write: Command,
        scope: String,
        #[serde(default)]
        null_deletes: bool,
    },
    Yaml {
        file: String,
        mount: String,
        #[serde(default)]
        argument: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub warnings: Vec<String>,
    pub schema: u32,
    pub name: String,
    pub documentation: String,
    pub driver: Driver,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub oidc_client: Option<crate::connections::OidcClient>,
    #[serde(default)]
    pub oidc_provider: Option<crate::connections::OidcProvider>,
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "Unsupported integration schema");
        let mut ids = std::collections::HashSet::new();
        for f in &self.fields {
            ensure!(
                crate::setup::slug(&f.id)
                    && ids.insert(&f.id)
                    && (!f.path.is_empty()
                        || (f.kind == "json" && matches!(self.driver, Driver::Yaml { .. }))),
                "Invalid or duplicate setting"
            );
            validate_field(f)?;
        }
        ids.clear();
        for a in &self.actions {
            ensure!(
                crate::setup::slug(&a.id) && ids.insert(&a.id) && !a.steps.is_empty(),
                "Invalid action"
            );
            let mut keys = std::collections::HashSet::new();
            for f in &a.inputs {
                ensure!(
                    crate::setup::slug(&f.id) && keys.insert(&f.id),
                    "Duplicate action input"
                );
                validate_field(f)?;
            }
            ensure!(
                !a.schedulable || a.inputs.iter().all(|f| f.default.is_some()),
                "Scheduled actions need defaults for every input"
            );
            for step in &a.steps {
                validate_command(&step.command)?;
                if let Some(c) = &step.skip_if {
                    validate_command(c)?;
                }
            }
        }
        match &self.driver {
            Driver::ComposeEnvironment => {
                for field in &self.fields {
                    ensure!(
                        field.path.len() == 1 && environment_key(&field.path[0]),
                        "Compose fields require one environment key"
                    );
                }
            }
            Driver::Command { read, write, .. } => {
                validate_command(read)?;
                validate_command(write)?;
            }
            Driver::Yaml { file, mount, .. } => ensure!(
                !file.is_empty()
                    && !file.contains(['/', '\\'])
                    && !file.starts_with('.')
                    && mount.starts_with('/')
                    && !mount.contains([':', '$']),
                "Invalid configuration file path"
            ),
        }
        if let Some(client) = &self.oidc_client {
            if client.has_environment() {
                ensure!(
                    matches!(self.driver, Driver::ComposeEnvironment) && client.action.is_empty(),
                    "Environment OIDC contracts require the Compose environment driver"
                );
                ensure!(
                    client
                        .environment
                        .keys()
                        .chain(client.environment_json.keys())
                        .chain(client.administrator_environment.keys())
                        .all(|k| environment_key(k)),
                    "Invalid OIDC environment key"
                );
                ensure!(
                    client
                        .environment_json
                        .keys()
                        .all(|k| !client.environment.contains_key(k)
                            && !client.administrator_environment.contains_key(k)),
                    "OIDC environment keys must be unique across template formats"
                );
                ensure!(
                    client.administrator_environment.is_empty()
                        == client.administrator_role.is_empty(),
                    "Administrator mapping must declare its role"
                );
            } else {
                ensure!(
                    client.administrator_environment.is_empty(),
                    "Administrator mappings require environment configuration"
                );
                let action = self
                    .actions
                    .iter()
                    .find(|a| a.id == client.action)
                    .context("OIDC client action is missing")?;
                ensure!(
                    action
                        .inputs
                        .iter()
                        .all(|f| client.inputs.contains_key(&f.id) || f.default.is_some()),
                    "OIDC client mapping misses action inputs"
                );
            }
        }
        Ok(())
    }
}
pub(crate) fn environment_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
/// Preserve Compose interpolation and unrelated keys; never read or rewrite .env here.
pub(crate) fn compose_environment(config: &Value) -> Result<BTreeMap<String, String>> {
    match config.get("environment") {
        None => Ok(BTreeMap::new()),
        Some(Value::Object(values)) => values.iter().map(|(k,v)| Ok((k.clone(), v.as_str().context("Use explicit string environment values before editing app configuration")?.into()))).collect(),
        Some(Value::Array(values)) => values.iter().map(|v| { let (k,v) = v.as_str().and_then(|s|s.split_once('=')).context("Use explicit KEY=value environment entries")?; Ok((k.into(),v.into())) }).collect(),
        _ => anyhow::bail!("Use a Compose environment mapping or list")
    }
}
fn validate_field(f: &Field) -> Result<()> {
    ensure!(
        [
            "string",
            "secret",
            "integer",
            "boolean",
            "string_list",
            "arguments",
            "json"
        ]
        .contains(&f.kind.as_str()),
        "Unsupported field type"
    );
    ensure!(
        f.minimum.zip(f.maximum).is_none_or(|(min, max)| min <= max),
        "Invalid field range"
    );
    if let Some(default) = &f.default {
        validate_value(f, default)?;
    }
    Ok(())
}
fn validate_command(c: &Command) -> Result<()> {
    ensure!(
        !c.argv.is_empty()
            && c.argv.len() <= 64
            && c.argv.iter().all(|a| a.len() <= 4096 && !a.contains('\0')),
        "Invalid integration command"
    );
    ensure!(
        c.user
            .as_ref()
            .is_none_or(|s| !s.starts_with('-') && !s.contains('\0')),
        "Invalid execution user"
    );
    Ok(())
}
fn validate_value(field: &Field, value: &Value) -> Result<()> {
    let ok = match field.kind.as_str() {
        "string" | "secret" => value.as_str().is_some_and(|s| {
            s.len() <= 4096
                && s.chars().count() >= field.minimum_length
                && field
                    .maximum_length
                    .is_none_or(|max| s.chars().count() <= max)
                && !s.contains('\0')
        }),
        "integer" => value.as_i64().is_some_and(|v| {
            field.minimum.is_none_or(|min| v >= min) && field.maximum.is_none_or(|max| v <= max)
        }),
        "boolean" => value.is_boolean(),
        "json" => {
            (value.is_object() || value.is_array())
                && serde_json::to_vec(value)?.len() <= 128 * 1024
        }
        "string_list" | "arguments" => value.as_array().is_some_and(|a| {
            a.len() <= 64
                && a.iter().all(|v| {
                    v.as_str()
                        .is_some_and(|s| s.len() <= 4096 && !s.contains('\0'))
                })
        }),
        _ => false,
    };
    ensure!(ok, "Invalid value for {}", field.label);
    ensure!(
        field.choices.is_empty()
            || value
                .as_str()
                .is_some_and(|v| field.choices.iter().any(|c| c == v)),
        "Choose a supported value for {}",
        field.label
    );
    Ok(())
}
fn get<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}
fn set(value: &mut Value, path: &[String], next: Value) -> Result<()> {
    if path.is_empty() {
        ensure!(
            next.is_object() || next.is_array(),
            "Root configuration must be an object or array"
        );
        *value = next;
        return Ok(());
    }
    if value.is_null() {
        *value = json!({});
    }
    let map = value
        .as_object_mut()
        .context("Setting parent must be a mapping")?;
    if path.len() == 1 {
        map.insert(path[0].clone(), next);
    } else {
        set(
            map.entry(path[0].clone()).or_insert(json!({})),
            &path[1..],
            next,
        )?;
    }
    Ok(())
}
fn fingerprint(value: &Value, profile: &Profile) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(value, profile))?)
    ))
}
fn remove(value: &mut Value, path: &[String]) {
    if let Some(map) = value.as_object_mut() {
        if path.len() == 1 {
            map.remove(&path[0]);
        } else if let Some(next) = map.get_mut(&path[0]) {
            remove(next, &path[1..]);
        }
    }
}
fn validate_change(profile: &Profile, field: &Field, value: &Value, restore: bool) -> Result<()> {
    if restore && value.is_null() {
        ensure!(
            matches!(
                profile.driver,
                Driver::Yaml { .. }
                    | Driver::ComposeEnvironment
                    | Driver::Command {
                        null_deletes: true,
                        ..
                    }
            ),
            "This integration cannot restore absent settings"
        );
        Ok(())
    } else {
        validate_value(field, value)
    }
}
pub(crate) fn expand(text: &str, inputs: &BTreeMap<String, Value>) -> Result<String> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let end = rest.find("}}").context("Unclosed action input")?;
        let value = inputs
            .get(&rest[..end])
            .context("Unresolved action input")?;
        output.push_str(
            &value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string()),
        );
        rest = &rest[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}
fn arguments(command: &Command, inputs: &BTreeMap<String, Value>) -> Result<Vec<String>> {
    let mut args = Vec::new();
    for arg in &command.argv {
        if let Some(key) = arg.strip_prefix("{{").and_then(|s| s.strip_suffix("}}"))
            && let Some(Value::Array(values)) = inputs.get(key)
        {
            for v in values {
                args.push(v.as_str().context("Arguments must be strings")?.into());
            }
            continue;
        }
        args.push(expand(arg, inputs)?);
    }
    Ok(args)
}
impl Store {
    fn guard_interpolated_fields(
        &self,
        id: &str,
        service: &str,
        profile: &Profile,
        changes: &BTreeMap<String, Value>,
    ) -> Result<()> {
        if matches!(profile.driver, Driver::ComposeEnvironment) {
            let setup = self.setup(id)?;
            let raw = compose_environment(&setup.compose["services"][service])?;
            for field in profile
                .fields
                .iter()
                .filter(|f| changes.contains_key(&f.id))
            {
                ensure!(
                    !raw.get(&field.path[0])
                        .is_some_and(|value| value.replace("$$", "").contains('$')),
                    "{} uses Compose interpolation; edit its .env variable or Compose source directly to preserve interpolation",
                    field.label
                );
            }
        }
        Ok(())
    }
    fn integration_revision(&self, id: &str, value: &Value, profile: &Profile) -> Result<String> {
        fingerprint(
            &json!({"config":value,"project":self.project(id)?,"setup":self.setup(id)?,"directory_revision":self.standalone.as_ref().map(|d| d.revision()).transpose()?}),
            profile,
        )
    }
    pub fn integration(&self, id: &str, service: &str) -> Result<Profile> {
        let project = self.project(id)?;
        project
            .services
            .iter()
            .find(|s| s.app == service)
            .and_then(|s| s.definition.integration.clone())
            .context("This service has no app integration")
    }
    async fn integration_command(
        &self,
        id: &str,
        service: &str,
        command: &Command,
        inputs: &BTreeMap<String, Value>,
        stdin: Option<Vec<u8>>,
    ) -> Result<String> {
        let project = self.project(id)?;
        let container = self
            .service_container(id, service)
            .await?
            .context("Start the service first")?;
        let mut cmd = self.project_docker(&project, true)?;
        cmd.args(["exec", "-i"]);
        if let Some(user) = &command.user {
            cmd.args(["--user", user]);
        }
        for (key, template) in &command.environment {
            ensure!(
                key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
                "Invalid environment name"
            );
            cmd.args(["--env", key]);
            cmd.env(key, expand(template, inputs)?);
        }
        cmd.arg(&container)
            .args(arguments(command, inputs)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let mut input = child.stdin.take().context("Missing command input")?;
        let output = child.stdout.take().context("Missing command output")?;
        let (status,bytes)=tokio::time::timeout(Duration::from_secs(300),async {
   let send=async {if let Some(data)=stdin{input.write_all(&data).await?;}drop(input);Ok::<_,std::io::Error>(())};
   let read=async {let mut bytes=Vec::new();output.take(1048577).read_to_end(&mut bytes).await?;ensure!(bytes.len()<=1024*1024,"Command output exceeds 1 MiB");Ok::<_,anyhow::Error>(bytes)};
   let (_,status,bytes)=tokio::try_join!(async{Ok::<_,anyhow::Error>(send.await?)},async{Ok(child.wait().await?)},read)?;Ok::<_,anyhow::Error>((status,bytes))
  }).await.context("Action timed out. A container command may still be running; inspect app state before retrying")??;
        ensure!(
            status.success(),
            "App command failed. Check its requirements and application logs."
        );
        Ok(String::from_utf8(bytes)?)
    }
    async fn integration_document(
        &self,
        id: &str,
        service: &str,
        profile: &Profile,
    ) -> Result<Value> {
        match &profile.driver {
            Driver::ComposeEnvironment => {
                let setup = self.setup(id)?;
                let values = compose_environment(&setup.compose["services"][service])?;
                Ok(Value::Object(
                    values
                        .into_iter()
                        .map(|(k, v)| (k, json!(v.replace("$$", "$"))))
                        .collect(),
                ))
            }
            Driver::Command { read, .. } => Ok(serde_json::from_str(
                &self
                    .integration_command(id, service, read, &BTreeMap::new(), None)
                    .await?,
            )?),
            Driver::Yaml {
                file,
                mount,
                argument,
            } => {
                let setup = self.setup(id)?;
                let service_config = &setup.compose["services"][service];
                if !argument.is_empty() {
                    let command = service_config["command"].as_array().context(
                        "Declare the app configuration argument in a Compose command array",
                    )?;
                    let args: Vec<_> = command.iter().filter_map(Value::as_str).collect();
                    let count = args
                        .iter()
                        .filter(|a| **a == argument || a.starts_with(&format!("{argument}=")))
                        .count();
                    ensure!(
                        count == 1
                            && (args.windows(2).any(|a| a[0] == argument && a[1] == mount)
                                || args.contains(&format!("{argument}={mount}").as_str())),
                        "Use one {argument} {mount} argument; layered configuration files need a custom integration"
                    );
                }
                for field in &profile.fields {
                    if let Some(env) = &field.environment {
                        let environment = &service_config["environment"];
                        let shadows = environment.get(env).is_some()
                            || environment.as_array().is_some_and(|entries| {
                                entries.iter().any(|v| {
                                    v.as_str().is_some_and(|v| {
                                        v == env || v.starts_with(&format!("{env}="))
                                    })
                                })
                            });
                        ensure!(
                            !shadows && service_config.get("env_file").is_none(),
                            "{} is controlled by environment overrides; remove the override or edit the Compose environment first",
                            field.label
                        );
                    }
                }
                let expected = format!("./files/{file}:{mount}:ro");
                ensure!(
                    service_config["volumes"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|m| m.as_str() == Some(&expected)
                            || (m["type"] == "bind"
                                && m["source"] == format!("./files/{file}")
                                && m["target"] == *mount
                                && m["read_only"] == true))),
                    "Mount ./files/{file} at {mount} read-only before editing app settings"
                );
                let text = setup
                    .files
                    .get(file)
                    .context("Configuration file is not declared in setup")?;
                let value: Value = serde_yaml::from_str(text)?;
                ensure!(
                    value.is_object()
                        || (value.is_array()
                            && profile
                                .fields
                                .iter()
                                .any(|f| f.path.is_empty() && f.kind == "json")),
                    "Configuration must match its declared YAML shape"
                );
                Ok(value)
            }
        }
    }
    pub async fn integration_state(&self, id: &str, service: &str) -> Result<Value> {
        let profile = self.integration(id, service)?;
        let current = self.integration_document(id, service, &profile).await?;
        let mut values = BTreeMap::new();
        for f in &profile.fields {
            if f.kind != "secret" {
                values.insert(
                    &f.id,
                    get(&current, &f.path).cloned().unwrap_or(Value::Null),
                );
            }
        }
        Ok(
            json!({"profile":profile,"values":values,"revision":self.integration_revision(id,&current,&profile)?}),
        )
    }
    pub async fn integration_plan(
        &self,
        id: &str,
        service: &str,
        changes: &BTreeMap<String, Value>,
    ) -> Result<Value> {
        self.integration_plan_inner(id, service, changes, false)
            .await
    }
    async fn integration_plan_inner(
        &self,
        id: &str,
        service: &str,
        changes: &BTreeMap<String, Value>,
        restore: bool,
    ) -> Result<Value> {
        let profile = self.integration(id, service)?;
        let current = self.integration_document(id, service, &profile).await?;
        let mut diff = Vec::new();
        self.guard_interpolated_fields(id, service, &profile, changes)?;
        for (key, value) in changes {
            let f = profile
                .fields
                .iter()
                .find(|f| &f.id == key)
                .context("Unknown app setting")?;
            validate_change(&profile, f, value, restore)?;
            let previous = get(&current, &f.path).cloned().unwrap_or(Value::Null);
            if previous != *value {
                diff.push(json!({"id":key,"label":f.label,"before":if f.kind=="secret"{json!("Hidden")}else{previous},"after":if f.kind=="secret"{json!("Hidden")}else{value.clone()}}));
            }
        }
        Ok(
            json!({"revision":self.integration_revision(id,&current,&profile)?,"changes":diff,"restart_required":matches!(profile.driver,Driver::Yaml{..}|Driver::ComposeEnvironment)}),
        )
    }
    pub async fn integration_apply(
        &self,
        id: &str,
        service: &str,
        changes: BTreeMap<String, Value>,
        revision: &str,
    ) -> Result<Value> {
        self.integration_apply_inner(id, service, changes, revision, false)
            .await
    }
    async fn integration_apply_inner(
        &self,
        id: &str,
        service: &str,
        changes: BTreeMap<String, Value>,
        revision: &str,
        restore: bool,
    ) -> Result<Value> {
        let _lock = self.project_lock(id)?;
        self.project_docker(&self.project(id)?, true)?;
        let profile = self.integration(id, service)?;
        let current = self.integration_document(id, service, &profile).await?;
        self.guard_interpolated_fields(id, service, &profile, &changes)?;
        ensure!(
            self.integration_revision(id, &current, &profile)? == revision,
            "App configuration changed; review a fresh preview"
        );
        let mut patch = json!({});
        let mut previous = json!({});
        for (key, value) in &changes {
            let field = profile
                .fields
                .iter()
                .find(|f| &f.id == key)
                .context("Unknown app setting")?;
            validate_change(&profile, field, value, restore)?;
            set(&mut patch, &field.path, value.clone())?;
            set(
                &mut previous,
                &field.path,
                get(&current, &field.path).cloned().unwrap_or(Value::Null),
            )?;
        }
        let dir = self.project_dir(id)?.join("app-config-backups");
        crate::core::private_dir(&dir)?;
        let backup = format!("config-{}", token(8)?);
        atomic_write(
            &dir.join(format!("{backup}.json")),
            &serde_json::to_vec_pretty(
                &json!({"service":service,"profile":profile,"before":previous,"changes":changes}),
            )?,
        )?;
        match &profile.driver {
            Driver::ComposeEnvironment => {
                let mut setup = self.setup(id)?;
                let mut values = compose_environment(&setup.compose["services"][service])?;
                for (key, value) in &changes {
                    let f = profile.fields.iter().find(|f| &f.id == key).unwrap();
                    if restore && value.is_null() {
                        values.remove(&f.path[0]);
                    } else {
                        values.insert(
                            f.path[0].clone(),
                            value
                                .as_str()
                                .context("Compose settings must be strings")?
                                .replace('$', "$$"),
                        );
                    }
                }
                setup.compose["services"][service]["environment"] = json!(values);
                self.save_setup_locked(id, setup)?;
            }
            Driver::Command { write, scope, .. } => {
                let payload = if scope.is_empty() {
                    patch
                } else {
                    patch
                        .get(scope)
                        .map(|v| json!({scope:v}))
                        .context("Invalid configuration scope")?
                };
                self.integration_command(
                    id,
                    service,
                    write,
                    &BTreeMap::new(),
                    Some(serde_json::to_vec(&payload)?),
                )
                .await
                .with_context(|| format!("Previous values saved in backup {backup}"))?;
            }
            Driver::Yaml { file, .. } => {
                let mut document = current;
                for (key, value) in &changes {
                    let f = profile.fields.iter().find(|f| &f.id == key).unwrap();
                    if restore && value.is_null() {
                        remove(&mut document, &f.path);
                    } else {
                        set(&mut document, &f.path, value.clone())?;
                    }
                }
                let mut setup = self.setup(id)?;
                setup
                    .files
                    .insert(file.clone(), serde_yaml::to_string(&document)?);
                // A bind-mounted file can retain its old inode after an atomic save.
                // Changing a Compose label ensures the next `up` recreates this service.
                let config = &mut setup.compose["services"][service];
                if config.get("labels").is_none() {
                    config["labels"] = json!({});
                }
                if let Some(entries) = config["labels"].as_array() {
                    let labels = entries
                        .iter()
                        .map(|v| {
                            let text = v.as_str().context("Compose labels must be strings")?;
                            let (key, value) = text.split_once('=').unwrap_or((text, ""));
                            Ok((key.to_owned(), json!(value)))
                        })
                        .collect::<Result<serde_json::Map<String, Value>>>()?;
                    config["labels"] = Value::Object(labels);
                }
                config["labels"]
                    .as_object_mut()
                    .context("Use a Compose label mapping or list")?
                    .insert(
                        "org.obiente.selfhost.config-revision".into(),
                        json!(token(8)?),
                    );
                self.save_setup_locked(id, setup)?;
            }
        }
        let applied = self
            .integration_document(id, service, &profile)
            .await
            .with_context(|| {
                format!(
                    "Write completed but verification failed; previous values saved in {backup}"
                )
            })?;
        for (key, value) in &changes {
            let field = profile.fields.iter().find(|f| &f.id == key).unwrap();
            ensure!(
                get(&applied, &field.path).unwrap_or(&Value::Null) == value,
                "{} did not retain its requested value; inspect app configuration overrides. Previous values saved in {backup}",
                field.label
            );
        }
        Ok(
            json!({"backup":backup,"verified":true,"restart_required":matches!(profile.driver,Driver::Yaml{..}|Driver::ComposeEnvironment)}),
        )
    }
    pub fn integration_backups(&self, id: &str, service: &str) -> Result<Vec<String>> {
        self.integration(id, service)?;
        let dir = self.project_dir(id)?.join("app-config-backups");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut backups = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|s| s == "json") {
                let saved: Value = serde_json::from_slice(&std::fs::read(entry.path())?)?;
                if saved["service"] == service {
                    backups.push(entry.path().file_stem().unwrap().to_string_lossy().into());
                }
            }
        }
        backups.sort();
        Ok(backups)
    }
    pub async fn integration_restore(
        &self,
        id: &str,
        service: &str,
        backup: &str,
        revision: Option<&str>,
    ) -> Result<Value> {
        ensure!(
            crate::setup::slug(backup) && backup.starts_with("config-"),
            "Invalid backup ID"
        );
        let profile = self.integration(id, service)?;
        let saved: Value = serde_json::from_slice(&std::fs::read(
            self.project_dir(id)?
                .join("app-config-backups")
                .join(format!("{backup}.json")),
        )?)?;
        ensure!(
            saved["service"] == service && saved["profile"] == serde_json::to_value(&profile)?,
            "Backup belongs to a different service or integration profile"
        );
        let mut values = BTreeMap::new();
        for key in saved["changes"]
            .as_object()
            .context("Invalid backup changes")?
            .keys()
        {
            let field = profile
                .fields
                .iter()
                .find(|f| &f.id == key)
                .context("Unknown backup field")?;
            values.insert(
                key.clone(),
                get(&saved["before"], &field.path)
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(revision) = revision {
            self.integration_apply_inner(id, service, values, revision, true)
                .await
        } else {
            self.integration_plan_inner(id, service, &values, true)
                .await
        }
    }
    pub async fn integration_action(
        &self,
        id: &str,
        service: &str,
        action_id: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Value> {
        self.integration_action_checked(id, service, action_id, inputs, None)
            .await
    }
    pub(crate) async fn integration_action_checked(
        &self,
        id: &str,
        service: &str,
        action_id: &str,
        mut inputs: BTreeMap<String, Value>,
        source_revision: Option<&str>,
    ) -> Result<Value> {
        let _lock = self.project_lock(id)?;
        if let Some(revision) = source_revision {
            ensure!(
                self.connection_source_revision(id)? == revision,
                "App files or server changed since connection preview; inspect saved client before recovery"
            );
        }

        self.project_docker(&self.project(id)?, true)?;
        let profile = self.integration(id, service)?;
        let action = profile
            .actions
            .iter()
            .find(|a| a.id == action_id)
            .context("Unknown app action")?;
        for key in inputs.keys() {
            ensure!(
                action.inputs.iter().any(|f| &f.id == key),
                "Unknown action input"
            );
        }
        for field in &action.inputs {
            if !inputs.contains_key(&field.id)
                && let Some(default) = &field.default
            {
                inputs.insert(field.id.clone(), default.clone());
            }
            validate_value(
                field,
                inputs
                    .get(&field.id)
                    .with_context(|| format!("{} is required", field.label))?,
            )?;
        }
        let activity_id = token(8)?;
        let project = self.project(id)?;
        self.mutate(|data| {
            data.activities.push(crate::core::Activity {
                id: activity_id.clone(),
                project_id: id.into(),
                project_name: project.name.clone(),
                action: format!("{}: {}", profile.name, action.label),
                status: "running".into(),
                message: String::new(),
                started_at: crate::core::now(),
                finished_at: None,
                read: false,
            });
            if data.activities.len() > 200 {
                data.activities.remove(0);
            }
            Ok(())
        })?;
        let mut results = Vec::new();
        for step in &action.steps {
            if let Some(check) = &step.skip_if
                && self
                    .integration_command(id, service, check, &inputs, None)
                    .await
                    .is_ok()
            {
                results.push(json!({"label":step.label,"status":"already_configured"}));
                continue;
            }
            let result = self
                .integration_command(id, service, &step.command, &inputs, None)
                .await;
            match result {
                Ok(mut output) => {
                    for f in action.inputs.iter().filter(|f| f.kind == "secret") {
                        if let Some(secret) = inputs
                            .get(&f.id)
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                        {
                            output = output.replace(secret, "[hidden]");
                        }
                    }
                    results.push(json!({"label":step.label,"status":"succeeded","output":output}));
                }
                Err(_) => {
                    self.finish_integration_activity(&activity_id, false)?;
                    return Ok(
                        json!({"ok":false,"steps":results,"failed_step":step.label,"message":"Step failed. Earlier successful steps remain applied; inspect app state before retrying."}),
                    );
                }
            }
        }
        self.finish_integration_activity(&activity_id, true)?;
        Ok(json!({"ok":true,"steps":results}))
    }
    fn finish_integration_activity(&self, id: &str, success: bool) -> Result<()> {
        self.mutate(|data| {
            if let Some(a) = data.activities.iter_mut().find(|a| a.id == id) {
                a.status = if success { "succeeded" } else { "failed" }.into();
                a.message = if success {
                    "App workflow completed."
                } else {
                    "App workflow stopped. Earlier steps may have completed."
                }
                .into();
                a.finished_at = Some(crate::core::now());
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declarative_password_inputs_reject_short_values_before_commands() {
        let profile: Profile =
            serde_json::from_str(include_str!("../catalog/integrations/freshrss.json")).unwrap();
        let field = profile.actions[0]
            .inputs
            .iter()
            .find(|f| f.id == "password")
            .unwrap();
        assert!(validate_value(field, &json!("")).is_err());
        assert!(validate_value(field, &json!("short")).is_err());
        assert!(validate_value(field, &json!("a sufficiently long password")).is_ok());
        assert!(validate_value(field, &json!("x".repeat(256))).is_err());
    }
    fn yaml_setup() -> crate::setup::Setup {
        crate::setup::Setup {database:None,compose:json!({"services":{"identity":{"image":"ghcr.io/zitadel/zitadel:v4","x-selfhost-integration":"zitadel","command":["start","--config","/config/zitadel.yaml"],"volumes":["./files/zitadel.yaml:/config/zitadel.yaml:ro"]}}}),environment:BTreeMap::new(),files:BTreeMap::from([("zitadel.yaml".into(),"ExternalDomain: identity.example.com\nExternalPort: 443\nExternalSecure: true\nLog:\n  Level: info\n  AddSource: true\nCustomExtension:\n  Retain: yes\n".into())])}
    }
    #[tokio::test]
    async fn compose_environment_preserves_unrelated_values_and_guards_interpolation() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Metrics".into(),
                apps: vec!["grafana".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let mut setup = store.setup(&project.id).unwrap();
        setup.compose["services"]["grafana"]["environment"]["GF_SERVER_ROOT_URL"] =
            json!("${PUBLIC_URL}");
        store.save_setup(&project.id, setup).unwrap();
        let state = store
            .integration_state(&project.id, "grafana")
            .await
            .unwrap();
        assert_eq!(state["values"]["public-url"], "${PUBLIC_URL}");
        store
            .integration_plan(&project.id, "grafana", &BTreeMap::new())
            .await
            .unwrap();
        let error = store
            .integration_plan(
                &project.id,
                "grafana",
                &BTreeMap::from([("public-url".into(), json!("https://new.example.com"))]),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("interpolation"));
        let mut setup = store.setup(&project.id).unwrap();
        setup.compose["services"]["grafana"]["environment"]["GF_SERVER_ROOT_URL"] =
            json!("https://metrics.example.com/$$literal");
        setup.compose["services"]["grafana"]["environment"]["UNRELATED"] = json!("${LEAVE_ME}");
        store.save_setup(&project.id, setup).unwrap();
        let changes = BTreeMap::from([("public-url".into(), json!("https://new.example.com"))]);
        let plan = store
            .integration_plan(&project.id, "grafana", &changes)
            .await
            .unwrap();
        let result = store
            .integration_apply(
                &project.id,
                "grafana",
                changes,
                plan["revision"].as_str().unwrap(),
            )
            .await
            .unwrap();
        let backup = result["backup"].as_str().unwrap();
        let plan = store
            .integration_restore(&project.id, "grafana", backup, None)
            .await
            .unwrap();
        store
            .integration_restore(&project.id, "grafana", backup, plan["revision"].as_str())
            .await
            .unwrap();
        let setup = store.setup(&project.id).unwrap();
        let env = &setup.compose["services"]["grafana"]["environment"];
        assert_eq!(
            env["GF_SERVER_ROOT_URL"],
            "https://metrics.example.com/$$literal"
        );
        assert_eq!(env["UNRELATED"], "${LEAVE_ME}");
    }
    #[test]
    fn typed_fields_and_argv_keep_user_values_literal() {
        let profile: Profile =
            serde_json::from_str(include_str!("../catalog/integrations/nextcloud.json")).unwrap();
        profile.validate().unwrap();
        let field = profile
            .fields
            .iter()
            .find(|f| f.id == "maintenance-window")
            .unwrap();
        assert!(validate_value(field, &json!(24)).is_err());
        assert!(validate_value(field, &json!("3")).is_err());
        assert!(validate_value(field, &json!(3)).is_ok());
        let inputs = BTreeMap::from([
            (
                "arguments".into(),
                json!([
                    "config:system:set",
                    "literal",
                    "--value={{secret}}; $(touch /tmp/not-run)"
                ]),
            ),
            ("secret".into(), json!("should-not-expand")),
        ]);
        let command = Command {
            argv: vec!["occ".into(), "{{arguments}}".into()],
            ..Default::default()
        };
        assert_eq!(
            arguments(&command, &inputs).unwrap()[3],
            "--value={{secret}}; $(touch /tmp/not-run)"
        );
        assert_eq!(
            expand("--value={{arguments}}", &inputs).unwrap(),
            format!("--value={}", inputs["arguments"])
        );
    }
    #[tokio::test]
    async fn yaml_merge_restore_drift_and_overrides() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create_setup("Identity test", "local", yaml_setup())
            .unwrap();
        let changes: BTreeMap<String, Value> =
            BTreeMap::from([("log-level".into(), json!("debug"))]);
        let profile = store.integration(&project.id, "identity").unwrap();
        let field = profile
            .fields
            .iter()
            .find(|f| f.path == vec!["Log", "Level"])
            .unwrap();
        let changes = BTreeMap::from([(field.id.clone(), changes["log-level"].clone())]);
        let plan = Box::pin(store.integration_plan(&project.id, "identity", &changes))
            .await
            .unwrap();
        let applied = Box::pin(store.integration_apply(
            &project.id,
            "identity",
            changes.clone(),
            plan["revision"].as_str().unwrap(),
        ))
        .await
        .unwrap();
        let text = &store.setup(&project.id).unwrap().files["zitadel.yaml"];
        let document: Value = serde_yaml::from_str(text).unwrap();
        assert_eq!(document["Log"]["Level"], "debug");
        assert_eq!(document["Log"]["AddSource"], true);
        assert_eq!(document["CustomExtension"]["Retain"], "yes");
        assert!(store.setup(&project.id).unwrap().compose["services"]["identity"]["labels"]["org.obiente.selfhost.config-revision"].is_string());
        assert!(
            Box::pin(store.integration_apply(
                &project.id,
                "identity",
                changes.clone(),
                plan["revision"].as_str().unwrap()
            ))
            .await
            .is_err()
        );
        let backup = applied["backup"].as_str().unwrap();
        let restore = Box::pin(store.integration_restore(&project.id, "identity", backup, None))
            .await
            .unwrap();
        Box::pin(store.integration_restore(
            &project.id,
            "identity",
            backup,
            restore["revision"].as_str(),
        ))
        .await
        .unwrap();
        let mut setup = store.setup(&project.id).unwrap();
        setup.compose["services"]["identity"]["environment"] = json!({"ZITADEL_LOG_LEVEL":"warn"});
        store.save_setup(&project.id, setup).unwrap();
        assert!(
            Box::pin(store.integration_plan(&project.id, "identity", &changes))
                .await
                .unwrap_err()
                .to_string()
                .contains("override")
        );
    }
    #[tokio::test]
    async fn restore_removes_previously_absent_yaml_values() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create_setup("Identity test", "local", yaml_setup())
            .unwrap();
        let profile = store.integration(&project.id, "identity").unwrap();
        let field = profile
            .fields
            .iter()
            .find(|f| f.path == vec!["TLS", "Enabled"])
            .unwrap();
        let changes = BTreeMap::from([(field.id.clone(), json!(true))]);
        let plan = Box::pin(store.integration_plan(&project.id, "identity", &changes))
            .await
            .unwrap();
        let applied = Box::pin(store.integration_apply(
            &project.id,
            "identity",
            changes,
            plan["revision"].as_str().unwrap(),
        ))
        .await
        .unwrap();
        let backup = applied["backup"].as_str().unwrap();
        let restore = Box::pin(store.integration_restore(&project.id, "identity", backup, None))
            .await
            .unwrap();
        Box::pin(store.integration_restore(
            &project.id,
            "identity",
            backup,
            restore["revision"].as_str(),
        ))
        .await
        .unwrap();
        let doc: Value =
            serde_yaml::from_str(&store.setup(&project.id).unwrap().files["zitadel.yaml"]).unwrap();
        assert!(doc["TLS"].get("Enabled").is_none());
    }
    #[tokio::test]
    async fn yaml_root_arrays_roundtrip_restore_and_accept_long_bind_syntax() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path().into()).unwrap();
        let recipe = store
            .catalog
            .iter_mut()
            .find(|app| app.id == "homarr")
            .unwrap();
        let profile: Profile = serde_json::from_value(json!({"schema":1,"name":"Array fixture","documentation":"https://example.test", "driver":{"kind":"yaml","file":"services.yaml","mount":"/config/services.yaml"},"fields":[{"id":"services","label":"Services","kind":"json","path":[]}],"actions":[]})).unwrap();
        profile.validate().unwrap();
        assert!(validate_value(&profile.fields[0], &json!("scalar")).is_err());
        assert!(validate_value(&profile.fields[0], &json!({"huge":"x".repeat(131073)})).is_err());
        recipe.integration = Some(profile);
        let image = recipe.image.clone();
        let original = json!([{"Existing":{"custom":true}}]);
        let setup = crate::setup::Setup {
            database: None,
            environment: BTreeMap::new(),
            compose: json!({"services":{"web":{"image":image,"x-selfhost-app":"homarr","volumes":[{"type":"bind","source":"./files/services.yaml","target":"/config/services.yaml","read_only":true}]}}}),
            files: BTreeMap::from([(
                "services.yaml".into(),
                serde_yaml::to_string(&original).unwrap(),
            )]),
        };
        let project = store.create_setup("Array fixture", "local", setup).unwrap();
        let changes = BTreeMap::from([(
            "services".into(),
            json!([{"New":{"url":"https://app.example.test"}}]),
        )]);
        let plan = Box::pin(store.integration_plan(&project.id, "web", &changes))
            .await
            .unwrap();
        let applied = Box::pin(store.integration_apply(
            &project.id,
            "web",
            changes,
            plan["revision"].as_str().unwrap(),
        ))
        .await
        .unwrap();
        let backup = applied["backup"].as_str().unwrap();
        let restore = Box::pin(store.integration_restore(&project.id, "web", backup, None))
            .await
            .unwrap();
        Box::pin(store.integration_restore(
            &project.id,
            "web",
            backup,
            restore["revision"].as_str(),
        ))
        .await
        .unwrap();
        let restored: Value =
            serde_yaml::from_str(&store.setup(&project.id).unwrap().files["services.yaml"])
                .unwrap();
        assert_eq!(restored, original);
    }
    #[test]
    fn only_declared_schedules_and_database_overrides_are_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Example".into(),
                apps: vec!["nextcloud".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let schedule = |action: &str| crate::core::Schedule {
            project_id: project.id.clone(),
            action: action.into(),
            interval_hours: 24,
            enabled: true,
            next_run: 0,
        };
        assert!(
            store
                .schedule(schedule("app:nextcloud:update-apps"))
                .is_ok()
        );
        assert!(store.schedule(schedule("app:nextcloud:occ")).is_err());
        store
            .configure_database(
                &project.id,
                crate::database::Selection {
                    engine: None,
                    mode: "dedicated".into(),
                    source_id: None,
                },
            )
            .unwrap();
        let setup = store.setup(&project.id).unwrap();
        let env = &setup.compose["services"]["nextcloud"]["environment"];
        assert!(env.get("SQLITE_DATABASE").is_none());
        assert!(env.get("POSTGRES_HOST").is_some());
    }
    #[tokio::test]
    async fn native_actions_reject_read_only_servers_before_container_access() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let server = store
            .add_server(crate::infrastructure::Server {
                id: String::new(),
                name: "Read-only test".into(),
                provider: crate::infrastructure::Provider::DockerContext,
                endpoint: "unavailable-test-engine".into(),
                group: String::new(),
                read_only: true,
                app_host: String::new(),
            })
            .unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Read-only app".into(),
                apps: vec!["nextcloud".into()],
                server_id: server.id,
            })
            .unwrap();
        let result =
            Box::pin(store.integration_action(&project.id, "nextcloud", "status", BTreeMap::new()))
                .await
                .unwrap_err();
        assert!(result.to_string().contains("read-only"));
    }
}
