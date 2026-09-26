//! References to externally owned applications. Never enter managed Compose lifecycle paths.
use crate::core::{Store, atomic_write, now, run, token};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs::OpenOptions};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingAction {
    pub id: String,
    pub label: String,
    pub description: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub write: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub image_repositories: Vec<String>,
    #[serde(default)]
    pub container_hint: String,
    #[serde(default)]
    pub actions: Vec<ExistingAction>,
}
impl ExistingProfile {
    fn validate(&self) -> Result<()> {
        ensure!(
            crate::setup::slug(&self.id) && !self.name.is_empty(),
            "Invalid existing app profile"
        );
        ensure!(
            (!self.image_repositories.is_empty() || self.actions.is_empty())
                && self.image_repositories.iter().all(|r| !r.is_empty()
                    && r.len() < 256
                    && r.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))),
            "Container actions need exact image repositories"
        );
        let mut ids = std::collections::HashSet::new();
        for action in &self.actions {
            ensure!(
                crate::setup::slug(&action.id) && ids.insert(&action.id),
                "Invalid or duplicate existing app action"
            );
            ensure!(
                !action.argv.is_empty()
                    && action.argv.len() <= 64
                    && action
                        .argv
                        .iter()
                        .all(|v| !v.contains('\0') && v.len() <= 4096),
                "Invalid action command"
            );
            ensure!(
                action.user.as_ref().is_none_or(|u| !u.is_empty()
                    && u.len() <= 64
                    && u.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-:".contains(&b))),
                "Invalid action user"
            );
        }
        Ok(())
    }
    fn accepts(&self, image: &str) -> bool {
        self.image_repositories.iter().any(|r| {
            image == r
                || image
                    .strip_prefix(r)
                    .is_some_and(|suffix| suffix.starts_with(':') || suffix.starts_with('@'))
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ExistingApp {
    pub id: String,
    pub name: String,
    pub url: String,
    pub server_id: String,
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub image_id: String,
    pub profile: ExistingProfile,
    pub allowed_actions: Vec<String>,
    pub linked_at: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkExisting {
    pub profile: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub container: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingConsent {
    pub confirmation: String,
    pub allowed_actions: Vec<String>,
}
fn reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn app_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("Enter the app's full URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use an HTTP or HTTPS app URL without credentials, query or fragment"
    );
    ensure!(
        url.scheme() == "https"
            || matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        "App URLs require HTTPS unless using localhost"
    );
    Ok(())
}
impl Store {
    /// Discover only non-sensitive Docker list fields and recipe matches.
    pub async fn discover_existing(&self, server: &str, profile: &str) -> Result<Vec<Value>> {
        let profiles = self.existing_profiles()?;
        if !profile.is_empty() {
            ensure!(
                profiles.contains_key(profile),
                "Existing app profile not found"
            );
        }
        let mut command = self.server(server)?.docker_command(false)?;
        command.args(["ps","-a","--no-trunc","--format",r#"{"id":{{json .ID}},"name":{{json .Names}},"image":{{json .Image}},"status":{{json .Status}},"ports":{{json .Ports}}}"#]);
        let output = run(command, 30).await?;
        let mut candidates = Vec::new();
        for line in output.lines().filter(|l| !l.trim().is_empty()) {
            let mut row: Value = serde_json::from_str(line)?;
            let image = row["image"].as_str().context("Container image missing")?;
            let matches = profiles
                .values()
                .filter(|p| p.accepts(image))
                .map(|p| p.id.clone())
                .collect::<Vec<_>>();
            if !profile.is_empty() && !matches.iter().any(|id| id == profile) {
                continue;
            }
            ensure!(
                row["id"]
                    .as_str()
                    .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())),
                "Invalid container identity"
            );
            row["profiles"] = serde_json::json!(matches);
            candidates.push(row);
        }
        Ok(candidates)
    }
    pub async fn reconnect_existing_plan(
        &self,
        id: &str,
        server: &str,
        container: &str,
    ) -> Result<Value> {
        let app = self.existing_app(id)?;
        let info = self.inspect_existing_container(server, container).await?;
        ensure!(
            app.profile
                .accepts(info["image"].as_str().unwrap_or_default()),
            "Replacement image does not match this app profile"
        );
        ensure!(
            !self
                .existing_apps()?
                .iter()
                .any(|a| a.id != id && a.server_id == server && a.container_id == info["id"]),
            "Replacement is already linked"
        );
        let revision = reconnect_revision(&app, server, &info)?;
        Ok(
            serde_json::json!({"app":app.name,"url":app.url,"previous_server":app.server_id,"previous_container":app.container_id,"server":server,"replacement":info,"reset_management_permissions":true,"revision":revision}),
        )
    }
    pub async fn reconnect_existing(
        &self,
        id: &str,
        server: &str,
        container: &str,
        revision: &str,
    ) -> Result<ExistingApp> {
        let old = self.existing_app(id)?;
        let info = self.inspect_existing_container(server, container).await?;
        ensure!(
            old.profile
                .accepts(info["image"].as_str().unwrap_or_default()),
            "Replacement image does not match this app profile"
        );
        ensure!(
            reconnect_revision(&old, server, &info)? == revision,
            "The app or replacement changed; review it again"
        );
        self.change_existing(|apps| {
            ensure!(
                !apps
                    .iter()
                    .any(|a| a.id != id && a.server_id == server && a.container_id == info["id"]),
                "Replacement is already linked"
            );
            let app = apps
                .iter_mut()
                .find(|a| a.id == id)
                .context("Linked app not found")?;
            ensure!(
                reconnect_revision(app, server, &info)? == revision,
                "Linked app changed; review it again"
            );
            app.server_id = server.into();
            app.container_id = info["id"].as_str().context("Missing identity")?.into();
            app.container_name = info["name"]
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('/')
                .into();
            app.image = info["image"].as_str().context("Missing image")?.into();
            app.image_id = info["image_id"]
                .as_str()
                .context("Missing image identity")?
                .into();
            app.allowed_actions.clear();
            app.linked_at = now();
            Ok(app.clone())
        })
    }
    pub fn existing_profiles(&self) -> Result<BTreeMap<String, ExistingProfile>> {
        let mut profiles = BTreeMap::new();
        for name in crate::catalog::BuiltinCatalog::iter()
            .filter(|n| n.starts_with("existing/") && n.ends_with(".json"))
        {
            let profile: ExistingProfile =
                serde_json::from_slice(&crate::catalog::BuiltinCatalog::get(&name).unwrap().data)?;
            profile.validate()?;
            profiles.insert(profile.id.clone(), profile);
        }
        let directory = self.root.join("existing-profiles");
        if directory.exists() {
            for entry in std::fs::read_dir(directory)? {
                let path = entry?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    let profile: ExistingProfile = serde_json::from_slice(&std::fs::read(path)?)?;
                    profile.validate()?;
                    profiles.insert(profile.id.clone(), profile);
                }
            }
        }
        Ok(profiles)
    }
    pub fn existing_apps(&self) -> Result<Vec<ExistingApp>> {
        let path = self.root.join("existing-apps.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    fn change_existing<T>(
        &self,
        change: impl FnOnce(&mut Vec<ExistingApp>) -> Result<T>,
    ) -> Result<T> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("existing-apps.lock"))?;
        file.try_lock_exclusive()
            .context("Another existing app change is running")?;
        let mut apps = self.existing_apps()?;
        let result = change(&mut apps)?;
        atomic_write(
            &self.root.join("existing-apps.json"),
            &serde_json::to_vec_pretty(&apps)?,
        )?;
        Ok(result)
    }
    fn existing_app(&self, id: &str) -> Result<ExistingApp> {
        self.existing_apps()?
            .into_iter()
            .find(|a| a.id == id)
            .context("Linked app not found")
    }
    async fn inspect_existing_container(&self, server: &str, container: &str) -> Result<Value> {
        ensure!(reference(container), "Use a container name or full ID");
        let mut command = self.server(server)?.docker_command(false)?;
        // Only selected fields are returned. Docker environment variables and mount sources never leave the server.
        command.args(["inspect", "--type", "container", "--format", r#"{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Config.Image}},"image_id":{{json .Image}},"running":{{json .State.Running}},"status":{{json .State.Status}}}"#, container]);
        let data: Value = serde_json::from_str(&run(command, 30).await?)?;
        ensure!(
            data["id"]
                .as_str()
                .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())),
            "Docker returned an invalid container identity"
        );
        Ok(data)
    }
    pub async fn link_existing(&self, input: LinkExisting) -> Result<ExistingApp> {
        ensure!(
            !input.name.trim().is_empty() && input.name.chars().count() <= 64,
            "Choose an app name up to 64 characters"
        );
        app_url(&input.url)?;
        let profile = self
            .existing_profiles()?
            .remove(&input.profile)
            .context("Existing app profile not found")?;
        if input.container.is_empty() {
            ensure!(
                input.server_id.is_empty(),
                "Choose both a server and container, or neither"
            );
            let app = ExistingApp {
                id: format!("e-{}", token(6)?),
                name: input.name.trim().into(),
                url: input.url,
                server_id: String::new(),
                container_id: String::new(),
                container_name: String::new(),
                image: String::new(),
                image_id: String::new(),
                profile,
                allowed_actions: vec![],
                linked_at: now(),
            };
            return self.change_existing(|apps| {
                ensure!(
                    !apps.iter().any(|a| a.url == app.url),
                    "This URL is already linked"
                );
                apps.push(app.clone());
                Ok(app)
            });
        }
        let info = self
            .inspect_existing_container(&input.server_id, &input.container)
            .await?;
        let image = info["image"].as_str().context("Missing image reference")?;
        ensure!(
            profile.accepts(image),
            "This container image does not match the selected app profile"
        );
        let app = ExistingApp {
            id: format!("e-{}", token(6)?),
            name: input.name.trim().into(),
            url: input.url,
            server_id: input.server_id,
            container_id: info["id"].as_str().unwrap().into(),
            container_name: info["name"]
                .as_str()
                .unwrap_or("")
                .trim_start_matches('/')
                .into(),
            image: image.into(),
            image_id: info["image_id"]
                .as_str()
                .context("Missing image identity")?
                .into(),
            profile,
            allowed_actions: vec![],
            linked_at: now(),
        };
        self.change_existing(|apps| {
            ensure!(
                !apps
                    .iter()
                    .any(|a| a.server_id == app.server_id && a.container_id == app.container_id),
                "This container is already linked"
            );
            apps.push(app.clone());
            Ok(app)
        })
    }
    pub async fn inspect_existing(&self, id: &str) -> Result<Value> {
        let app = self.existing_app(id)?;
        ensure!(
            !app.container_id.is_empty(),
            "This app is linked by URL only. Add a Docker connection to inspect it."
        );
        let info = self
            .inspect_existing_container(&app.server_id, &app.container_id)
            .await?;
        verify_identity(&app, &info)?;
        Ok(info)
    }
    pub fn consent_existing(&self, id: &str, input: ExistingConsent) -> Result<ExistingApp> {
        self.change_existing(|apps| {
            let app = apps
                .iter_mut()
                .find(|a| a.id == id)
                .context("Linked app not found")?;
            ensure!(
                input.confirmation == app.name,
                "Type the exact app name to change management permissions"
            );
            ensure!(
                input.allowed_actions.iter().all(|id| app
                    .profile
                    .actions
                    .iter()
                    .any(|a| a.id == *id && a.write)),
                "Unknown management action"
            );
            if !input.allowed_actions.is_empty() {
                ensure!(
                    !self.server(&app.server_id)?.read_only,
                    "This server is read-only"
                );
            }
            app.allowed_actions = input.allowed_actions;
            Ok(app.clone())
        })
    }
    pub fn unlink_existing(&self, id: &str, confirmation: &str) -> Result<()> {
        self.change_existing(|apps| {
            let app = apps
                .iter()
                .find(|a| a.id == id)
                .context("Linked app not found")?;
            ensure!(
                confirmation == app.name,
                "Type the exact app name to unlink it"
            );
            apps.retain(|a| a.id != id);
            Ok(())
        })
    }
    pub async fn existing_action(
        &self,
        id: &str,
        action_id: &str,
        confirmation: &str,
    ) -> Result<String> {
        // Permission changes and unlink wait until the active action finishes. A grant cannot
        // be revoked during inspection and then still be used by a delayed command.
        let operation = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("existing-apps.lock"))?;
        operation
            .try_lock_exclusive()
            .context("Another linked app action or permission change is running")?;
        let app = self.existing_app(id)?;
        ensure!(
            !app.container_id.is_empty(),
            "Container actions require a verified Docker connection"
        );
        let action = app
            .profile
            .actions
            .iter()
            .find(|a| a.id == action_id)
            .context("Action not found")?;
        if action.write {
            ensure!(
                app.allowed_actions.iter().any(|a| a == action_id) && confirmation == app.name,
                "This action needs explicit permission and the exact app name"
            );
        }
        let info = self
            .inspect_existing_container(&app.server_id, &app.container_id)
            .await?;
        verify_identity(&app, &info)?;
        ensure!(
            info["running"] == true,
            "The linked container is not running"
        );
        let mut command = self.server(&app.server_id)?.docker_command(action.write)?;
        command.arg("exec");
        if let Some(user) = &action.user {
            command.args(["--user", user]);
        }
        command.arg(&app.container_id).args(&action.argv);
        // No shell or user-provided command arguments; immutable ID prevents following a reused container name.
        run(command, if action.write { 300 } else { 30 }).await
    }
    pub async fn existing_stats(&self, id: &str) -> Result<Value> {
        let app = self.existing_app(id)?;
        self.inspect_existing(id).await?;
        let mut command = self.server(&app.server_id)?.docker_command(false)?;
        command.args([
            "stats",
            "--no-stream",
            "--format",
            "{{json .}}",
            &app.container_id,
        ]);
        Ok(serde_json::from_str(&run(command, 30).await?)?)
    }
}
fn reconnect_revision(app: &ExistingApp, server: &str, info: &Value) -> Result<String> {
    use sha2::{Digest, Sha256};
    // Running/status are intentionally excluded: ordinary lifecycle transitions do not alter identity.
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            app,
            server,
            &info["id"],
            &info["image"],
            &info["image_id"],
            &info["name"]
        ))?)
    ))
}
fn verify_identity(app: &ExistingApp, info: &Value) -> Result<()> {
    ensure!(
        info["id"] == app.container_id && info["image_id"] == app.image_id,
        "The linked container identity changed. Review and link the replacement explicitly."
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn profiles_match_exact_repositories_and_validate_targets() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into()).unwrap();
        let profiles = store.existing_profiles().unwrap();
        let aio = &profiles["nextcloud-aio"];
        assert!(aio.accepts("ghcr.io/nextcloud-releases/aio-nextcloud:latest"));
        assert!(!aio.accepts("ghcr.io/nextcloud-releases/aio-nextcloud-attacker:latest"));
        assert!(!profiles["custom"].accepts("any-image:latest"));
        assert!(!reference("--privileged"));
        assert!(!reference("app;sh"));
        assert!(app_url("http://localhost:8080").is_ok());
        assert!(app_url("https://user:secret@example.test").is_err());
    }
    #[test]
    fn unlink_and_permissions_never_enter_project_lifecycle() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path().into()).unwrap();
        let app = ExistingApp {
            id: "e-test".into(),
            name: "Example files".into(),
            url: "https://files.example.test".into(),
            server_id: "local".into(),
            container_id: "a".repeat(64),
            container_name: "files".into(),
            image: "nextcloud:stable".into(),
            image_id: "sha256:example".into(),
            profile: store.existing_profiles().unwrap()["nextcloud"].clone(),
            allowed_actions: vec![],
            linked_at: now(),
        };
        let info = json!({"id":"b".repeat(64),"name":"/replacement","image":"nextcloud:stable","image_id":"sha256:replacement","running":true});
        let revision = reconnect_revision(&app, "local", &info).unwrap();
        let mut changed = info.clone();
        changed["image_id"] = json!("sha256:another");
        assert_ne!(
            revision,
            reconnect_revision(&app, "local", &changed).unwrap()
        );
        let mut stopped = info.clone();
        stopped["running"] = json!(false);
        assert_eq!(
            revision,
            reconnect_revision(&app, "local", &stopped).unwrap()
        );
        assert_ne!(
            revision,
            reconnect_revision(&app, "another-host", &info).unwrap()
        );
        let mut allowed = app.clone();
        allowed.allowed_actions.push("update-apps".into());
        assert_ne!(
            revision,
            reconnect_revision(&allowed, "local", &info).unwrap()
        );
        store
            .change_existing(|apps| {
                apps.push(app.clone());
                Ok(())
            })
            .unwrap();
        assert!(store.read().unwrap().projects.is_empty());
        assert!(
            verify_identity(&app, &json!({"id":"b".repeat(64),"image_id":app.image_id})).is_err()
        );
        assert!(
            store
                .consent_existing(
                    &app.id,
                    ExistingConsent {
                        confirmation: app.name.clone(),
                        allowed_actions: vec!["destroy".into()]
                    }
                )
                .is_err()
        );
        assert!(store.unlink_existing(&app.id, "wrong name").is_err());
        assert_eq!(store.existing_apps().unwrap().len(), 1);
        store.unlink_existing(&app.id, &app.name).unwrap();
        assert!(store.existing_apps().unwrap().is_empty());
    }
}
