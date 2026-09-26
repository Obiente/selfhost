//! Explicit, reviewed recurring work. A persisted dispatch receipt prevents blind retries.
use crate::{
    core::{Store, atomic_write, now, private_dir, token},
    onboarding::{AppLink, OnboardingRequest},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    path::PathBuf,
};

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    #[default]
    DashboardLinks,
    ServiceAction,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub project_id: String,
    pub service: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRequest {
    pub name: String,
    #[serde(default)]
    pub kind: TaskKind,
    pub destination: Destination,
    #[serde(default)]
    pub managed: bool,
    #[serde(default)]
    pub existing: bool,
    /// Empty means all current and future managed projects, when managed is true.
    #[serde(default)]
    pub project_ids: Vec<String>,
    /// Empty means all current and future linked apps, when existing is true.
    #[serde(default)]
    pub existing_ids: Vec<String>,
    pub interval_seconds: u64,
    #[serde(default)]
    pub action: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub id: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub status: String,
    pub count: usize,
    pub message: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskRule {
    pub id: String,
    pub request: TaskRequest,
    pub enabled: bool,
    pub status: String,
    pub last_run: Option<u64>,
    pub next_run: u64,
    pub history: Vec<TaskRun>,
    pub pending: Option<TaskRun>,
    // Never export container identity or private onboarding fingerprint in API views.
    target_revision: String,
    container_id: String,
}
#[derive(Default, Serialize, Deserialize)]
struct Tasks {
    rules: Vec<TaskRule>,
}

fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn canonical_url(value: &str) -> Option<String> {
    let u = reqwest::Url::parse(value).ok()?;
    (matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none())
    .then(|| u.to_string())
}
fn loopback_host(value: &str) -> bool {
    value.eq_ignore_ascii_case("localhost")
        || value
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
}
fn public(rule: &TaskRule) -> Value {
    json!({"id":rule.id,"request":rule.request,"enabled":rule.enabled,"status":rule.status,
        "last_run":rule.last_run,"next_run":rule.next_run,"history":rule.history,"pending":rule.pending})
}
impl Store {
    fn tasks_lock(&self) -> Result<File> {
        private_dir(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("tasks.lock"))?;
        lock.try_lock_exclusive()
            .context("A task is running or changing; try again when it finishes")?;
        Ok(lock)
    }
    fn tasks_read(&self) -> Result<Tasks> {
        let p = self.root.join("tasks.json");
        if p.exists() {
            Ok(serde_json::from_slice(&std::fs::read(p)?)?)
        } else {
            Ok(Tasks::default())
        }
    }
    fn tasks_save(&self, data: &Tasks) -> Result<()> {
        atomic_write(
            &self.root.join("tasks.json"),
            &serde_json::to_vec_pretty(data)?,
        )
    }
    pub fn tasks(&self) -> Result<Value> {
        // Atomic replacement allows status reads while a remote task is running.
        Ok(json!(
            self.tasks_read()?
                .rules
                .iter()
                .map(public)
                .collect::<Vec<_>>()
        ))
    }
    pub fn task_destinations(&self) -> Result<Value> {
        let mut output = Vec::new();
        for p in self.read()?.projects {
            for s in &p.services {
                let supports_sync =
                    s.definition.onboarding.as_ref().is_some_and(|x| {
                        x.steps.iter().any(|s| s.modes.iter().any(|m| m == "sync"))
                    });
                let actions: Vec<_> = s
                    .definition
                    .actions
                    .iter()
                    .filter(|a| !a.confirm)
                    .map(|a| json!({"id":a.id,"label":a.label,"description":a.description}))
                    .collect();
                if !supports_sync && actions.is_empty() {
                    continue;
                }
                let info = self.onboarding_info(&p.id, &s.app)?;
                output.push(json!({"project_id":p.id,"service":s.app,"name":format!("{} / {}",p.name,s.definition.name),"supports_sync":supports_sync,"configured":info["state"]["completed"] == true && info["state"]["pending"].is_null(),"actions":actions}));
            }
        }
        Ok(json!(output))
    }
    fn task_validate(&self, request: &TaskRequest) -> Result<String> {
        ensure!(
            !request.name.trim().is_empty()
                && request.name.len() <= 100
                && !request.name.contains(['\n', '\r', '\0']),
            "Choose a task name up to 100 characters"
        );
        ensure!(
            (60..=2_592_000).contains(&request.interval_seconds),
            "Task intervals must be between one minute and 30 days"
        );
        ensure!(
            request.project_ids.len() <= 1000 && request.existing_ids.len() <= 1000,
            "Too many selected sources"
        );
        ensure!(
            (request.managed || request.project_ids.is_empty())
                && (request.existing || request.existing_ids.is_empty()),
            "Clear IDs for disabled source scopes"
        );
        let project = self.project(&request.destination.project_id)?;
        let service = project
            .services
            .iter()
            .find(|s| s.app == request.destination.service)
            .context("Destination service was removed")?;
        let server = self.server(&project.server_id)?;
        ensure!(!server.read_only, "Destination server is read-only");
        ensure!(
            !self
                .project_dir(&project.id)?
                .join("migration.json")
                .exists(),
            "Finish the project migration before running tasks"
        );
        let capability = match request.kind {
            TaskKind::DashboardLinks => {
                ensure!(
                    request.managed || request.existing,
                    "Choose at least one source scope"
                );
                ensure!(
                    request.action.is_empty(),
                    "Dashboard link tasks do not run a service action"
                );
                let profile = service
                    .definition
                    .onboarding
                    .as_ref()
                    .context("Destination does not support app onboarding")?;
                ensure!(
                    profile
                        .steps
                        .iter()
                        .any(|s| s.modes.iter().any(|m| m == "sync")),
                    "Destination does not support link synchronization"
                );
                let info = self.onboarding_info(&project.id, &service.app)?;
                ensure!(
                    info["state"]["completed"] == true && info["state"]["pending"].is_null(),
                    "Finish or recover destination onboarding before running tasks"
                );
                self.onboarding_destination_revision(&project.id, &service.app)?
            }
            TaskKind::ServiceAction => {
                ensure!(
                    !request.managed
                        && !request.existing
                        && request.project_ids.is_empty()
                        && request.existing_ids.is_empty(),
                    "Service action tasks do not use source scopes"
                );
                let action = service
                    .definition
                    .actions
                    .iter()
                    .find(|a| a.id == request.action)
                    .context("Choose a declared service action")?;
                ensure!(
                    !action.confirm,
                    "Actions requiring confirmation must be run interactively"
                );
                digest(action)?
            }
        };
        digest(&(service, server, capability))
    }
    fn task_links(&self, request: &TaskRequest) -> Result<Vec<AppLink>> {
        let mut output = Vec::new();
        let mut seen = BTreeSet::new();
        let d = &request.destination;
        let info = self.onboarding_info(&d.project_id, &d.service)?;
        if let Some(urls) = info["state"]["linked_urls"].as_array() {
            seen.extend(
                urls.iter()
                    .filter_map(Value::as_str)
                    .filter_map(canonical_url),
            );
        }
        if request.managed {
            for p in self.read()?.projects {
                if !request.project_ids.is_empty() && !request.project_ids.contains(&p.id) {
                    continue;
                }
                let server = self.server(&p.server_id)?;
                // Remote loopback points to the reader's computer, not the remote service.
                if server.app_host.is_empty()
                    || (p.server_id != "local" && loopback_host(&server.app_host))
                {
                    continue;
                }
                for s in p.services {
                    if p.id == d.project_id && s.app == d.service {
                        continue;
                    }
                    if p.server_id != "local" && p.access != "lan" {
                        continue;
                    }
                    let url = format!("http://{}:{}", server.app_host, s.port);
                    if let Some(key) = canonical_url(&url)
                        && seen.insert(key)
                    {
                        output.push(AppLink {
                            name: s.definition.name,
                            url,
                            icon: s.definition.icon,
                            description: s.definition.description,
                        });
                    }
                }
            }
        }
        if request.existing {
            for app in self.existing_apps()? {
                if !request.existing_ids.is_empty() && !request.existing_ids.contains(&app.id) {
                    continue;
                }
                let parsed = reqwest::Url::parse(&app.url)?;
                if app.server_id != "local" && parsed.host_str().is_some_and(loopback_host) {
                    continue;
                }
                if let Some(key) = canonical_url(&app.url)
                    && seen.insert(key)
                {
                    output.push(AppLink {
                        name: app.name,
                        url: app.url,
                        icon: String::new(),
                        description: "Linked existing application".into(),
                    });
                }
            }
        }
        // Stable batches keep each remote operation bounded. The next run picks up the rest.
        output.sort_by(|a, b| a.url.cmp(&b.url));
        output.truncate(100);
        Ok(output)
    }
    async fn task_preview(&self, request: &TaskRequest) -> Result<(String, String, Vec<AppLink>)> {
        let target = self.task_validate(request)?;
        let d = &request.destination;
        let container = if request.kind == TaskKind::DashboardLinks {
            self.onboarding_target(&d.project_id, &d.service).await?.1
        } else {
            self.service_container(&d.project_id, &d.service)
                .await?
                .context("Start the destination before creating a task")?
        };
        let apps = if request.kind == TaskKind::DashboardLinks {
            self.task_links(request)?
        } else {
            Vec::new()
        };
        Ok((target, container, apps))
    }
    pub async fn task_plan(&self, request: TaskRequest) -> Result<Value> {
        let (target, container, apps) = self.task_preview(&request).await?;
        let d = &request.destination;
        let revision = digest(&(&request, &target, &container, &apps))?;
        let action = if request.kind == TaskKind::ServiceAction {
            let project = self.project(&d.project_id)?;
            let service = project
                .services
                .iter()
                .find(|s| s.app == d.service)
                .context("Destination removed")?;
            serde_json::to_value(
                service
                    .definition
                    .actions
                    .iter()
                    .find(|a| a.id == request.action)
                    .context("Action removed")?,
            )?
        } else {
            Value::Null
        };
        let mut warnings = vec![
            "Tasks run while selfhost serve is running, or when you run them from the CLI. A changed destination requires a new review.",
        ];
        if request.kind == TaskKind::DashboardLinks {
            warnings.extend(["This task can write to the selected destination whenever it runs. It only adds links; it does not remove or rename existing links.","New matching sources are included automatically when a source ID list is empty. Private and localhost URLs may only work for browsers on the same network."]);
        } else {
            warnings.push("This task repeats the reviewed command inside the selected container. Command output is discarded; its completion status is recorded.");
        }
        Ok(
            json!({"revision":revision,"request":request,"destination":d,"apps":apps,"action":action,"warnings":warnings}),
        )
    }
    pub async fn task_create(&self, request: TaskRequest, revision: &str) -> Result<Value> {
        let _lock = self.tasks_lock()?;
        let (target_revision, container_id, apps) = self.task_preview(&request).await?;
        ensure!(
            digest(&(&request, &target_revision, &container_id, &apps))? == revision,
            "Task destination or sources changed; review a fresh plan"
        );
        let d = &request.destination;
        let mut data = self.tasks_read()?;
        ensure!(data.rules.len() < 100, "At most 100 tasks are supported");
        ensure!(
            !data
                .rules
                .iter()
                .any(|r| r.request.destination.project_id == d.project_id
                    && r.request.destination.service == d.service
                    && r.request.kind == request.kind
                    && r.request.action == request.action),
            "A task for this destination and action already exists"
        );
        let rule = TaskRule {
            id: format!("task-{}", token(8)?),
            request,
            enabled: true,
            status: "ready".into(),
            last_run: None,
            next_run: now(),
            history: vec![],
            pending: None,
            target_revision,
            container_id,
        };
        let output = public(&rule);
        data.rules.push(rule);
        self.tasks_save(&data)?;
        Ok(output)
    }
    pub fn task_set_enabled(&self, id: &str, enabled: bool) -> Result<Value> {
        let _lock = self.tasks_lock()?;
        let mut data = self.tasks_read()?;
        let rule = data
            .rules
            .iter_mut()
            .find(|r| r.id == id)
            .context("Task not found")?;
        if enabled {
            ensure!(
                rule.pending.is_none() && rule.status != "uncertain",
                "Inspect the uncertain remote operation before enabling this task"
            );
        }
        rule.enabled = enabled;
        if enabled {
            rule.next_run = now();
            rule.status = "ready".into();
        }
        let value = public(rule);
        self.tasks_save(&data)?;
        Ok(value)
    }
    pub fn task_remove(&self, id: &str, confirmation: &str) -> Result<()> {
        let _lock = self.tasks_lock()?;
        let mut data = self.tasks_read()?;
        let rule = data
            .rules
            .iter()
            .find(|r| r.id == id)
            .context("Task not found")?;
        ensure!(
            confirmation == rule.request.name,
            "Type the task name to remove it"
        );
        // Removing scheduling does not delete remote links or the onboarding recovery receipt.
        data.rules.retain(|r| r.id != id);
        self.tasks_save(&data)
    }
    pub async fn task_run(&self, id: &str) -> Result<Value> {
        let _lock = self.tasks_lock()?;
        let mut data = self.tasks_read()?;
        let index = data
            .rules
            .iter()
            .position(|r| r.id == id)
            .context("Task not found")?;
        ensure!(
            data.rules[index].enabled,
            "Enable this task before running it"
        );
        ensure!(
            data.rules[index].pending.is_none() && data.rules[index].status != "uncertain",
            "Task dispatch may already have reached the app. Inspect its receipt and remote resources; automatic retries are disabled"
        );
        let request = data.rules[index].request.clone();
        let d = &request.destination;
        let prepared: Result<(Vec<AppLink>, String)> = async {
            ensure!(
                self.task_validate(&request)? == data.rules[index].target_revision,
                "Destination configuration changed; remove this task and review a replacement"
            );
            let container = if request.kind == TaskKind::DashboardLinks {
                self.onboarding_target(&d.project_id, &d.service).await?.1
            } else {
                self.service_container(&d.project_id, &d.service)
                    .await?
                    .context("Destination is stopped")?
            };
            ensure!(
                container == data.rules[index].container_id,
                "Destination container changed; review a replacement task"
            );
            let apps = if request.kind == TaskKind::DashboardLinks {
                self.task_links(&request)?
            } else {
                Vec::new()
            };
            let revision = if request.kind == TaskKind::DashboardLinks && !apps.is_empty() {
                let plan = self
                    .onboarding_plan(
                        &d.project_id,
                        &d.service,
                        OnboardingRequest {
                            mode: "sync".into(),
                            inputs: Default::default(),
                            apps: apps.clone(),
                        },
                    )
                    .await?;
                plan["revision"]
                    .as_str()
                    .context("Missing onboarding revision")?
                    .to_owned()
            } else {
                String::new()
            };
            Ok((apps, revision))
        }
        .await;
        let mut run = TaskRun {
            id: token(8)?,
            started_at: now(),
            finished_at: None,
            status: "running".into(),
            count: 0,
            message: String::new(),
        };
        let (apps, revision) = match prepared {
            Ok(v) => v,
            Err(_) => {
                run.status = "blocked".into();
                run.message="Destination is unavailable, changed, read-only, or needs recovery. Review it before retrying.".into();
                run.finished_at = Some(now());
                finish(&mut data.rules[index], run);
                data.rules[index].enabled = false;
                self.tasks_save(&data)?;
                return Ok(public(&data.rules[index]));
            }
        };
        run.count = apps.len();
        if request.kind == TaskKind::DashboardLinks && apps.is_empty() {
            run.status = "idle".into();
            run.finished_at = Some(now());
            finish(&mut data.rules[index], run);
            self.tasks_save(&data)?;
            return Ok(public(&data.rules[index]));
        }
        // Durable dispatch intent is written BEFORE invoking any operation that can write remotely.
        data.rules[index].pending = Some(run.clone());
        data.rules[index].status = "running".into();
        self.tasks_save(&data)?;
        let result = if request.kind == TaskKind::DashboardLinks {
            self.task_dashboard_links(
                &request,
                &data.rules[index].target_revision,
                &data.rules[index].container_id,
                apps,
                &revision,
            )
            .await
            .map(|_| ())
        } else {
            self.task_action(
                &request,
                &data.rules[index].target_revision,
                &data.rules[index].container_id,
            )
            .await
        };
        run.finished_at = Some(now());
        if result.is_ok() {
            run.status = "complete".into();
            data.rules[index].pending = None;
        } else {
            run.status = "uncertain".into();
            run.message="The operation may have reached the app. Automatic retries are disabled; inspect its resources and private onboarding receipt.".into();
            data.rules[index].enabled = false;
        }
        finish(&mut data.rules[index], run);
        self.tasks_save(&data)?;
        Ok(public(&data.rules[index]))
    }
    async fn task_dashboard_links(
        &self,
        request: &TaskRequest,
        target: &str,
        container: &str,
        apps: Vec<AppLink>,
        revision: &str,
    ) -> Result<Value> {
        let d = &request.destination;
        let _operation = self.project_lock(&d.project_id)?;
        ensure!(
            self.task_validate(request)? == target,
            "Task destination changed before dispatch"
        );
        ensure!(
            self.onboarding_target(&d.project_id, &d.service).await?.1 == container,
            "Task container changed before dispatch"
        );
        self.onboarding_apply_locked(
            &d.project_id,
            &d.service,
            OnboardingRequest {
                mode: "sync".into(),
                inputs: Default::default(),
                apps,
            },
            revision,
        )
        .await
    }
    async fn task_action(
        &self,
        request: &TaskRequest,
        target: &str,
        container: &str,
    ) -> Result<()> {
        let d = &request.destination;
        let _operation = self.project_lock(&d.project_id)?;
        ensure!(
            self.task_validate(request)? == target,
            "Task destination changed before dispatch"
        );
        ensure!(
            self.service_container(&d.project_id, &d.service)
                .await?
                .as_deref()
                == Some(container),
            "Task container changed before dispatch"
        );
        let project = self.project(&d.project_id)?;
        let service = project
            .services
            .iter()
            .find(|s| s.app == d.service)
            .context("Destination removed")?;
        let action = service
            .definition
            .actions
            .iter()
            .find(|a| a.id == request.action && !a.confirm)
            .context("Task action changed")?;
        let mut inspect = self.project_docker(&project, false)?;
        inspect.args(["inspect", container]);
        let data: Value = serde_json::from_str(&crate::core::run(inspect, 10).await?)?;
        ensure!(
            data[0]["State"]["Running"] == true
                && data[0]["Config"]["Image"] == service.image
                && data[0]["Config"]["Labels"]["com.docker.compose.project"]
                    == format!("selfhost-{}", d.project_id)
                && data[0]["Config"]["Labels"]["com.docker.compose.service"] == d.service,
            "Task container ownership changed"
        );
        let mut command = self.project_docker(&project, true)?;
        command.args(["exec", container]).args(&action.command);
        crate::core::run(command, action.timeout_seconds).await?;
        Ok(())
    }
    pub async fn tasks_tick(&self) -> Result<Value> {
        {
            let _lock = self.tasks_lock()?;
            let mut data = self.tasks_read()?;
            let mut changed = false;
            for rule in &mut data.rules {
                if rule.status == "running"
                    && let Some(mut run) = rule.pending.clone()
                {
                    run.status = "uncertain".into();
                    run.finished_at = Some(now());
                    run.message = "Selfhost stopped before completion was saved. Inspect the destination; this operation will not be retried automatically.".into();
                    rule.enabled = false;
                    finish(rule, run);
                    changed = true;
                }
            }
            if changed {
                self.tasks_save(&data)?;
            }
        }
        let ids: Vec<_> = self
            .tasks_read()?
            .rules
            .into_iter()
            .filter(|r| {
                r.enabled && r.pending.is_none() && r.status != "uncertain" && r.next_run <= now()
            })
            .map(|r| r.id)
            .collect();
        let mut results = Vec::new();
        for id in ids {
            if let Ok(value) = Box::pin(self.task_run(&id)).await {
                results.push(value);
            }
        }
        Ok(json!(results))
    }
}
fn finish(rule: &mut TaskRule, run: TaskRun) {
    rule.status = run.status.clone();
    rule.last_run = run.finished_at;
    rule.next_run = now().saturating_add(rule.request.interval_seconds);
    rule.history.push(run);
    if rule.history.len() > 100 {
        rule.history.remove(0);
    }
}

#[derive(clap::Subcommand)]
pub enum TaskCommand {
    /// List task status and bounded history
    List,
    /// List available destination capabilities
    Destinations,
    /// Review an opt-in task request JSON file
    Plan {
        file: PathBuf,
    },
    /// Save a reviewed task; no remote writes until its first run
    Create {
        file: PathBuf,
        #[arg(long)]
        revision: String,
    },
    /// Run an enabled task once, without the dashboard
    Run {
        id: String,
    },
    /// Run all enabled due tasks once, without the dashboard
    Tick,
    /// Enable or retry a task blocked before dispatch
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    /// Remove scheduling, preserving app links and onboarding receipts
    Remove {
        id: String,
        #[arg(long)]
        confirmation: String,
    },
}
pub async fn run(store: &Store, command: TaskCommand) -> Result<()> {
    let value = match command {
        TaskCommand::List => store.tasks()?,
        TaskCommand::Destinations => store.task_destinations()?,
        TaskCommand::Plan { file } => {
            Box::pin(store.task_plan(serde_json::from_slice(&std::fs::read(file)?)?)).await?
        }
        TaskCommand::Create { file, revision } => {
            Box::pin(store.task_create(serde_json::from_slice(&std::fs::read(file)?)?, &revision))
                .await?
        }
        TaskCommand::Run { id } => Box::pin(store.task_run(&id)).await?,
        TaskCommand::Tick => Box::pin(store.tasks_tick()).await?,
        TaskCommand::Enable { id } => store.task_set_enabled(&id, true)?,
        TaskCommand::Disable { id } => store.task_set_enabled(&id, false)?,
        TaskCommand::Remove { id, confirmation } => {
            store.task_remove(&id, &confirmation)?;
            json!({"removed":true})
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, TaskRequest) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                server_id: "local".into(),
                name: "Dashboard fixture".into(),
                apps: vec!["homarr".into()],
            })
            .unwrap();
        let path = store.project_dir(&project.id).unwrap().join("onboarding");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("homarr.json"),serde_json::to_vec(&json!({"started":true,"completed":true,"pending":null,"api_key":"fixture-secret","board_url":"http://localhost/boards/fixture","apps_added":0,"items_added":0,"resources":{"board_id:0":"fixture-board"},"finished_steps":[],"linked_urls":[]})).unwrap()).unwrap();
        let request = TaskRequest {
            name: "Fixture links".into(),
            kind: TaskKind::DashboardLinks,
            destination: Destination {
                project_id: project.id,
                service: "homarr".into(),
            },
            managed: true,
            existing: false,
            project_ids: vec![],
            existing_ids: vec![],
            interval_seconds: 300,
            action: String::new(),
        };
        (dir, store, request)
    }
    fn rule(request: TaskRequest) -> TaskRule {
        TaskRule {
            id: "task-fixture".into(),
            request,
            enabled: true,
            status: "ready".into(),
            last_run: None,
            next_run: 0,
            history: vec![],
            pending: None,
            target_revision: "fixture-target".into(),
            container_id: "fixture-container".into(),
        }
    }
    #[test]
    fn scoped_reconciliation_discovers_new_sources_and_deduplicates_receipts() {
        let (_dir, store, mut request) = fixture();
        assert!(store.task_links(&request).unwrap().is_empty());
        let source = store
            .create(crate::core::CreateProject {
                server_id: "local".into(),
                name: "Source fixture".into(),
                apps: vec!["gotify".into()],
            })
            .unwrap();
        let links = store.task_links(&request).unwrap();
        assert_eq!(links.len(), 1);
        request.project_ids = vec![request.destination.project_id.clone()];
        assert!(store.task_links(&request).unwrap().is_empty());
        request.project_ids = vec![source.id];
        assert_eq!(store.task_links(&request).unwrap().len(), 1);
        let path = store
            .project_dir(&request.destination.project_id)
            .unwrap()
            .join("onboarding/homarr.json");
        let mut receipt: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        receipt["linked_urls"] = json!([format!("{}/", links[0].url)]);
        std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(store.task_links(&request).unwrap().is_empty());
    }
    #[test]
    fn target_pins_private_board_identity_but_not_link_progress() {
        let (_dir, store, request) = fixture();
        let original = store.task_validate(&request).unwrap();
        let path = store
            .project_dir(&request.destination.project_id)
            .unwrap()
            .join("onboarding/homarr.json");
        let mut receipt: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        receipt["linked_urls"] = json!(["https://example.test/"]);
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert_eq!(original, store.task_validate(&request).unwrap());
        receipt["resources"]["board_id:0"] = json!("replacement-board");
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert_ne!(original, store.task_validate(&request).unwrap());
        receipt["pending"] = json!("create-link:0");
        std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert!(store.task_validate(&request).is_err());
    }
    #[tokio::test]
    async fn link_dispatch_rechecks_reviewed_destination_under_project_lock() {
        let (_dir, store, request) = fixture();
        let reviewed = store.task_validate(&request).unwrap();
        let path = store
            .project_dir(&request.destination.project_id)
            .unwrap()
            .join("onboarding/homarr.json");
        let mut receipt: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        receipt["resources"]["board_id:0"] = json!("a-different-board");
        let changed = serde_json::to_vec(&receipt).unwrap();
        std::fs::write(&path, &changed).unwrap();
        let error = store
            .task_dashboard_links(
                &request,
                &reviewed,
                "previous-container",
                vec![AppLink {
                    name: "New source".into(),
                    url: "https://new.example.test".into(),
                    ..Default::default()
                }],
                "fresh-plan-must-not-authorize-a-new-board",
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Task destination changed before dispatch")
        );
        assert_eq!(std::fs::read(path).unwrap(), changed);
        assert!(store.tasks().unwrap().as_array().unwrap().is_empty());
    }
    #[tokio::test]
    async fn crash_journal_cannot_be_reenabled_or_retried_and_never_exports_credentials() {
        let (_dir, store, request) = fixture();
        let mut task = rule(request);
        task.status = "running".into();
        task.pending = Some(TaskRun {
            id: "operation".into(),
            started_at: 0,
            finished_at: None,
            status: "running".into(),
            count: 1,
            message: String::new(),
        });
        store.tasks_save(&Tasks { rules: vec![task] }).unwrap();
        assert!(store.task_set_enabled("task-fixture", true).is_err());
        assert!(store.task_run("task-fixture").await.is_err());
        assert!(
            store
                .tasks_tick()
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.tasks().unwrap()[0]["status"], "uncertain");
        assert_eq!(store.tasks().unwrap()[0]["enabled"], false);
        let text = store.tasks().unwrap().to_string();
        assert!(
            !text.contains("fixture-secret")
                && !text.contains("fixture-target")
                && !text.contains("fixture-container")
        );
        assert!(store.task_remove("task-fixture", "wrong").is_err());
        store.task_remove("task-fixture", "Fixture links").unwrap();
        assert!(
            store
                .onboarding_info(&store.read().unwrap().projects[0].id, "homarr")
                .unwrap()["state"]["completed"]
                == true
        );
    }
    #[tokio::test]
    async fn missing_destination_blocks_before_dispatch_and_requires_explicit_retry() {
        let (_dir, store, mut request) = fixture();
        request.destination.project_id = "p-0000000000".into();
        store
            .tasks_save(&Tasks {
                rules: vec![rule(request)],
            })
            .unwrap();
        let result = store.task_run("task-fixture").await.unwrap();
        assert_eq!(result["status"], "blocked");
        assert_eq!(result["enabled"], false);
        assert!(result["pending"].is_null());
        assert_eq!(result["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            store.task_set_enabled("task-fixture", true).unwrap()["enabled"],
            true
        );
    }
    #[test]
    fn writer_lock_and_scope_validation_fail_closed() {
        let (_dir, store, mut request) = fixture();
        let lock = store.tasks_lock().unwrap();
        assert!(store.tasks_lock().is_err());
        drop(lock);
        assert!(store.tasks_lock().is_ok());
        request.managed = false;
        assert!(store.task_validate(&request).is_err());
        request.managed = true;
        request.interval_seconds = 1;
        assert!(store.task_validate(&request).is_err());
        assert!(canonical_url("https://user:password@example.test").is_none());
        assert!(canonical_url("https://example.test/?token=secret").is_none());
        assert!(loopback_host("127.0.0.2"));
        assert!(loopback_host("[::1]"));
        assert!(loopback_host("0.0.0.0"));
    }
    #[test]
    fn action_tasks_pin_commands_and_reject_confirmation_required_actions() {
        let (_dir, store, mut request) = fixture();
        request.kind = TaskKind::ServiceAction;
        request.managed = false;
        request.action = "check-fixture".into();
        store
            .mutate(|data| {
                data.projects[0].services[0].definition.actions.push(
                    crate::catalog::ServiceAction {
                        id: "check-fixture".into(),
                        label: "Check fixture".into(),
                        description: "Read-only fixture validation".into(),
                        command: vec!["true".into()],
                        timeout_seconds: 10,
                        confirm: false,
                    },
                );
                Ok(())
            })
            .unwrap();
        let original = store.task_validate(&request).unwrap();
        store
            .mutate(|data| {
                data.projects[0].services[0]
                    .definition
                    .actions
                    .last_mut()
                    .unwrap()
                    .command = vec!["false".into()];
                Ok(())
            })
            .unwrap();
        assert_ne!(original, store.task_validate(&request).unwrap());
        store
            .mutate(|data| {
                data.projects[0].services[0]
                    .definition
                    .actions
                    .last_mut()
                    .unwrap()
                    .confirm = true;
                Ok(())
            })
            .unwrap();
        assert!(store.task_validate(&request).is_err());
        request.action = "update".into();
        assert!(store.task_validate(&request).is_err());
    }
}
