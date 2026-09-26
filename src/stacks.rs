//! Declarative multi-service installs, materialized as ordinary portable setups.
use crate::{
    core::{Project, Store, token},
    setup::Setup,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    #[serde(default)]
    pub icon: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub inputs: BTreeMap<String, Input>,
    pub secrets: BTreeMap<String, Secret>,
    pub compose: Value,
    /// Portable files mounted explicitly from ./files in the Compose document.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub environment_generators: BTreeMap<String, Expiry>,
    pub instructions: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub label: String,
    pub kind: String,
    pub default: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Secret {
    pub bytes: usize,
    #[serde(default)]
    pub prefix: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expiry {
    pub utc_expiry_days: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    pub blueprint: String,
    pub name: String,
    pub server_id: String,
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub acknowledgements: Vec<String>,
}
impl Blueprint {
    fn materialize(&self, supplied: &BTreeMap<String, Value>) -> Result<Setup> {
        ensure!(
            supplied.keys().all(|k| self.inputs.contains_key(k)),
            "Unknown stack input"
        );
        let mut vars = BTreeMap::new();
        for (key, input) in &self.inputs {
            let value = supplied.get(key).unwrap_or(&input.default);
            let valid = match input.kind.as_str() {
                "port" => value.as_u64().is_some_and(|p| (1024..=65535).contains(&p)),
                "hostname" => value
                    .as_str()
                    .is_some_and(|s| s == "localhost" || crate::networking::hostname(s)),
                "email" => value.as_str().is_some_and(|s| {
                    s.contains('@')
                        && s.len() < 254
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b))
                }),
                "identifier" => value.as_str().is_some_and(crate::setup::slug),
                _ => false,
            };
            ensure!(valid, "Invalid input: {}", input.label);
            vars.insert(key.clone(), value.clone());
        }
        let mut environment = self.environment.clone();
        for (name, generator) in &self.environment_generators {
            ensure!(
                (1..=365).contains(&generator.utc_expiry_days),
                "Generated token expiry must be within one year"
            );
            environment.insert(
                name.clone(),
                (chrono::Utc::now() + chrono::Duration::days(generator.utc_expiry_days))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            );
        }
        for (name, secret) in &self.secrets {
            ensure!(
                (16..=64).contains(&secret.bytes) && secret.prefix.len() <= 16,
                "Invalid secret generator"
            );
            environment.insert(
                name.clone(),
                format!("{}{}", secret.prefix, token(secret.bytes)?),
            );
        }
        let setup = Setup {
            database: None,
            compose: crate::networking::render(&self.compose, &vars)?,
            environment,
            files: serde_json::from_value(crate::networking::render(&json!(self.files), &vars)?)?,
        };
        setup.validate()?;
        Ok(setup)
    }
}
impl Store {
    pub fn blueprints(&self) -> Result<BTreeMap<String, Blueprint>> {
        let mut result = BTreeMap::new();
        for name in crate::catalog::BuiltinCatalog::iter()
            .filter(|n| n.starts_with("stacks/") && n.ends_with(".json"))
        {
            let blueprint: Blueprint =
                serde_json::from_slice(&crate::catalog::BuiltinCatalog::get(&name).unwrap().data)?;
            ensure!(crate::setup::slug(&blueprint.id), "Invalid blueprint ID");
            result.insert(blueprint.id.clone(), blueprint);
        }
        let dir = self.root.join("stack-profiles");
        if dir.exists() {
            for file in std::fs::read_dir(dir)? {
                let file = file?;
                if file.path().extension().is_some_and(|e| e == "json") {
                    let blueprint: Blueprint =
                        serde_json::from_slice(&std::fs::read(file.path())?)?;
                    ensure!(crate::setup::slug(&blueprint.id), "Invalid blueprint ID");
                    result.insert(blueprint.id.clone(), blueprint);
                }
            }
        }
        Ok(result)
    }
    pub fn install_blueprint(&self, input: Install) -> Result<(Project, Vec<String>)> {
        let blueprint = self
            .blueprints()?
            .remove(&input.blueprint)
            .context("Stack not found")?;
        ensure!(
            blueprint
                .requirements
                .iter()
                .all(|r| input.acknowledgements.contains(&r.id)),
            "Review and acknowledge every deployment requirement"
        );
        let setup = blueprint.materialize(&input.inputs)?;
        let project = self.create_setup(&input.name, &input.server_id, setup)?;
        Ok((project, blueprint.instructions))
    }
    pub fn stack_catalog(&self) -> Result<Value> {
        Ok(json!(self.blueprints()?.values().collect::<Vec<_>>()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_default_blueprint_materializes_and_keeps_declared_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        for blueprint in store.blueprints().unwrap().values() {
            let setup = blueprint
                .materialize(&BTreeMap::new())
                .unwrap_or_else(|error| panic!("{}: {error}", blueprint.id));
            assert_eq!(setup.files.len(), blueprint.files.len());
            for name in blueprint.files.keys() {
                assert!(setup.files.contains_key(name));
            }
            let project = store.create_setup(&blueprint.name, "local", setup).unwrap();
            assert!(!project.services.is_empty());
        }
    }
    #[test]
    fn blueprints_are_portable_with_unique_secrets_and_reject_interpolation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let blueprints = store.blueprints().unwrap();
        assert!(blueprints.len() >= 3);
        for blueprint in blueprints.values().filter(|b| b.category == "Identity") {
            let defaults = blueprint
                .inputs
                .iter()
                .map(|(k, v)| (k.clone(), v.default.clone()))
                .collect();
            let one = blueprint.materialize(&defaults).unwrap();
            let two = blueprint.materialize(&defaults).unwrap();
            assert_ne!(one.environment, two.environment);
            assert!(one.files.is_empty());
            assert!(!one.compose.to_string().contains("docker.sock"));
            let mut bad = defaults.clone();
            bad.insert("domain".into(), json!("${HOST}"));
            assert!(blueprint.materialize(&bad).is_err());
        }
    }
}
