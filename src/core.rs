use crate::catalog::{self, AppInfo};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::process::Command;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn token(bytes: usize) -> Result<String> {
    let mut data = vec![0; bytes];
    getrandom::fill(&mut data).map_err(|e| anyhow::anyhow!("Unable to generate a secret: {e}"))?;
    Ok(data.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub app: String,
    pub image: String,
    pub port: u16,
    pub definition: AppInfo,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditService {
    pub app: String,
    pub image: String,
    pub port: u16,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub custom: bool,
    #[serde(default = "crate::infrastructure::local_id")]
    pub server_id: String,
    pub id: String,
    pub name: String,
    pub services: Vec<Service>,
    pub access: String,
    pub created_at: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProject {
    #[serde(default = "crate::infrastructure::local_id")]
    pub server_id: String,
    pub name: String,
    pub apps: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditProject {
    pub name: String,
    pub services: Vec<EditService>,
    pub access: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub action: String,
    pub status: String,
    pub message: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub read: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub created_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub project_id: String,
    pub action: String,
    pub interval_hours: u32,
    pub enabled: bool,
    pub next_run: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub servers: Vec<crate::infrastructure::Server>,
    pub schema: u32,
    pub projects: Vec<Project>,
    pub activities: Vec<Activity>,
    pub snapshots: Vec<Snapshot>,
    pub schedules: Vec<Schedule>,
}
impl Default for Data {
    fn default() -> Self {
        Self {
            schema: 1,
            servers: vec![],
            projects: vec![],
            activities: vec![],
            snapshots: vec![],
            schedules: vec![],
        }
    }
}
#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
    pub catalog: Vec<AppInfo>,
    pub(crate) standalone: Option<crate::standalone::Directory>,
}

pub(crate) fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    {
        static SID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let sid = if let Some(sid) = SID.get() {
            sid.clone()
        } else {
            let output = std::process::Command::new("whoami")
                .args(["/user", "/fo", "csv", "/nh"])
                .output()?;
            ensure!(
                output.status.success(),
                "Cannot determine the current Windows account"
            );
            let text = String::from_utf8(output.stdout)?;
            let sid = text
                .split('"')
                .find(|v| {
                    v.starts_with("S-1-")
                        && v.bytes()
                            .all(|c| c.is_ascii_digit() || c == b'S' || c == b'-')
                })
                .context("Cannot determine the Windows account SID")?
                .to_owned();
            let _ = SID.set(sid.clone());
            sid
        };
        let output = std::process::Command::new("icacls")
            .arg(path)
            .args([
                "/inheritance:r",
                "/grant:r",
                &format!("*{sid}:(OI)(CI)F"),
                "*S-1-5-18:(OI)(CI)F",
            ])
            .output()?;
        ensure!(
            output.status.success(),
            "Cannot protect the private data directory"
        );
    }
    Ok(())
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temp =
        tempfile::NamedTempFile::new_in(path.parent().context("Missing parent directory")?)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "Invalid project identifier"
    );
    Ok(())
}
fn validate_edit(edit: &EditProject) -> Result<()> {
    ensure!(
        !edit.name.trim().is_empty() && edit.name.chars().count() <= 64,
        "Choose a project name between 1 and 64 characters"
    );
    ensure!(
        matches!(edit.access.as_str(), "local" | "lan"),
        "Access must be local or lan"
    );
    ensure!(!edit.services.is_empty(), "Choose at least one app");
    let mut apps = std::collections::HashSet::new();
    let mut ports = std::collections::HashSet::new();
    for service in &edit.services {
        ensure!(
            apps.insert(&service.app),
            "An app can only appear once in a project"
        );
        ensure!(
            service.port >= 1024 && ports.insert(service.port),
            "Apps need distinct ports between 1024 and 65535"
        );
        ensure!(
            service.image.contains(':')
                && service.image.len() <= 256
                && service
                    .image
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/._-:@".contains(&b)),
            "Use a valid image reference with a tag or digest"
        );
    }
    Ok(())
}
pub fn compose(project: &Project) -> Result<Value> {
    let mut services = serde_json::Map::new();
    let mut volumes = serde_json::Map::new();
    for service in &project.services {
        let definition = &service.definition;
        let volume = format!("{}-data", service.app);
        let bind = if project.access == "lan" {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        let mut entry = json!({"image": service.image, "restart": "unless-stopped", "ports": [format!("{bind}:{}:{}", service.port, definition.container_port)], "volumes": [format!("{volume}:{}", definition.data_path)], "logging": {"driver": "json-file", "options": {"max-size": "10m", "max-file": "3"}} });
        let mut environment = definition.environment.clone();
        if !definition.command.is_empty() {
            entry["command"] = json!(definition.command);
        }
        for secret in &definition.secrets {
            environment.insert(
                secret.environment.clone(),
                format!(
                    "${{{}:?Missing application secret}}",
                    catalog::secret_key(definition, &secret.environment)
                ),
            );
        }
        if !environment.is_empty() {
            entry["environment"] = json!(environment);
        }
        services.insert(service.app.clone(), entry);
        volumes.insert(volume, json!({}));
    }
    Ok(
        json!({"name": format!("selfhost-{}", project.id), "services": services, "volumes": volumes}),
    )
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        Self::with_catalog(root, None)
    }
    pub fn with_catalog(root: PathBuf, directory: Option<&Path>) -> Result<Self> {
        ensure!(
            !root.join("standalone.json").exists(),
            "This is private directory-helper metadata. Use selfhost app --directory with the parent application directory"
        );
        private_dir(&root)?;
        Ok(Self {
            root,
            catalog: catalog::load(directory)?,
            standalone: None,
        })
    }
    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("state.lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn read_unlocked(&self) -> Result<Data> {
        let path = self.root.join("state.json");
        if !path.exists() {
            return Ok(Data::default());
        }
        let data: Data =
            serde_json::from_slice(&fs::read(path)?).context("Unable to read selfhost state")?;
        ensure!(
            data.schema == 1,
            "This state was written by an incompatible selfhost version"
        );
        Ok(data)
    }
    pub fn read(&self) -> Result<Data> {
        let _lock = self.lock()?;
        let mut data = self.read_unlocked()?;
        if let Some(directory) = &self.standalone {
            directory.refresh_projects(&mut data)?;
        }
        Ok(data)
    }
    pub(crate) fn mutate<T>(&self, f: impl FnOnce(&mut Data) -> Result<T>) -> Result<T> {
        let _lock = self.lock()?;
        let mut data = self.read_unlocked()?;
        let result = f(&mut data)?;
        atomic_write(
            &self.root.join("state.json"),
            &serde_json::to_vec_pretty(&data)?,
        )?;
        Ok(result)
    }
    pub fn project(&self, id: &str) -> Result<Project> {
        validate_id(id)?;
        self.read()?
            .projects
            .into_iter()
            .find(|p| p.id == id)
            .context("Project not found")
    }
    pub fn project_dir(&self, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        Ok(self.root.join("projects").join(id))
    }
    pub(crate) fn project_lock(&self, id: &str) -> Result<File> {
        self.project(id)?;
        ensure!(
            !self.project_dir(id)?.join("migration.json").exists(),
            "A migration needs completion or recovery before this project can change"
        );
        ensure!(
            !self.project_dir(id)?.join("removal.pending.json").exists(),
            "An interrupted removal needs reconciliation from the removal archive before this project can change"
        );
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.project_dir(id)?.join("operation.lock"))?;
        file.try_lock_exclusive()
            .context("Another action is already running for this project")?;
        // Recheck after acquiring the lock: another operation can finish and leave
        // a recovery marker between the optimistic checks above and this acquisition.
        self.project(id)?;
        ensure!(
            !self.project_dir(id)?.join("migration.json").exists(),
            "A migration needs completion or recovery before this project can change"
        );
        ensure!(
            !self.project_dir(id)?.join("removal.pending.json").exists(),
            "An interrupted removal needs reconciliation from the removal archive before this project can change"
        );
        Ok(file)
    }
    pub fn create(&self, input: CreateProject) -> Result<Project> {
        self.create_versioned(input, std::collections::BTreeMap::new())
    }
    pub fn create_versioned(
        &self,
        input: CreateProject,
        selections: std::collections::BTreeMap<String, crate::versions::Selection>,
    ) -> Result<Project> {
        ensure!(
            selections.keys().all(|id| input.apps.contains(id)),
            "Version selection names an app outside this project"
        );
        let destination = self.server(&input.server_id)?;
        destination.require_docker()?;
        let mut services = vec![];
        for id in &input.apps {
            let a = self
                .catalog
                .iter()
                .find(|a| a.id == *id)
                .context("Unknown app")?;
            ensure!(
                !a.deployment_only,
                "{} requires its deployment configuration. Use selfhost app init {} or choose its deployment method in the dashboard",
                a.name,
                a.id
            );
            services.push(EditService {
                app: id.clone(),
                image: if let Some(selection) = selections.get(id) {
                    crate::versions::select(a, selection)?;
                    selection.image.clone()
                } else {
                    a.image.clone()
                },
                port: a.port,
            });
        }
        let mut edit = EditProject {
            name: input.name.trim().into(),
            services,
            access: "local".into(),
        };
        validate_edit(&edit)?;
        self.mutate(|data| {
            let mut used: std::collections::HashSet<u16> = data
                .projects
                .iter()
                .filter(|p| p.server_id == input.server_id)
                .flat_map(|p| p.services.iter().map(|s| s.port))
                .collect();
            for service in &mut edit.services {
                while used.contains(&service.port) {
                    service.port = service.port.checked_add(1).context("No available port")?;
                }
                used.insert(service.port);
            }
            let services = edit
                .services
                .into_iter()
                .map(|s| {
                    let definition = self
                        .catalog
                        .iter()
                        .find(|a| a.id == s.app)
                        .context("Unknown app")?
                        .clone();
                    Ok(Service {
                        app: s.app,
                        image: s.image,
                        port: s.port,
                        definition,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let project = Project {
                custom: false,
                server_id: input.server_id.clone(),
                id: format!("p-{}", token(5)?),
                name: edit.name,
                services,
                access: edit.access,
                created_at: now(),
            };
            let dir = self.project_dir(&project.id)?;
            private_dir(&dir)?;
            let mut environment = String::new();
            for service in &project.services {
                for secret in &service.definition.secrets {
                    environment.push_str(&format!(
                        "{}={}\n",
                        catalog::secret_key(&service.definition, &secret.environment),
                        token(secret.bytes)?
                    ));
                }
            }
            atomic_write(&dir.join(".env"), environment.as_bytes())?;
            atomic_write(
                &dir.join("compose.json"),
                &serde_json::to_vec_pretty(&compose(&project)?)?,
            )?;
            data.projects.push(project.clone());
            Ok(project)
        })
    }
    pub fn edit(&self, id: &str, edit: EditProject) -> Result<Project> {
        ensure!(
            !self.project(id)?.custom,
            "Use the setup editor for custom projects"
        );
        validate_edit(&edit)?;
        let _operation = self.project_lock(id)?;
        let server_id = self.project(id)?.server_id;
        self.mutate(|data| {
            for service in &edit.services {
                ensure!(
                    !data
                        .projects
                        .iter()
                        .filter(|p| p.id != id && p.server_id == server_id)
                        .any(|p| p.services.iter().any(|s| s.port == service.port)),
                    "Port {} is assigned to another project",
                    service.port
                );
            }
            let project = data
                .projects
                .iter_mut()
                .find(|p| p.id == id)
                .context("Project not found")?;
            ensure!(
                edit.services.len() == project.services.len()
                    && edit
                        .services
                        .iter()
                        .all(|s| project.services.iter().any(|old| old.app == s.app)),
                "App membership cannot be changed in this version"
            );
            project.name = edit.name.trim().into();
            for service in edit.services {
                let existing = project
                    .services
                    .iter_mut()
                    .find(|s| s.app == service.app)
                    .context("App not found")?;
                existing.image = service.image;
                existing.port = service.port;
            }
            project.access = edit.access;
            Ok(project.clone())
        })
    }
    pub fn render(&self, id: &str) -> Result<Value> {
        let project = self.project(id)?;
        if project.custom {
            return Ok(self.setup(id)?.compose);
        }
        compose(&project)
    }
    pub(crate) fn write_compose(&self, project: &Project) -> Result<()> {
        if self.standalone.is_some() {
            self.setup(&project.id)?;
            return Ok(());
        }
        if project.custom {
            return self.materialize_setup(project);
        }
        atomic_write(
            &self.project_dir(&project.id)?.join("compose.json"),
            &serde_json::to_vec_pretty(&compose(project)?)?,
        )
    }
    #[cfg(test)]
    pub fn snapshot(&self, id: &str) -> Result<Snapshot> {
        let _operation = self.project_lock(id)?;
        self.snapshot_inner(id)
    }
    pub(crate) fn snapshot_inner(&self, id: &str) -> Result<Snapshot> {
        let project = self.project(id)?;
        let snapshot = Snapshot {
            id: format!("s-{}", token(6)?),
            project_id: id.into(),
            project_name: project.name.clone(),
            created_at: now(),
        };
        let dir = self.root.join("snapshots");
        private_dir(&dir)?;
        let secret = fs::read_to_string(self.runtime_dir(id)?.join(".env"))?;
        let setup = if project.custom {
            Some(self.setup(id)?)
        } else {
            None
        };
        atomic_write(
            &dir.join(format!("{}.json", snapshot.id)),
            &serde_json::to_vec_pretty(
                &json!({"project": project, "environment": secret, "setup": setup}),
            )?,
        )?;
        self.mutate(|data| {
            data.snapshots.push(snapshot.clone());
            Ok(())
        })?;
        Ok(snapshot)
    }
    pub fn restore_config(&self, snapshot_id: &str) -> Result<Project> {
        validate_id(snapshot_id)?;
        let value: Value = serde_json::from_slice(&fs::read(
            self.root
                .join("snapshots")
                .join(format!("{snapshot_id}.json")),
        )?)?;
        let mut saved: Project = serde_json::from_value(value["project"].clone())?;
        saved.server_id = self.project(&saved.id)?.server_id;
        let _operation = self.project_lock(&saved.id)?;
        self.snapshot_inner(&saved.id)?;
        if saved.custom {
            let setup: crate::setup::Setup = serde_json::from_value(value["setup"].clone())?;
            setup.validate()?;
            atomic_write(
                &self.project_dir(&saved.id)?.join("setup.json"),
                &serde_json::to_vec_pretty(&setup)?,
            )?;
        }
        // Restore desired configuration only. A subsequent Start applies it to containers.
        self.mutate(|data| {
            ensure!(
                !data
                    .projects
                    .iter()
                    .filter(|p| p.id != saved.id && p.server_id == saved.server_id)
                    .any(|p| p
                        .services
                        .iter()
                        .any(|s| saved.services.iter().any(|old| old.port == s.port))),
                "A saved port is now used by another project"
            );
            let current = data
                .projects
                .iter_mut()
                .find(|p| p.id == saved.id)
                .context("Project not found")?;
            let environment = value["environment"]
                .as_str()
                .context("Snapshot is missing its environment")?;
            atomic_write(
                &self.project_dir(&saved.id)?.join(".env"),
                environment.as_bytes(),
            )?;
            *current = saved.clone();
            Ok(saved.clone())
        })
    }
    pub fn schedule(&self, mut schedule: Schedule) -> Result<Schedule> {
        let _operation = self.project_lock(&schedule.project_id)?;
        self.project(&schedule.project_id)?;
        if !matches!(schedule.action.as_str(), "snapshot" | "refresh") {
            let parts: Vec<_> = schedule.action.split(':').collect();
            ensure!(
                parts.len() == 3 && parts[0] == "app",
                "Invalid scheduled action"
            );
            let profile = self.integration(&schedule.project_id, parts[1])?;
            ensure!(
                profile
                    .actions
                    .iter()
                    .any(|a| a.id == parts[2] && a.schedulable),
                "This app action cannot be scheduled"
            );
        }
        ensure!(
            (1..=8760).contains(&schedule.interval_hours),
            "Interval must be between 1 and 8760 hours"
        );
        schedule.next_run = now() + u64::from(schedule.interval_hours) * 3600;
        self.mutate(|data| {
            data.schedules
                .retain(|s| s.project_id != schedule.project_id || s.action != schedule.action);
            data.schedules.push(schedule.clone());
            Ok(schedule)
        })
    }
    pub async fn scheduled_action(&self, project: &str, action: &str) -> Result<()> {
        if let Some(rest) = action.strip_prefix("app:") {
            let (service, action) = rest.split_once(':').context("Invalid scheduled action")?;
            let profile = self.integration(project, service)?;
            ensure!(
                profile
                    .actions
                    .iter()
                    .any(|a| a.id == action && a.schedulable),
                "App action is no longer schedulable"
            );
            let outcome = self
                .integration_action(project, service, action, Default::default())
                .await?;
            ensure!(
                outcome["ok"] == true,
                "Scheduled app workflow did not complete"
            );
            Ok(())
        } else {
            self.action(project, action).await
        }
    }
    pub fn due(&self) -> Result<Vec<Schedule>> {
        self.mutate(|data| {
            let mut due = vec![];
            for schedule in &mut data.schedules {
                if schedule.enabled && schedule.next_run <= now() {
                    due.push(schedule.clone());
                    schedule.next_run = now() + u64::from(schedule.interval_hours) * 3600;
                }
            }
            Ok(due)
        })
    }
    pub fn mark_read(&self) -> Result<()> {
        self.mutate(|data| {
            for a in &mut data.activities {
                a.read = true;
            }
            Ok(())
        })
    }
    pub async fn action(&self, id: &str, action: &str) -> Result<()> {
        ensure!(
            matches!(
                action,
                "start" | "stop" | "restart" | "refresh" | "snapshot"
            ),
            "Unknown action"
        );
        let _operation = self.project_lock(id)?;
        let project = self.project(id)?;
        if matches!(action, "start" | "refresh") {
            self.assert_deployment_start_safe(id).await?;
        }
        if project.custom && matches!(action, "start" | "refresh") {
            self.check_database_ready(id)?;
        }
        if action != "snapshot" {
            self.project_docker(&project, true)?;
        }
        let activity_id = token(8)?;
        self.mutate(|data| {
            data.activities.push(Activity {
                id: activity_id.clone(),
                project_id: id.into(),
                project_name: project.name.clone(),
                action: action.into(),
                status: "running".into(),
                message: String::new(),
                started_at: now(),
                finished_at: None,
                read: false,
            });
            if data.activities.len() > 200 {
                data.activities.remove(0);
            }
            Ok(())
        })?;
        let outcome: Result<String> = async {
            if action == "snapshot" {
                self.snapshot_inner(id)?;
                return Ok(
                    "Configuration snapshot saved. Application data is not included.".into(),
                );
            }
            self.write_compose(&project)?;
            let wait = self.setup(&project.id)?.wait_seconds()?.to_string();
            match action {
                "start" => {
                    self.docker(&project, &["up", "-d", "--wait", "--wait-timeout", &wait])
                        .await?;
                }
                "stop" => {
                    self.docker(&project, &["stop"]).await?;
                }
                "restart" => {
                    self.docker(&project, &["restart"]).await?;
                }
                "refresh" => {
                    self.snapshot_inner(id)?;
                    self.docker(&project, &["pull"]).await?;
                    self.docker(&project, &["up", "-d", "--wait", "--wait-timeout", &wait])
                        .await?;
                }
                _ => unreachable!(),
            }
            Ok(match action {
                "start" => "Services started",
                "stop" => "Services stopped. Volumes retained.",
                "restart" => "Services restarted",
                _ => "Images refreshed using the configured tags",
            }
            .into())
        }
        .await;
        self.mutate(|data| {
            if let Some(activity) = data.activities.iter_mut().find(|a| a.id == activity_id) {
                activity.status = if outcome.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                }
                .into();
                activity.message = match &outcome {
                    Ok(s) => s.clone(),
                    Err(_) => "Action failed. See the current action output for details.".into(),
                };
                activity.finished_at = Some(now());
            }
            Ok(())
        })?;
        outcome.map(|_| ())
    }
    pub(crate) async fn docker(&self, project: &Project, args: &[&str]) -> Result<String> {
        let dir = self.runtime_dir(&project.id)?;
        let mut command = self.project_docker(project, true)?;
        command
            .current_dir(dir)
            .args([
                "compose",
                "--ansi",
                "never",
                "--project-name",
                &format!("selfhost-{}", project.id),
                "--file",
                if self.standalone.is_some() {
                    "compose.yaml"
                } else {
                    "compose.json"
                },
                "--env-file",
                ".env",
            ])
            .args(args);
        let timeout = if args.first() == Some(&"up") {
            600 + self.setup(&project.id)?.wait_seconds()?
        } else {
            600
        };
        run(command, timeout).await
    }
    fn service(&self, id: &str, app: &str) -> Result<(Project, Service)> {
        let project = self.project(id)?;
        let service = project
            .services
            .iter()
            .find(|s| s.app == app)
            .context("Service not found in this project")?
            .clone();
        Ok((project, service))
    }
    pub(crate) async fn service_container(&self, id: &str, app: &str) -> Result<Option<String>> {
        let (project, _) = self.service(id, app)?;
        let mut command = self.project_docker(&project, false)?;
        command.args([
            "ps",
            "-a",
            "--filter",
            &format!("label=com.docker.compose.project=selfhost-{id}"),
            "--filter",
            &format!("label=com.docker.compose.service={app}"),
            "--filter",
            "label=com.docker.compose.oneoff=False",
            "--format",
            "{{.ID}}",
        ]);
        let output = run(command, 10).await?;
        let ids: Vec<_> = output.lines().filter(|s| !s.is_empty()).collect();
        ensure!(ids.len() <= 1, "Multiple containers found for this service");
        if let Some(container) = ids.first() {
            ensure!(
                container.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid container identifier"
            );
        }
        Ok(ids.first().map(|s| s.to_string()))
    }
    pub async fn service_stats(&self, id: &str, app: &str) -> Result<Value> {
        let Some(container) = self.service_container(id, app).await? else {
            return Ok(json!({"sample": null, "message": "Start this service to see its stats."}));
        };
        let mut state = self.project_docker(&self.project(id)?, false)?;
        state.args(["inspect", "--format", "{{.State.Running}}", &container]);
        if run(state, 10).await?.trim() != "true" {
            return Ok(json!({"sample": null, "message": "This service is stopped."}));
        }
        let mut command = self.project_docker(&self.project(id)?, false)?;
        command.args(["stats", "--no-stream", "--format", "{{json .}}", &container]);
        let output = run(command, 15).await?;
        let sample: Value =
            serde_json::from_str(output.trim()).context("Invalid Docker stats response")?;
        Ok(json!({"sample": sample, "sampled_at": now()}))
    }
    pub async fn service_logs(&self, id: &str, app: &str) -> Result<String> {
        let Some(container) = self.service_container(id, app).await? else {
            return Ok("No container has been created yet.".into());
        };
        let mut command = self.project_docker(&self.project(id)?, false)?;
        command.args(["logs", "--timestamps", "--tail", "200", &container]);
        run_output(command, 15, true).await
    }
    pub async fn service_action(&self, id: &str, app: &str, action: &str) -> Result<String> {
        let _operation = self.project_lock(id)?;
        let (project, service) = self.service(id, app)?;
        if matches!(action, "start" | "update") {
            self.assert_deployment_start_safe(id).await?;
        }
        if project.custom && matches!(action, "start" | "update") {
            self.check_database_ready(id)?;
        }
        let custom = service.definition.actions.iter().find(|a| a.id == action);
        ensure!(
            ["start", "stop", "restart", "update"].contains(&action) || custom.is_some(),
            "This service does not define that action"
        );
        self.project_docker(&project, true)?;
        let label = custom.map(|a| a.label.as_str()).unwrap_or(action);
        let activity_id = token(8)?;
        self.mutate(|data| {
            data.activities.push(Activity {
                id: activity_id.clone(),
                project_id: id.into(),
                project_name: project.name.clone(),
                action: format!("{}: {label}", service.definition.name),
                status: "running".into(),
                message: String::new(),
                started_at: now(),
                finished_at: None,
                read: false,
            });
            if data.activities.len() > 200 {
                data.activities.remove(0);
            }
            Ok(())
        })?;
        let outcome: Result<String> = async {
            if let Some(recipe) = custom {
                let container = self
                    .service_container(id, app)
                    .await?
                    .context("Start this service before running its actions")?;
                let mut command = self.project_docker(&project, true)?;
                command.args(["exec", &container]).args(&recipe.command);
                return run_output(command, recipe.timeout_seconds, true).await;
            }
            self.write_compose(&project)?;
            let wait = self.setup(&project.id)?.wait_seconds()?.to_string();
            match action {
                "start" => {
                    self.docker(
                        &project,
                        &[
                            "up",
                            "-d",
                            "--no-deps",
                            "--wait",
                            "--wait-timeout",
                            &wait,
                            app,
                        ],
                    )
                    .await
                }
                "stop" => self.docker(&project, &["stop", app]).await,
                "restart" => self.docker(&project, &["restart", app]).await,
                "update" => {
                    self.snapshot_inner(id)?;
                    self.docker(&project, &["pull", app]).await?;
                    self.docker(
                        &project,
                        &[
                            "up",
                            "-d",
                            "--no-deps",
                            "--wait",
                            "--wait-timeout",
                            &wait,
                            app,
                        ],
                    )
                    .await
                }
                _ => unreachable!(),
            }
        }
        .await;
        self.mutate(|data| {
            if let Some(activity) = data.activities.iter_mut().find(|a| a.id == activity_id) {
                activity.status = if outcome.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                }
                .into();
                // Service output can contain credentials; display it only in the active session.
                activity.message = if outcome.is_ok() {
                    "Service action completed."
                } else {
                    "Service action failed. See the current action output for details."
                }
                .into();
                activity.finished_at = Some(now());
            }
            Ok(())
        })?;
        outcome
    }
    pub async fn statuses(&self) -> Result<Value> {
        self.server_statuses("local").await
    }
    pub async fn server_statuses(&self, server_id: &str) -> Result<Value> {
        let mut command = self.server(server_id)?.docker_command(false)?;
        command.args([
            "ps",
            "-a",
            "--filter",
            "label=com.docker.compose.project",
            "--format",
            "{{json .}}",
        ]);
        let output = run(command, 8).await?;
        let known: std::collections::HashSet<String> = self
            .read()?
            .projects
            .iter()
            .filter(|p| p.server_id == server_id)
            .map(|p| format!("selfhost-{}", p.id))
            .collect();
        let containers: Vec<Value> = output
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|container| {
                container["Labels"]
                    .as_str()
                    .unwrap_or_default()
                    .split(',')
                    .any(|l| {
                        l.strip_prefix("com.docker.compose.project=")
                            .is_some_and(|p| known.contains(p))
                    })
            })
            .collect();
        Ok(json!({"available": true, "containers": containers}))
    }
    pub async fn dashboard_sync(&self, id: &str, target: &str, key: String) -> Result<usize> {
        let _operation = self.project_lock(id)?;
        let project = self.project(id)?;
        ensure!(
            project.server_id == "local",
            "Remote dashboard sync requires an authenticated tunnel; use the app dashboard directly for now"
        );
        let dashboard = project
            .services
            .iter()
            .find(|s| s.app == target)
            .context("Dashboard app not found")?;
        let recipe = dashboard
            .definition
            .dashboard
            .as_ref()
            .context("This app does not define a dashboard integration")?;
        let key = if key.is_empty() {
            self.onboarding_key(id, target)?
        } else {
            key
        };
        ensure!(
            !key.trim().is_empty() && key.len() < 2048,
            "Enter the dashboard's API key"
        );
        let (base, container) = self.onboarding_target(id, target).await?;
        Self::dashboard_sync_http(
            &project,
            target,
            recipe,
            &base,
            &key,
            Some((self, id, container.as_str())),
        )
        .await
    }
    async fn dashboard_sync_http(
        project: &Project,
        target: &str,
        recipe: &catalog::Dashboard,
        base: &str,
        key: &str,
        guard: Option<(&Store, &str, &str)>,
    ) -> Result<usize> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let existing: Vec<Value> = client
            .get(format!("{base}{}", recipe.list_path))
            .header(&recipe.auth_header, key)
            .send()
            .await?
            .error_for_status()
            .context("The dashboard rejected access to its app catalog")?
            .json()
            .await?;
        let mut count = 0;
        for service in project.services.iter().filter(|s| s.app != target) {
            let info = &service.definition;
            let href = format!("http://localhost:{}", service.port);
            if existing
                .iter()
                .any(|a| a[&recipe.match_field].as_str() == Some(&href))
            {
                continue;
            }
            let body: std::collections::BTreeMap<_, _> = recipe
                .body
                .iter()
                .map(|(field, template)| {
                    let value = template
                        .replace("{name}", &info.name)
                        .replace("{description}", &info.description)
                        .replace("{icon}", &info.icon)
                        .replace("{url}", &href);
                    (field.clone(), value)
                })
                .collect();
            if let Some((store, id, container)) = guard {
                let (current_base, current_container) = store.onboarding_target(id, target).await?;
                ensure!(
                    current_base == base && current_container == container,
                    "Dashboard container changed during sync"
                );
            }
            client
                .post(format!("{base}{}", recipe.create_path))
                .header(&recipe.auth_header, key)
                .json(&body)
                .send()
                .await?
                .error_for_status()
                .context("The dashboard could not create an app")?;
            count += 1;
        }
        // An explicitly supplied API key is not persisted. Onboarded credentials stay in the private receipt.
        Ok(count)
    }
}

pub(crate) async fn run(command: Command, timeout: u64) -> Result<String> {
    run_output(command, timeout, false).await
}
async fn run_output(mut command: Command, timeout: u64, include_stderr: bool) -> Result<String> {
    use tokio::io::AsyncReadExt;
    async fn capture(mut stream: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let count = stream.read(&mut buffer).await?;
            if count == 0 {
                return Ok(bytes);
            }
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.len() > 262144 {
                bytes.drain(..bytes.len() - 262144);
            }
        }
    }
    command.kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .context("Command could not be started. Check that the required tool is on PATH.")?;
    let stdout = child.stdout.take().context("Missing stdout")?;
    let stderr = child.stderr.take().context("Missing stderr")?;
    let (status, stdout, stderr) = tokio::time::timeout(Duration::from_secs(timeout), async {
        tokio::try_join!(child.wait(), capture(stdout), capture(stderr))
    })
    .await
    .context("Timed out waiting for the command; a remote operation may still be running")??;
    ensure!(
        status.success(),
        "Command could not complete the action: {}",
        String::from_utf8_lossy(&stderr)
            .chars()
            .rev()
            .take(2500)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    );
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    if include_stderr && !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&stderr));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn service_actions_reject_unowned_services_and_unknown_actions() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = create(&store);
        assert!(
            store
                .service_action(&project.id, "another-service", "restart")
                .await
                .unwrap_err()
                .to_string()
                .contains("Service not found")
        );
        assert!(
            store
                .service_action(&project.id, "homarr", "arbitrary-command")
                .await
                .unwrap_err()
                .to_string()
                .contains("does not define")
        );
        assert!(store.read().unwrap().activities.is_empty());
    }
    fn create(store: &Store) -> Project {
        store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Test home".into(),
                apps: vec!["homarr".into(), "uptime-kuma".into()],
            })
            .unwrap()
    }
    #[test]
    fn projects_keep_ports_and_secrets_separate() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let a = create(&store);
        let b = create(&store);
        assert!(
            a.services
                .iter()
                .all(|s| b.services.iter().all(|t| s.port != t.port))
        );
        let first = fs::read_to_string(store.project_dir(&a.id).unwrap().join(".env")).unwrap();
        let second = fs::read_to_string(store.project_dir(&b.id).unwrap().join(".env")).unwrap();
        assert_ne!(first, second);
        assert!(
            !serde_json::to_string(&store.read().unwrap())
                .unwrap()
                .contains(first.trim().split('=').nth(1).unwrap())
        );
        let plan = store.render(&a.id).unwrap();
        assert_eq!(
            plan["services"]["homarr"]["ports"][0],
            "127.0.0.1:7575:7575"
        );
        assert!(!plan.to_string().contains("docker.sock"));
    }
    #[test]
    fn rejects_traversal_and_conflicting_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        assert!(store.project_dir("../../outside").is_err());
        let a = create(&store);
        let b = create(&store);
        assert!(
            store
                .edit(
                    &b.id,
                    EditProject {
                        name: "Conflict".into(),
                        services: a
                            .services
                            .into_iter()
                            .map(|s| EditService {
                                app: s.app,
                                image: s.image,
                                port: s.port
                            })
                            .collect(),
                        access: "local".into()
                    }
                )
                .is_err()
        );
    }
    #[test]
    fn snapshot_restores_config_and_preserves_secret() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = create(&store);
        let secret = fs::read(store.project_dir(&p.id).unwrap().join(".env")).unwrap();
        let backup = store.snapshot(&p.id).unwrap();
        store
            .edit(
                &p.id,
                EditProject {
                    name: "Changed".into(),
                    services: p
                        .services
                        .into_iter()
                        .map(|s| EditService {
                            app: s.app,
                            image: s.image,
                            port: s.port,
                        })
                        .collect(),
                    access: "lan".into(),
                },
            )
            .unwrap();
        let restored = store.restore_config(&backup.id).unwrap();
        assert_eq!(restored.name, "Test home");
        assert_eq!(restored.access, "local");
        assert_eq!(
            secret,
            fs::read(store.project_dir(&p.id).unwrap().join(".env")).unwrap()
        );
    }
    #[test]
    fn schedules_are_claimed_once_and_retain_disabled_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = create(&store);
        store
            .schedule(Schedule {
                project_id: p.id,
                action: "snapshot".into(),
                interval_hours: 1,
                enabled: true,
                next_run: 0,
            })
            .unwrap();
        store
            .mutate(|d| {
                d.schedules[0].next_run = 0;
                Ok(())
            })
            .unwrap();
        assert_eq!(store.due().unwrap().len(), 1);
        assert!(store.due().unwrap().is_empty());
    }

    #[test]
    fn catalog_updates_do_not_rewrite_existing_projects() {
        let data_dir = tempfile::tempdir().unwrap();
        let recipes = tempfile::tempdir().unwrap();
        let original = include_str!("../catalog/uptime-kuma.toml");
        fs::write(recipes.path().join("service.toml"), original).unwrap();
        let store = Store::with_catalog(data_dir.path().into(), Some(recipes.path())).unwrap();
        let before = create(&store);
        let plan = store.render(&before.id).unwrap();
        fs::write(
            recipes.path().join("service.toml"),
            original.replace("/app/data", "/new/location"),
        )
        .unwrap();
        let updated = Store::with_catalog(data_dir.path().into(), Some(recipes.path())).unwrap();
        assert_eq!(updated.render(&before.id).unwrap(), plan);
        let new_project = create(&updated);
        assert!(
            updated
                .render(&new_project.id)
                .unwrap()
                .to_string()
                .contains("/new/location")
        );
    }

    #[tokio::test]
    async fn dashboard_recipe_controls_paths_headers_body_and_duplicate_detection() {
        use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
        use std::sync::Arc;
        type Entries = Arc<tokio::sync::Mutex<Vec<Value>>>;
        async fn list(State(entries): State<Entries>, headers: HeaderMap) -> Json<Vec<Value>> {
            assert_eq!(headers["X-Test-Key"], "test-only-key");
            Json(entries.lock().await.clone())
        }
        async fn add(
            State(entries): State<Entries>,
            headers: HeaderMap,
            Json(value): Json<Value>,
        ) -> Json<Value> {
            assert_eq!(headers["X-Test-Key"], "test-only-key");
            entries.lock().await.push(value);
            Json(json!({"ok": true}))
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let entries = Entries::default();
        let router = Router::new()
            .route("/entries", get(list).post(add))
            .with_state(entries.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(directory.path().into()).unwrap();
        let mut dashboard = store.catalog[0].clone();
        dashboard.id = "test-dashboard".into();
        dashboard.port = port;
        dashboard.dashboard = Some(crate::catalog::Dashboard {
            list_path: "/entries".into(),
            create_path: "/entries".into(),
            auth_header: "X-Test-Key".into(),
            match_field: "address".into(),
            body: [
                ("title".into(), "{name}".into()),
                ("address".into(), "{url}".into()),
            ]
            .into(),
        });
        let mut service = store.catalog[1].clone();
        service.id = "test-service".into();
        service.name = "Test service".into();
        store.catalog = vec![dashboard, service];
        let project = store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Integration test".into(),
                apps: vec!["test-dashboard".into(), "test-service".into()],
            })
            .unwrap();
        assert_eq!(
            Store::dashboard_sync_http(
                &project,
                "test-dashboard",
                project.services[0].definition.dashboard.as_ref().unwrap(),
                &format!("http://127.0.0.1:{port}"),
                "test-only-key",
                None
            )
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            Store::dashboard_sync_http(
                &project,
                "test-dashboard",
                project.services[0].definition.dashboard.as_ref().unwrap(),
                &format!("http://127.0.0.1:{port}"),
                "test-only-key",
                None
            )
            .await
            .unwrap(),
            0
        );
        assert_eq!(entries.lock().await[0]["title"], "Test service");
        assert!(
            !fs::read_to_string(store.root.join("state.json"))
                .unwrap()
                .contains("test-only-key")
        );
        server.abort();
    }
}
