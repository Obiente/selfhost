use anyhow::{Context, Result, ensure};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(RustEmbed)]
#[folder = "catalog/"]
pub(crate) struct BuiltinCatalog;

pub fn database_driver(engine: &str) -> Result<serde_json::Value> {
    ensure!(crate::setup::slug(engine), "Invalid database engine");
    let file = BuiltinCatalog::get(&format!("databases/{engine}.json"))
        .context("Missing database driver configuration")?;
    let driver: serde_json::Value = serde_json::from_slice(&file.data)?;
    ensure!(
        driver["engine"] == engine
            && driver["port"]
                .as_u64()
                .is_some_and(|p| p > 0 && p <= u16::MAX as u64),
        "Invalid database driver identity or port"
    );
    Ok(driver)
}

pub fn database_drivers() -> Result<Vec<serde_json::Value>> {
    let mut drivers = Vec::new();
    for file in
        BuiltinCatalog::iter().filter(|p| p.starts_with("databases/") && p.ends_with(".json"))
    {
        let driver: serde_json::Value = serde_json::from_slice(
            &BuiltinCatalog::get(&file)
                .context("Missing database driver")?
                .data,
        )?;
        drivers.push(serde_json::json!({"engine":driver["engine"],"name":driver["name"],"port":driver["port"],"auth_database":driver["auth_database"]}));
    }
    drivers.sort_by_key(|v| v["engine"].as_str().unwrap_or_default().to_owned());
    Ok(drivers)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Secret {
    pub environment: String,
    pub bytes: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dashboard {
    pub list_path: String,
    pub create_path: String,
    pub auth_header: String,
    pub match_field: String,
    pub body: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAction {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Executable and arguments run inside this service's container.
    pub command: Vec<String>,
    #[serde(default = "action_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub confirm: bool,
}
fn action_timeout() -> u64 {
    30
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseRecipe {
    pub engine: String,
    #[serde(default)]
    pub engines: Vec<String>,
    #[serde(default)]
    pub engine_environment: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    pub engine_ssl_modes: BTreeMap<String, Vec<String>>,
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub unset_environment: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppInfo {
    /// This app requires its declared deployment method (dependencies or file mounts).
    #[serde(default)]
    pub deployment_only: bool,
    #[serde(default)]
    pub versions: Option<crate::versions::Profile>,
    #[serde(default)]
    pub onboarding: Option<crate::onboarding::Profile>,
    #[serde(default)]
    pub integration: Option<crate::integrations::Profile>,
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub image: String,
    pub port: u16,
    pub container_port: u16,
    pub data_path: String,
    pub docs: String,
    pub icon: String,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub dashboard: Option<Dashboard>,
    #[serde(default)]
    pub actions: Vec<ServiceAction>,
    #[serde(default)]
    pub database: Option<DatabaseRecipe>,
    #[serde(default)]
    pub command: Vec<String>,
}
impl AppInfo {
    pub fn validate(&self) -> Result<()> {
        if let Some(profile) = &self.versions {
            profile.validate(&self.image)?;
        }
        if let Some(profile) = &self.onboarding {
            profile.validate()?;
        }
        if let Some(profile) = &self.integration {
            profile.validate()?;
        }
        ensure!(
            self.command.len() <= 64
                && self
                    .command
                    .iter()
                    .all(|a| !a.contains('\0') && a.len() <= 4096),
            "Invalid container command"
        );
        if let Some(database) = &self.database {
            database_driver(&database.engine)?;
            for (engine, modes) in &database.engine_ssl_modes {
                ensure!(
                    (engine == &database.engine || database.engines.contains(engine))
                        && !modes.is_empty()
                        && modes
                            .iter()
                            .all(|mode| ["disable", "require", "verify-full"]
                                .contains(&mode.as_str())),
                    "Invalid database adapter TLS modes"
                );
            }
            for engine in &database.engines {
                database_driver(engine)?;
                ensure!(
                    engine == &database.engine
                        || database
                            .engine_environment
                            .get(engine)
                            .is_some_and(|m| !m.is_empty()),
                    "Every alternate engine needs an environment mapping"
                );
            }
            ensure!(
                !database.environment.is_empty(),
                "Database recipe needs environment mappings"
            );
        }
        ensure!(self.schema == 1, "Unsupported manifest schema");
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 48
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "Use a lowercase app id with letters, numbers, and hyphens"
        );
        ensure!(
            !self.name.trim().is_empty() && !self.description.trim().is_empty(),
            "Name and description are required"
        );
        ensure!(
            self.port >= 1024 && self.container_port > 0,
            "Use a host port above 1023 and a nonzero container port"
        );
        ensure!(
            self.image.contains(':')
                && self
                    .image
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/._-:@".contains(&b)),
            "Use a Docker image with an explicit version tag or digest"
        );
        ensure!(
            self.data_path.starts_with('/')
                && self.data_path != "/"
                && !self.data_path.contains("..")
                && !self.data_path.contains(':')
                && !self.data_path.contains('$'),
            "Use an absolute container path for application data"
        );
        ensure!(
            self.docs.starts_with("https://") && self.icon.starts_with("https://"),
            "Documentation and icon URLs must use HTTPS"
        );
        let mut names = std::collections::HashSet::new();
        let mut action_ids = std::collections::HashSet::new();
        for action in &self.actions {
            ensure!(
                !action.id.is_empty()
                    && action.id.len() <= 48
                    && action
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "Invalid service action id"
            );
            ensure!(
                !["start", "stop", "restart", "update"].contains(&action.id.as_str())
                    && action_ids.insert(&action.id),
                "Duplicate or reserved service action id"
            );
            ensure!(
                !action.label.trim().is_empty() && !action.description.trim().is_empty(),
                "Actions need a label and description"
            );
            ensure!(
                !action.command.is_empty()
                    && action.command.len() <= 64
                    && !action.command[0].is_empty()
                    && action
                        .command
                        .iter()
                        .all(|arg| arg.len() <= 4096 && !arg.contains('\0')),
                "Actions need a valid executable and argument list"
            );
            ensure!(
                (1..=300).contains(&action.timeout_seconds),
                "Action timeout must be between 1 and 300 seconds"
            );
        }
        for secret in &self.secrets {
            ensure!(
                !secret.environment.is_empty()
                    && secret
                        .environment
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
                "Secret environment names must use uppercase letters, digits, and underscores"
            );
            ensure!(
                (16..=128).contains(&secret.bytes),
                "Secrets must contain between 16 and 128 random bytes"
            );
            ensure!(
                names.insert(&secret.environment)
                    && !self.environment.contains_key(&secret.environment),
                "Duplicate environment name"
            );
        }
        if let Some(recipe) = &self.dashboard {
            for path in [&recipe.list_path, &recipe.create_path] {
                ensure!(
                    path.starts_with('/')
                        && !path.starts_with("//")
                        && !path.contains(['\r', '\n', '#']),
                    "Dashboard API paths must be relative to the service"
                );
            }
            ensure!(
                !recipe.auth_header.is_empty()
                    && recipe
                        .auth_header
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                "Invalid API key header"
            );
            ensure!(
                !recipe.match_field.is_empty() && recipe.body.values().any(|v| v == "{url}"),
                "Dashboard recipe must include an app URL and matching field"
            );
        }
        Ok(())
    }
    pub fn parse(content: &str) -> Result<Self> {
        let app: Self = toml::from_str(content)?;
        app.validate()?;
        Ok(app)
    }
}
pub fn load(directory: Option<&Path>) -> Result<Vec<AppInfo>> {
    let mut apps = BTreeMap::new();
    for name in BuiltinCatalog::iter() {
        if !name.ends_with(".toml") {
            continue;
        }
        let file = BuiltinCatalog::get(&name).context("Missing bundled manifest")?;
        let mut app = AppInfo::parse(std::str::from_utf8(&file.data)?)
            .with_context(|| format!("Invalid manifest {name}"))?;
        if let Some(file) = BuiltinCatalog::get(&format!("integrations/{}.json", app.id)) {
            app.integration = Some(serde_json::from_slice(&file.data)?);
            app.validate()?;
        }
        if let Some(file) = BuiltinCatalog::get(&format!("onboarding/{}.json", app.id)) {
            app.onboarding = Some(serde_json::from_slice(&file.data)?);
            app.validate()?;
        }
        if let Some(file) = BuiltinCatalog::get(&format!("versions/{}.json", app.id)) {
            app.versions = Some(serde_json::from_slice(&file.data)?);
            app.validate()?;
        }
        apps.insert(app.id.clone(), app);
    }
    if let Some(directory) = directory {
        let mut seen = std::collections::HashSet::new();
        for file in fs::read_dir(directory)? {
            let file = file?;
            if file.path().extension().is_some_and(|e| e == "toml") {
                let mut app =
                    AppInfo::parse(&fs::read_to_string(file.path())?).with_context(|| {
                        format!("Invalid manifest {}", file.file_name().to_string_lossy())
                    })?;
                ensure!(
                    seen.insert(app.id.clone()),
                    "Duplicate catalog id {}",
                    app.id
                );
                let profile_path = directory
                    .join("integrations")
                    .join(format!("{}.json", app.id));
                if profile_path.is_file() {
                    app.integration = Some(serde_json::from_slice(&fs::read(profile_path)?)?);
                    app.validate()?;
                }
                let onboarding_path = directory
                    .join("onboarding")
                    .join(format!("{}.json", app.id));
                if onboarding_path.is_file() {
                    app.onboarding = Some(serde_json::from_slice(&fs::read(onboarding_path)?)?);
                    app.validate()?;
                }
                let versions_path = directory.join("versions").join(format!("{}.json", app.id));
                if versions_path.is_file() {
                    app.versions = Some(serde_json::from_slice(&fs::read(versions_path)?)?);
                    app.validate()?;
                }
                apps.insert(app.id.clone(), app);
            }
        }
    }
    Ok(apps.into_values().collect())
}
pub fn secret_key(app: &AppInfo, name: &str) -> String {
    format!(
        "SELFHOST_{}_{}",
        app.id.replace('-', "_").to_uppercase(),
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_manifests_validate() {
        assert!(load(None).unwrap().len() >= 2);
    }
    #[test]
    fn unknown_fields_and_unsafe_paths_fail() {
        let source = include_str!("../catalog/homarr.toml");
        assert!(AppInfo::parse(&source.replace("/appdata", "/../../data")).is_err());
        assert!(AppInfo::parse(&format!("typo = 1\n{source}")).is_err());
    }
    #[test]
    fn service_actions_validate_and_keep_arguments_literal() {
        let source = include_str!("../catalog/uptime-kuma.toml");
        let app = AppInfo::parse(source).unwrap();
        assert_eq!(app.actions[0].command, ["du", "-sh", "/app/data"]);
        assert!(AppInfo::parse(&source.replace("id = \"data-usage\"", "id = \"update\"")).is_err());
        assert!(
            AppInfo::parse(&source.replace("timeout_seconds = 15", "timeout_seconds = 999"))
                .is_err()
        );
        assert!(AppInfo::parse(&source.replace("[\"du\", \"-sh\", \"/app/data\"]", "[]")).is_err());
        let mut with_literal = app;
        with_literal.actions[0]
            .command
            .push("$(touch /tmp/example); echo literal".into());
        with_literal.validate().unwrap();
        let restored: AppInfo =
            serde_json::from_str(&serde_json::to_string(&with_literal).unwrap()).unwrap();
        assert_eq!(restored.actions[0].command, with_literal.actions[0].command);
    }
    #[test]
    fn new_service_can_be_loaded_without_code_changes() {
        let directory = tempfile::tempdir().unwrap();
        let source =
            include_str!("../catalog/uptime-kuma.toml").replace("uptime-kuma", "example-service");
        fs::write(directory.path().join("example.toml"), source).unwrap();
        assert!(
            load(Some(directory.path()))
                .unwrap()
                .iter()
                .any(|a| a.id == "example-service")
        );
    }
}
