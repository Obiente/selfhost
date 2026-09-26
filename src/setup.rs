//! Portable, user-owned Compose setups. Canonical configuration is one private
//! atomic JSON document; Docker files are materialized only when applying it.
use crate::{
    catalog::AppInfo,
    core::{Project, Service, Store, atomic_write, now, token},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    #[serde(default)]
    pub database: Option<crate::database::Binding>,
    pub compose: Value,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// Files are portable mounts under ./files/, never arbitrary host paths.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

pub fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
}
fn filename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn parse_compose(text: &str) -> Result<Value> {
    ensure!(
        text.len() <= 512 * 1024,
        "Compose configuration is too large"
    );
    let value: Value = serde_yaml::from_str(text).context("Invalid Compose YAML or JSON")?;
    Ok(value)
}

impl Setup {
    pub fn wait_seconds(&self) -> Result<u64> {
        match self.compose.pointer("/x-selfhost/wait_seconds") {
            Some(value) => {
                let seconds = value
                    .as_u64()
                    .context("Startup wait must be a whole number of seconds")?;
                ensure!(
                    (10..=900).contains(&seconds),
                    "Startup wait must be between 10 and 900 seconds"
                );
                Ok(seconds)
            }
            None => Ok(90),
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.wait_seconds()?;
        ensure!(
            serde_json::to_vec(self)?.len() <= 1024 * 1024,
            "Setup exceeds 1 MiB"
        );
        let services = self.compose["services"]
            .as_object()
            .context("Compose needs a services mapping")?;
        ensure!(
            !services.is_empty() && services.len() <= 64,
            "Choose between 1 and 64 services"
        );
        // Keep exports self-contained. Include/build/extends can otherwise read
        // arbitrary files that would be absent from the standalone export.
        ensure!(
            self.compose.get("include").is_none(),
            "Include Compose services directly"
        );
        for (key, value) in &self.environment {
            ensure!(
                !key.is_empty()
                    && !key.as_bytes()[0].is_ascii_digit()
                    && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "Invalid environment variable name"
            );
            ensure!(
                !value.contains(['\0', '\r', '\n']),
                "Use a config file for multiline values"
            );
        }
        for name in self.files.keys() {
            ensure!(
                filename(name),
                "Config filenames may contain letters, digits, dots, hyphens and underscores"
            );
        }
        for (name, service) in services {
            ensure!(slug(name), "Invalid Compose service name");
            let image = service["image"]
                .as_str()
                .context("Each service needs a container image")?;
            ensure!(
                !image.is_empty() && image.len() < 512 && !image.contains(['\r', '\n', '\0']),
                "Invalid image"
            );
            ensure!(
                service.get("build").is_none() && service.get("extends").is_none(),
                "Use prebuilt images and inline service definitions"
            );
            ensure!(
                service.get("env_file").is_none(),
                "Manage variables in Environment and reference them from Compose"
            );
            if let Some(mounts) = service.get("volumes") {
                for mount in mounts.as_array().context("Volumes must be a list")? {
                    let source = if let Some(text) = mount.as_str() {
                        text.split(':').next().unwrap_or("")
                    } else {
                        mount["source"].as_str().context("Volume needs a source")?
                    };
                    if source == "/var/run/docker.sock" {
                        ensure!(
                            self.compose
                                .pointer("/x-selfhost/host_resources/docker_socket")
                                == Some(&Value::Bool(true)),
                            "Docker socket access must be declared in x-selfhost.host_resources.docker_socket"
                        );
                        ensure!(
                            mount.as_str() == Some("/var/run/docker.sock:/var/run/docker.sock:ro"),
                            "Use the declared read-only Docker socket mount; Docker API access still grants control over the host"
                        );
                    } else if let Some(file) = source.strip_prefix("./files/") {
                        ensure!(
                            self.files.contains_key(file),
                            "Declare mounted config file {file}"
                        );
                    } else {
                        ensure!(
                            slug(source),
                            "Use a named volume or ./files/NAME for portable storage"
                        );
                    }
                }
            }
        }
        for kind in ["configs", "secrets"] {
            if let Some(items) = self.compose.get(kind) {
                for item in items
                    .as_object()
                    .context("Configs and secrets must be mappings")?
                    .values()
                {
                    if let Some(file) = item.get("file") {
                        let file = file
                            .as_str()
                            .and_then(|f| f.strip_prefix("./files/"))
                            .context("Config and secret files must use ./files/NAME")?;
                        ensure!(self.files.contains_key(file), "Missing file {file}");
                    }
                }
            }
        }
        Ok(())
    }
    pub fn dotenv(&self) -> String {
        self.environment
            .iter()
            .map(|(k, v)| {
                let escaped = v
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "$$");
                format!("{k}=\"{escaped}\"\n")
            })
            .collect()
    }
}

fn services(setup: &Setup, catalog: &[AppInfo]) -> Result<Vec<Service>> {
    setup.compose["services"]
        .as_object()
        .context("Missing services")?
        .iter()
        .map(|(id, s)| {
            let image = s["image"].as_str().context("Missing image")?.to_string();
            let port = s["ports"]
                .as_array()
                .and_then(|p| p.first())
                .and_then(|p| {
                    if let Some(p) = p.as_str() {
                        let parts: Vec<_> = p.split(':').collect();
                        if parts.len() >= 2 {
                            parts[parts.len() - 2].parse().ok()
                        } else {
                            None
                        }
                    } else {
                        p["published"]
                            .as_u64()
                            .and_then(|p| u16::try_from(p).ok())
                            .or_else(|| p["published"].as_str().and_then(|p| p.parse().ok()))
                    }
                })
                .unwrap_or(0);
            if let Some(name) = s.get("x-selfhost-app") {
                let name = name
                    .as_str()
                    .context("App binding must name a catalogue recipe")?;
                let definition = catalog
                    .iter()
                    .find(|app| app.id == name)
                    .context("Bound app recipe not found")?
                    .clone();
                ensure!(
                    crate::versions::image_repository(&image)?
                        == crate::versions::image_repository(&definition.image)?,
                    "Bound app image must use its recipe repository"
                );
                ensure!(
                    s.get("x-selfhost-integration").is_none(),
                    "Use the app binding or an explicit integration, not both"
                );
                return Ok(Service {
                    app: id.clone(),
                    image,
                    port,
                    definition,
                });
            }
            Ok(Service {
                app: id.clone(),
                image: image.clone(),
                port,
                definition: AppInfo {
                    deployment_only: false,
                    versions: None,
                    onboarding: None,
                    schema: 1,
                    id: id.clone(),
                    name: id.clone(),
                    description: "Custom service".into(),
                    category: "Custom".into(),
                    image,
                    port,
                    container_port: 0,
                    integration: s
                        .get("x-selfhost-integration")
                        .and_then(Value::as_str)
                        .map(|name| -> Result<_> {
                            ensure!(slug(name), "Invalid integration id");
                            if let Some(profile) = catalog
                                .iter()
                                .find(|app| app.id == name)
                                .and_then(|app| app.integration.clone())
                            {
                                profile.validate()?;
                                return Ok(profile);
                            }
                            let file = crate::catalog::BuiltinCatalog::get(&format!(
                                "integrations/{name}.json"
                            ))
                            .context("Integration not found")?;
                            let profile: crate::integrations::Profile =
                                serde_json::from_slice(&file.data)?;
                            profile.validate()?;
                            Ok(profile)
                        })
                        .transpose()?,
                    data_path: String::new(),
                    docs: String::new(),
                    icon: "/brand/selfhost-monogram.svg".into(),
                    secrets: vec![],
                    environment: BTreeMap::new(),
                    dashboard: None,
                    actions: vec![],
                    database: None,
                    command: vec![],
                },
            })
        })
        .collect()
}

impl Store {
    pub fn setup(&self, id: &str) -> Result<Setup> {
        if let Some(directory) = &self.standalone {
            return directory.read(id);
        }
        let project = self.project(id)?;
        if project.custom {
            return Ok(serde_json::from_slice(&fs::read(
                self.project_dir(id)?.join("setup.json"),
            )?)?);
        }
        let text = fs::read_to_string(self.project_dir(id)?.join(".env"))?;
        // Legacy generated environment files only contain unquoted hex values.
        let environment = text
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Ok(Setup {
            database: None,
            compose: crate::core::compose(&project)?,
            environment,
            files: BTreeMap::new(),
        })
    }
    pub fn create_setup(&self, name: &str, server_id: &str, mut setup: Setup) -> Result<Project> {
        ensure!(
            !name.trim().is_empty() && name.len() <= 64,
            "Choose a name of up to 64 characters"
        );
        let server = self.server(server_id)?;
        server.require_docker()?;
        ensure!(
            server_id == "local" || setup.files.is_empty(),
            "File mounts currently require the local Docker engine"
        );
        setup.validate()?;
        let id = format!("p-{}", token(5)?);
        setup.compose["name"] = json!(format!("selfhost-{id}"));
        let project = Project {
            custom: true,
            server_id: server_id.into(),
            id,
            name: name.trim().into(),
            services: services(&setup, &self.catalog)?,
            access: "custom".into(),
            created_at: now(),
        };
        let dir = self.project_dir(&project.id)?;
        crate::core::private_dir(&dir)?;
        atomic_write(&dir.join("setup.json"), &serde_json::to_vec_pretty(&setup)?)?;
        self.materialize_setup(&project)?;
        self.mutate(|data| {
            data.projects.push(project.clone());
            Ok(())
        })?;
        Ok(project)
    }
    pub fn save_setup(&self, id: &str, setup: Setup) -> Result<Project> {
        let _operation = self.project_lock(id)?;
        let current = self.project(id)?;
        let services = setup.compose["services"]
            .as_object()
            .context("Compose needs a services mapping")?;
        ensure!(
            current
                .services
                .iter()
                .all(|service| services.contains_key(&service.app)),
            "Remove apps using the reviewed removal flow before saving a configuration that omits them"
        );
        self.save_setup_locked(id, setup)
    }
    pub(crate) fn save_setup_locked(&self, id: &str, mut setup: Setup) -> Result<Project> {
        if self.standalone.is_some() {
            return self.save_standalone_setup(id, setup);
        }
        let mut project = self.project(id)?;
        ensure!(
            project.server_id == "local" || setup.files.is_empty(),
            "File mounts currently require the local Docker engine"
        );
        setup.validate()?;
        setup.compose["name"] = json!(format!("selfhost-{id}"));
        let previous_setup = self.setup(id)?;
        self.snapshot_inner(id)?;
        let updated = services(&setup, &self.catalog)?;
        // Preserve native labels/actions for services whose identity is retained.
        project.services = updated
            .into_iter()
            .map(|mut s| {
                if let Some(old) = project.services.iter().find(|o| o.app == s.app)
                    && previous_setup.compose["services"][&s.app].get("x-selfhost-app")
                        == setup.compose["services"][&s.app].get("x-selfhost-app")
                    && previous_setup.compose["services"][&s.app].get("x-selfhost-integration")
                        == setup.compose["services"][&s.app].get("x-selfhost-integration")
                    && (old.image == s.image
                        || matches!(
                            (crate::versions::image_repository(&old.image), crate::versions::image_repository(&s.image)),
                            (Ok(before), Ok(after)) if before == after
                        ))
                {
                    s.definition = old.definition.clone();
                }
                s
            })
            .collect();
        project.custom = true;
        project.access = "custom".into();
        atomic_write(
            &self.project_dir(id)?.join("setup.json"),
            &serde_json::to_vec_pretty(&setup)?,
        )?;
        self.mutate(|data| {
            *data
                .projects
                .iter_mut()
                .find(|p| p.id == id)
                .context("Project not found")? = project.clone();
            Ok(())
        })?;
        Ok(project)
    }
    pub(crate) fn materialize_setup(&self, project: &Project) -> Result<()> {
        let dir = self.project_dir(&project.id)?;
        let setup: Setup = serde_json::from_slice(&fs::read(dir.join("setup.json"))?)?;
        setup.validate()?;
        let files = dir.join("files");
        if files.exists() {
            ensure!(
                !fs::symlink_metadata(&files)?.file_type().is_symlink(),
                "Files directory must not be a symlink"
            );
        }
        crate::core::private_dir(&files)?;
        for (name, text) in &setup.files {
            atomic_write(&files.join(name), text.as_bytes())?;
        }
        atomic_write(&dir.join(".env"), setup.dotenv().as_bytes())?;
        atomic_write(
            &dir.join("compose.json"),
            &serde_json::to_vec_pretty(&setup.compose)?,
        )?;
        Ok(())
    }
    pub fn export_setup(&self, id: &str) -> Result<Vec<u8>> {
        let _operation = self.project_lock(id)?;
        let setup = self.setup(id)?;
        setup.validate()?;
        let mut archive = tar::Builder::new(Vec::new());
        let mut append = |path: &str, bytes: &[u8]| -> Result<()> {
            let mut h = tar::Header::new_gnu();
            h.set_size(bytes.len() as u64);
            h.set_mode(0o600);
            h.set_cksum();
            archive.append_data(&mut h, path, bytes)?;
            Ok(())
        };
        append("compose.json", &serde_json::to_vec_pretty(&setup.compose)?)?;
        append(".env", setup.dotenv().as_bytes())?;
        for (name, text) in &setup.files {
            append(&format!("files/{name}"), text.as_bytes())?;
        }
        append("README.txt",b"Run: docker compose -f compose.json up -d\nStop: docker compose -f compose.json stop\nLogs: docker compose -f compose.json logs\nThis directory contains credentials. Keep it private.\nNo selfhost account or process is required. Keep the Compose project name to reuse existing Docker volumes on the same engine.\nApplication data in volumes is NOT included. Back up and transfer volumes separately when changing engines. External databases and networks remain external dependencies.\n")?;
        archive.finish()?;
        Ok(archive.into_inner()?)
    }
    pub fn export_to(&self, id: &str, path: &Path) -> Result<()> {
        use std::io::Write;
        let bytes = self.export_setup(id)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).context("Choose a new export filename")?;
        file.write_all(&bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_editor_cannot_skip_reviewed_app_removal() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().into()).unwrap();
        let mut initial = example();
        initial.compose["services"]["worker"] = json!({"image":"nginx:stable-alpine"});
        let project = store.create_setup("Two apps", "local", initial).unwrap();
        let mut changed = store.setup(&project.id).unwrap();
        changed.compose["services"]
            .as_object_mut()
            .unwrap()
            .remove("worker");
        assert!(
            store
                .save_setup(&project.id, changed)
                .err()
                .unwrap()
                .to_string()
                .contains("reviewed removal")
        );
        assert_eq!(store.project(&project.id).unwrap().services.len(), 2);
        assert!(
            store.setup(&project.id).unwrap().compose["services"]
                .get("worker")
                .is_some()
        );
    }
    fn example() -> Setup {
        Setup {
            database: None,
            compose: json!({"services":{"web":{"image":"nginx:stable-alpine","environment":{"EXAMPLE":"${EXAMPLE}"},"volumes":["web-data:/data","./files/app.conf:/etc/example.conf:ro"]}},"volumes":{"web-data":{}}}),
            environment: BTreeMap::from([("EXAMPLE".into(), "private-value".into())]),
            files: BTreeMap::from([("app.conf".into(), "example = true\n".into())]),
        }
    }
    #[test]
    fn setup_export_and_restore_keep_files_environment_and_volume_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let project = store.create_setup("Example", "local", example()).unwrap();
        let before = store.setup(&project.id).unwrap();
        let snapshot = store.snapshot(&project.id).unwrap();
        let mut edited = before.clone();
        edited
            .environment
            .insert("EXAMPLE".into(), "changed".into());
        edited.files.insert("app.conf".into(), "new".into());
        store.save_setup(&project.id, edited).unwrap();
        store.restore_config(&snapshot.id).unwrap();
        let restored = store.setup(&project.id).unwrap();
        assert_eq!(restored.environment, before.environment);
        assert_eq!(restored.files, before.files);
        let archive = store.export_setup(&project.id).unwrap();
        let mut archive = tar::Archive::new(&archive[..]);
        let mut files = BTreeMap::new();
        for entry in archive.entries().unwrap() {
            use std::io::Read;
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            files.insert(path, text);
        }
        let compose: Value = serde_json::from_str(&files["compose.json"]).unwrap();
        assert_eq!(compose["name"], format!("selfhost-{}", project.id));
        assert!(files[".env"].contains("private-value"));
        assert_eq!(files["files/app.conf"], "example = true\n");
        assert!(
            !serde_json::to_string(&store.read().unwrap())
                .unwrap()
                .contains("private-value")
        );
        assert!(!files.contains_key("setup.json"));
    }
    #[test]
    fn app_binding_preserves_profiles_and_rejects_repository_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let mut setup = example();
        let recipe = store.catalog.iter().find(|app| app.id == "homarr").unwrap();
        setup.compose["services"]["web"]["image"] = json!(recipe.image);
        setup.compose["services"]["web"]["x-selfhost-app"] = json!("homarr");
        let bound = services(&setup, &store.catalog).unwrap();
        assert_eq!(bound[0].app, "web");
        assert!(bound[0].definition.onboarding.is_some());
        assert_eq!(bound[0].definition.container_port, recipe.container_port);
        setup.compose["services"]["web"]["x-selfhost-integration"] = json!("homarr");
        assert!(services(&setup, &store.catalog).is_err());
        setup.compose["services"]["web"]
            .as_object_mut()
            .unwrap()
            .remove("x-selfhost-integration");
        setup.compose["services"]["web"]["image"] = json!("unrelated/image:1");
        assert!(services(&setup, &store.catalog).is_err());
    }
    #[test]
    fn reject_missing_files_traversal_and_external_build_inputs() {
        let mut setup = example();
        setup.files.clear();
        assert!(setup.validate().is_err());
        let mut setup = example();
        setup.files.insert("../outside".into(), "secret".into());
        assert!(setup.validate().is_err());
        let mut setup = example();
        setup.compose["services"]["web"]["build"] = json!("../../outside");
        assert!(setup.validate().is_err());
        let mut setup = example();
        setup.environment.insert("BAD-NAME".into(), "x".into());
        assert!(setup.validate().is_err());
    }
    #[test]
    fn setup_edits_update_explicit_bindings_but_keep_unchanged_profiles_frozen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().into()).unwrap();
        let mut setup = example();
        let recipe = store.catalog.iter().find(|app| app.id == "homarr").unwrap();
        let original_name = recipe.name.clone();
        setup.compose["services"]["web"]["image"] = json!(recipe.image);
        let project = store.create_setup("Binding edits", "local", setup).unwrap();
        assert!(project.services[0].definition.onboarding.is_none());
        let mut bound = store.setup(&project.id).unwrap();
        bound.compose["services"]["web"]["x-selfhost-app"] = json!("homarr");
        let attached = store.save_setup(&project.id, bound).unwrap();
        assert!(attached.services[0].definition.onboarding.is_some());
        store
            .catalog
            .iter_mut()
            .find(|app| app.id == "homarr")
            .unwrap()
            .name = "New catalogue name".into();
        let unchanged = store
            .save_setup(&project.id, store.setup(&project.id).unwrap())
            .unwrap();
        assert_eq!(unchanged.services[0].definition.name, original_name);
        let mut detached = store.setup(&project.id).unwrap();
        detached.compose["services"]["web"]
            .as_object_mut()
            .unwrap()
            .remove("x-selfhost-app");
        let detached = store.save_setup(&project.id, detached).unwrap();
        assert!(detached.services[0].definition.onboarding.is_none());
        assert!(detached.services[0].definition.integration.is_none());
        assert!(detached.services[0].definition.actions.is_empty());
        let mut integrated = store.setup(&project.id).unwrap();
        integrated.compose["services"]["web"]["x-selfhost-integration"] = json!("homarr");
        let integrated = store.save_setup(&project.id, integrated).unwrap();
        assert!(integrated.services[0].definition.integration.is_some());
        let mut removed = store.setup(&project.id).unwrap();
        removed.compose["services"]["web"]
            .as_object_mut()
            .unwrap()
            .remove("x-selfhost-integration");
        assert!(
            store.save_setup(&project.id, removed).unwrap().services[0]
                .definition
                .integration
                .is_none()
        );
    }
}
