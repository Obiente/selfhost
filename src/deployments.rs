//! Deployment choices are contributor data, resolved to existing recipes or stack blueprints.
use crate::{
    core::{CreateProject, Project, Store, run},
    stacks::Install,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Safety comes from the actual Compose resources, not optional catalog metadata.
fn owned_resource_names(
    compose: &Value,
    project_name: &str,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut containers = BTreeSet::new();
    let mut volumes = BTreeSet::new();
    for (key, names) in [
        ("reserved_containers", &mut containers),
        ("reserved_volumes", &mut volumes),
    ] {
        if let Some(values) = compose["x-selfhost"].get(key) {
            for name in values
                .as_array()
                .context("Reserved resources must be a list")?
            {
                names.insert(
                    name.as_str()
                        .context("Invalid reserved resource name")?
                        .to_owned(),
                );
            }
        }
    }
    if let Some(services) = compose["services"].as_object() {
        for service in services.values() {
            if let Some(name) = service.get("container_name") {
                containers.insert(
                    name.as_str()
                        .context("Container name must be a string")?
                        .to_owned(),
                );
            }
        }
    }
    if let Some(definitions) = compose.get("volumes") {
        for (key, definition) in definitions
            .as_object()
            .context("Volume definitions must be a mapping")?
        {
            // Explicitly external volumes are references, never claimed as owned.
            // Catalog-reserved names above remain protected even when marked external.
            if definition.get("external") == Some(&Value::Bool(true)) {
                continue;
            }
            ensure!(
                definition
                    .get("external")
                    .is_none_or(|v| v == &Value::Bool(false)),
                "Use a boolean external volume declaration"
            );
            let name = match definition.get("name") {
                Some(name) => name
                    .as_str()
                    .context("Volume name must be a string")?
                    .to_owned(),
                None => format!("{project_name}_{key}"),
            };
            volumes.insert(name);
        }
    }
    for name in containers.iter().chain(&volumes) {
        ensure!(
            !name.is_empty()
                && name.len() <= 255
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "Use literal container and volume names so Selfhost can verify ownership before starting"
        );
    }
    Ok((containers, volumes))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployment {
    pub app: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub recipe: Option<String>,
    pub blueprint: Option<String>,
    #[serde(default)]
    pub default: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDeployment {
    #[serde(default)]
    pub version: Option<crate::versions::Selection>,
    pub app: String,
    pub method: String,
    pub name: String,
    pub server_id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub acknowledgements: Vec<String>,
}
impl Store {
    fn deployment_profiles(&self) -> Result<BTreeMap<(String, String), Deployment>> {
        let mut result = BTreeMap::new();
        for name in crate::catalog::BuiltinCatalog::iter()
            .filter(|n| n.starts_with("deployments/") && n.ends_with(".json"))
        {
            for profile in serde_json::from_slice::<Vec<Deployment>>(
                &crate::catalog::BuiltinCatalog::get(&name).unwrap().data,
            )? {
                result.insert((profile.app.clone(), profile.id.clone()), profile);
            }
        }
        let directory = self.root.join("deployment-profiles");
        if directory.exists() {
            for entry in std::fs::read_dir(directory)? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    for profile in serde_json::from_slice::<Vec<Deployment>>(&std::fs::read(path)?)?
                    {
                        result.insert((profile.app.clone(), profile.id.clone()), profile);
                    }
                }
            }
        }
        let mut defaults = BTreeSet::new();
        for profile in result.values() {
            ensure!(
                !profile.default || defaults.insert(profile.app.clone()),
                "Only one deployment method per app can be the default"
            );
            ensure!(
                crate::setup::slug(&profile.app) && crate::setup::slug(&profile.id),
                "Invalid deployment profile ID"
            );
            ensure!(
                profile.recipe.is_some() != profile.blueprint.is_some(),
                "Choose one deployment recipe or blueprint"
            );
        }
        Ok(result)
    }
    pub fn deployments(&self) -> Result<Value> {
        let blueprints = self.blueprints()?;
        let mut values = vec![];
        for profile in self.deployment_profiles()?.values() {
            let mut value = serde_json::to_value(profile)?;
            if let Some(id) = &profile.blueprint {
                value["configuration"] = json!(
                    blueprints
                        .get(id)
                        .context("Deployment blueprint not found")?
                );
            }
            values.push(value);
        }
        Ok(json!(values))
    }
    pub fn create_deployment(&self, input: CreateDeployment) -> Result<(Project, Vec<String>)> {
        let profile = self
            .deployment_profiles()?
            .remove(&(input.app, input.method))
            .context("Deployment method not found")?;
        if let Some(recipe) = profile.recipe {
            ensure!(
                input.inputs.is_empty(),
                "This method is configured after project creation"
            );
            let selections = input
                .version
                .map(|s| BTreeMap::from([(recipe.clone(), s)]))
                .unwrap_or_default();
            let project = self.create_versioned(
                CreateProject {
                    server_id: input.server_id,
                    name: input.name,
                    apps: vec![recipe],
                },
                selections,
            )?;
            return Ok((
                project,
                vec!["Review the project settings before starting it.".into()],
            ));
        }
        ensure!(
            input.version.is_none(),
            "Stack deployment images must be selected through their declared inputs; a single image override is not supported"
        );
        self.install_blueprint(Install {
            blueprint: profile.blueprint.context("Missing deployment blueprint")?,
            name: input.name,
            server_id: input.server_id,
            inputs: input.inputs,
            acknowledgements: input.acknowledgements,
        })
    }
    /// Before every start/refresh, do not adopt fixed container or volume names belonging to another deployment.
    pub async fn assert_deployment_start_safe(&self, id: &str) -> Result<()> {
        let project = self.project(id)?;
        let setup = self.setup(id)?;
        let expected = format!("selfhost-{id}");
        let (containers, volumes) = owned_resource_names(&setup.compose, &expected)?;
        for (names, kind, format) in [
            (
                containers,
                "container",
                r#"{{index .Config.Labels "com.docker.compose.project"}}"#,
            ),
            (
                volumes,
                "volume",
                r#"{{index .Labels "com.docker.compose.project"}}"#,
            ),
        ] {
            if !names.is_empty() {
                let server = self.server(&project.server_id)?;
                let mut list = server.docker_command(false)?;
                if kind == "container" {
                    list.args(["container", "ls", "-a", "--format", "{{.Names}}"]);
                } else {
                    list.args(["volume", "ls", "--format", "{{.Name}}"]);
                }
                let existing = run(list, 30).await?;
                for name in names {
                    if existing.lines().any(|line| line.trim() == name) {
                        let mut inspect = server.docker_command(false)?;
                        inspect.args([kind, "inspect", "--format", format, &name]);
                        ensure!(
                            run(inspect, 30).await?.trim() == expected,
                            "Reserved {kind} {name} already exists outside this project. Link the existing app instead."
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_deployment_versions_validate_before_creating_any_project() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into()).unwrap();
        let make = |method: &str, image: &str, allow_untested| CreateDeployment {
            version: Some(crate::versions::Selection {
                image: image.into(),
                allow_untested,
            }),
            app: "nextcloud".into(),
            method: method.into(),
            name: "Files".into(),
            server_id: "local".into(),
            inputs: BTreeMap::new(),
            acknowledgements: vec![],
        };
        assert!(
            store
                .create_deployment(make("docker", "nextcloud:34.0.5-apache", false))
                .is_err()
        );
        assert!(
            store
                .create_deployment(make("docker", "other/app:34.0.5", true))
                .is_err()
        );
        assert!(
            store
                .create_deployment(make("aio", "nextcloud:34.0.5-apache", true))
                .is_err()
        );
        assert!(store.read().unwrap().projects.is_empty());
        let (project, _) = store
            .create_deployment(make("docker", "nextcloud:34.0.5-apache", true))
            .unwrap();
        assert_eq!(project.services[0].image, "nextcloud:34.0.5-apache");
    }
    #[test]
    fn ownership_checks_are_derived_without_optional_metadata() {
        let compose = json!({"services":{"aio":{"image":"example","container_name":"nextcloud-aio-mastercontainer"}},"volumes":{"data":{"name":"nextcloud_aio_mastercontainer"},"local":null,"shared":{"external":true,"name":"shared-data"}}});
        let (containers, volumes) = owned_resource_names(&compose, "selfhost-example").unwrap();
        assert!(containers.contains("nextcloud-aio-mastercontainer"));
        assert!(volumes.contains("nextcloud_aio_mastercontainer"));
        assert!(volumes.contains("selfhost-example_local"));
        assert!(!volumes.contains("shared-data"));
        let mut reserved = compose.clone();
        reserved["x-selfhost"] = json!({"reserved_volumes":["shared-data"]});
        assert!(
            owned_resource_names(&reserved, "selfhost-example")
                .unwrap()
                .1
                .contains("shared-data")
        );
        let mut dynamic = compose;
        dynamic["volumes"]["data"]["name"] = json!("${VOLUME_NAME}");
        assert!(owned_resource_names(&dynamic, "selfhost-example").is_err());
    }
    #[test]
    fn deployment_default_is_explicit_and_unique_per_app() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let defaults = store.deployment_profiles().unwrap();
        assert!(defaults[&("nextcloud".into(), "docker".into())].default);
        assert!(!defaults[&("nextcloud".into(), "aio".into())].default);
        let overrides = directory.path().join("deployment-profiles");
        std::fs::create_dir(&overrides).unwrap();
        std::fs::write(overrides.join("duplicate.json"),serde_json::to_vec(&json!([{"app":"nextcloud","id":"other","name":"Other","description":"Test","recipe":"nextcloud","blueprint":null,"default":true}])).unwrap()).unwrap();
        assert!(store.deployment_profiles().is_err());
    }
    #[test]
    fn aio_requires_consent_and_keeps_fixed_resources_and_socket_scope() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into()).unwrap();
        let make = |acknowledgements| CreateDeployment {
            version: None,
            app: "nextcloud".into(),
            method: "aio".into(),
            name: "Files".into(),
            server_id: "local".into(),
            inputs: BTreeMap::new(),
            acknowledgements,
        };
        assert!(store.create_deployment(make(vec![])).is_err());
        let (project, _) = store
            .create_deployment(make(vec![
                "docker-socket".into(),
                "fixed-resources".into(),
                "aio-managed".into(),
            ]))
            .unwrap();
        let mut setup = store.setup(&project.id).unwrap();
        assert_eq!(
            setup.compose["services"]["aio"]["container_name"],
            "nextcloud-aio-mastercontainer"
        );
        assert_eq!(
            setup.compose["volumes"]["nextcloud_aio_mastercontainer"]["name"],
            "nextcloud_aio_mastercontainer"
        );
        setup.compose["x-selfhost"]["host_resources"]["docker_socket"] = json!(false);
        assert!(setup.validate().is_err());
        setup.compose["x-selfhost"]["host_resources"]["docker_socket"] = json!(true);
        setup.compose["services"]["aio"]["volumes"][1] = json!("/etc:/host:ro");
        assert!(setup.validate().is_err());
    }
    #[tokio::test]
    #[ignore = "Creates and removes one uniquely named empty volume on the local Docker engine"]
    async fn reserved_volume_guard_checks_actual_docker_ownership() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into()).unwrap();
        let volume = format!(
            "selfhost-deployment-verification-{}",
            crate::core::token(6).unwrap()
        );
        let setup = crate::setup::Setup {
            database: None,
            compose: json!({"services":{"app":{"image":"nginx:stable-alpine","volumes":["data:/data"]}},"volumes":{"data":{"name":volume}}}),
            environment: BTreeMap::new(),
            files: BTreeMap::new(),
        };
        let project = store
            .create_setup("Ownership verification", "local", setup)
            .unwrap();
        let server = crate::infrastructure::Server::local();
        let outcome: Result<()> = async {
            store.assert_deployment_start_safe(&project.id).await?;
            let mut create = server.docker_command(true)?;
            create.args([
                "volume",
                "create",
                "--label",
                "com.docker.compose.project=another-project",
                &volume,
            ]);
            run(create, 30).await?;
            ensure!(
                store
                    .assert_deployment_start_safe(&project.id)
                    .await
                    .is_err(),
                "Foreign volume was not rejected"
            );
            let mut remove = server.docker_command(true)?;
            remove.args(["volume", "rm", &volume]);
            run(remove, 30).await?;
            let mut create = server.docker_command(true)?;
            create.args([
                "volume",
                "create",
                "--label",
                &format!("com.docker.compose.project=selfhost-{}", project.id),
                &volume,
            ]);
            run(create, 30).await?;
            store.assert_deployment_start_safe(&project.id).await?;
            Ok(())
        }
        .await;
        let mut cleanup = server.docker_command(true).unwrap();
        cleanup.args(["volume", "rm", &volume]);
        let _ = run(cleanup, 30).await;
        outcome.unwrap();
    }
}
