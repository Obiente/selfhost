//! Reviewed removal of Selfhost-owned projects. Volumes, bind mounts, networks,
//! database sources, and externally linked applications are never deleted here.
use crate::core::{Activity, Project, Schedule, Store, atomic_write, now, private_dir, run, token};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, fs::OpenOptions, path::PathBuf};

#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemovalMode {
    #[default]
    Archive,
    RemoveContainers,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupChoice {
    #[default]
    Configuration,
    ConfigurationAndDatabase,
    None,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalRequest {
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub mode: RemovalMode,
    #[serde(default)]
    pub backup: BackupChoice,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovalApply {
    pub plan_id: String,
    pub revision: String,
    pub confirmation: String,
    pub acknowledge_preserved_data: bool,
    #[serde(default)]
    pub acknowledge_no_backup: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerTarget {
    pub id: String,
    pub service: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RemovalPlan {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub request: RemovalRequest,
    pub revision: String,
    pub confirmation: String,
    pub expires_at: u64,
    pub containers: Vec<ContainerTarget>,
    pub scope: Vec<String>,
    pub warnings: Vec<String>,
    pub database_backup_available: bool,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    plan: RemovalPlan,
    project: Project,
    setup: crate::setup::Setup,
    schedules: Vec<Schedule>,
    status: String,
    snapshot_id: Option<String>,
    database_backup_id: Option<String>,
    created_at: u64,
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn owned_targets(
    values: &[Value],
    project: &Project,
    service: Option<&str>,
) -> Result<Vec<ContainerTarget>> {
    let allowed: BTreeSet<_> = project.services.iter().map(|s| s.app.as_str()).collect();
    let expected = format!("selfhost-{}", project.id);
    let mut result = Vec::new();
    for value in values {
        let labels = &value["Config"]["Labels"];
        let app = labels["com.docker.compose.service"]
            .as_str()
            .context("Container is missing its service ownership label")?;
        ensure!(
            labels["com.docker.compose.project"] == expected && allowed.contains(app),
            "Container ownership does not match this project"
        );
        ensure!(
            labels["com.docker.compose.oneoff"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("false")),
            "One-off containers cannot be removed with an app"
        );
        ensure!(
            service.is_none_or(|s| s == app),
            "Container belongs to a different service"
        );
        let id = value["Id"].as_str().context("Container is missing an ID")?;
        ensure!(
            id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid container ID"
        );
        // Container removal would destroy its writable layer. Require the user to
        // review each exact container, without treating mount preservation as a backup.
        result.push(ContainerTarget {
            id: id.into(),
            service: app.into(),
            name: value["Name"]
                .as_str()
                .unwrap_or(app)
                .trim_start_matches('/')
                .into(),
        });
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
fn remaining_setup(mut setup: crate::setup::Setup, service: &str) -> Result<crate::setup::Setup> {
    let services = setup.compose["services"]
        .as_object()
        .context("Missing Compose services")?;
    ensure!(
        services.len() > 1,
        "Remove the project when removing its last app"
    );
    ensure!(
        services.contains_key(service),
        "App not found in this project"
    );
    for (name, config) in services.iter().filter(|(name, _)| *name != service) {
        let dependency = &config["depends_on"];
        let uses_dependency = dependency
            .as_object()
            .is_some_and(|d| d.contains_key(service))
            || dependency
                .as_array()
                .is_some_and(|d| d.iter().any(|v| v == service));
        let uses_namespace = ["network_mode", "pid", "ipc"]
            .iter()
            .any(|key| config[*key] == format!("service:{service}"));
        let uses_link = ["links", "volumes_from"].iter().any(|key| {
            config[*key].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item.as_str()
                        .is_some_and(|s| s.split(':').next() == Some(service))
                })
            })
        });
        ensure!(
            !uses_dependency && !uses_namespace && !uses_link,
            "App {name} depends on {service}. Update that connection before removing the app"
        );
    }
    ensure!(
        setup.database.is_none(),
        "This project has a managed database connection. Remove the whole project or change its database connection first"
    );
    setup.compose["services"]
        .as_object_mut()
        .unwrap()
        .remove(service);
    setup.validate()?;
    Ok(setup)
}
impl Store {
    fn removal_dir(&self) -> Result<PathBuf> {
        let dir = self.root.join("removals");
        private_dir(&dir)?;
        Ok(dir)
    }
    fn removal_path(&self, id: &str, suffix: &str) -> Result<PathBuf> {
        ensure!(
            id.starts_with("r-") && crate::setup::slug(id),
            "Invalid removal ID"
        );
        Ok(self.removal_dir()?.join(format!("{id}.{suffix}.json")))
    }
    fn removal_revision(&self, project: &Project) -> Result<String> {
        let schedules: Vec<_> = self
            .read()?
            .schedules
            .into_iter()
            .filter(|s| s.project_id == project.id)
            .collect();
        digest(
            &json!({"project":project,"setup":self.setup(&project.id)?,"server":self.server(&project.server_id)?,"schedules":schedules,"sources":self.database_sources()?}),
        )
    }
    async fn removal_containers(
        &self,
        project: &Project,
        service: Option<&str>,
    ) -> Result<Vec<ContainerTarget>> {
        let mut command = self.project_docker(project, false)?;
        command.args([
            "ps",
            "-a",
            "--no-trunc",
            "--filter",
            &format!("label=com.docker.compose.project=selfhost-{}", project.id),
            "--format",
            "{{.ID}}",
        ]);
        if let Some(service) = service {
            command.args([
                "--filter",
                &format!("label=com.docker.compose.service={service}"),
            ]);
        }
        let ids = run(command, 15).await?;
        let ids: Vec<_> = ids.lines().filter(|s| !s.is_empty()).collect();
        ensure!(ids.len() <= 128, "Too many containers for one removal");
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        ensure!(
            ids.iter()
                .all(|id| id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit())),
            "Invalid container identifiers"
        );
        let mut command = self.project_docker(project, false)?;
        command.arg("inspect").args(ids);
        let values: Vec<Value> = serde_json::from_str(&run(command, 15).await?)?;
        owned_targets(&values, project, service)
    }
    pub async fn plan_removal(&self, id: &str, request: RemovalRequest) -> Result<RemovalPlan> {
        let _lock = self.project_lock(id)?;
        ensure!(
            !self
                .removal_archives()?
                .iter()
                .any(|a| a["project_id"] == id
                    && matches!(
                        a["status"].as_str(),
                        Some("runtime_pending" | "state_pending")
                    )),
            "An interrupted removal needs recovery before another removal can be planned"
        );
        let project = self.project(id)?;
        let setup = self.setup(id)?;
        if let Some(service) = &request.service {
            remaining_setup(setup.clone(), service)?;
        }
        let mut warnings = vec!["Persistent volumes, mounted files, networks, images and external databases are kept. A configuration snapshot does not contain application data.".into()];
        let hosts_shared_database = self
            .database_sources()?
            .iter()
            .any(|source| source["managed_project"] == id);
        if hosts_shared_database {
            warnings.push("This project hosts a shared database. Existing database clients keep their connection, but restoring this project is required to manage that database's containers through Selfhost.".into());
        }
        if setup.compose.pointer("/x-selfhost/external_lifecycle") == Some(&Value::Bool(true)) {
            ensure!(
                request.mode == RemovalMode::Archive,
                "This app manages containers outside its Selfhost project. Use its own administration controls for runtime removal, or archive its Selfhost record"
            );
            warnings.push("This app manages additional containers itself. Archiving its Selfhost record leaves those containers and the app's administration controls running.".into());
        }
        let scope = if request.mode == RemovalMode::RemoveContainers {
            ensure!(
                !self.server(&project.server_id)?.read_only,
                "This server is read-only. Archive the local record instead"
            );
            ensure!(
                !hosts_shared_database,
                "This project hosts a shared database source. Keep its runtime and archive only after reviewing its consumers"
            );
            warnings.push("Selected containers will stop. Data stored only in their writable container layers will be lost. Back up application files using the app's backup tools before proceeding.".into());
            vec![
                "Stop and remove only the listed Selfhost-owned containers".into(),
                "Remove the selected app or project from active management".into(),
                "Keep its private configuration archive and disable its schedules".into(),
            ]
        } else {
            warnings.push("Running containers keep running after archive. Selfhost will no longer schedule or manage the archived selection.".into());
            vec![
                "Remove the selected app or project from active management".into(),
                "Keep running containers, stored data and a private configuration archive".into(),
                "Disable schedules for the archived selection".into(),
            ]
        };
        let database_backup_available = setup.database.as_ref().is_some_and(|db| db.provisioned);
        ensure!(
            request.backup != BackupChoice::ConfigurationAndDatabase || database_backup_available,
            "A provisioned supported database connection is required for a database backup"
        );
        let containers = if request.mode == RemovalMode::RemoveContainers {
            self.removal_containers(&project, request.service.as_deref())
                .await?
        } else {
            vec![]
        };
        let plan = RemovalPlan {
            id: format!("r-{}", token(10)?),
            project_id: id.into(),
            project_name: project.name.clone(),
            confirmation: request
                .service
                .clone()
                .unwrap_or_else(|| project.name.clone()),
            request,
            revision: self.removal_revision(&project)?,
            expires_at: now() + 600,
            containers,
            scope,
            warnings,
            database_backup_available,
        };
        atomic_write(
            &self.removal_path(&plan.id, "plan")?,
            &serde_json::to_vec_pretty(&plan)?,
        )?;
        Ok(plan)
    }
    pub async fn apply_removal(&self, id: &str, input: RemovalApply) -> Result<Value> {
        let _lock = self.project_lock(id)?;
        let plan: RemovalPlan =
            serde_json::from_slice(&fs::read(self.removal_path(&input.plan_id, "plan")?)?)?;
        ensure!(
            plan.project_id == id && plan.id == input.plan_id,
            "Removal plan belongs to another project"
        );
        ensure!(
            plan.expires_at > now(),
            "Removal plan expired. Review a new plan"
        );
        ensure!(
            input.confirmation == plan.confirmation,
            "Type the exact app or project name to confirm"
        );
        ensure!(
            input.acknowledge_preserved_data,
            "Confirm that persistent data is retained and container-layer data is not backed up"
        );
        ensure!(
            plan.request.backup != BackupChoice::None || input.acknowledge_no_backup,
            "Explicitly confirm proceeding without a new backup"
        );
        let project = self.project(id)?;
        ensure!(
            input.revision == plan.revision && self.removal_revision(&project)? == plan.revision,
            "Project or server settings changed. Review a new removal plan"
        );
        let journal_path = self.removal_path(&plan.id, "archive")?;
        ensure!(
            !journal_path.exists(),
            "This removal has already started. Review its archive before taking further action"
        );
        // Prepare and validate the desired configuration before any runtime mutation.
        let next_setup = plan
            .request
            .service
            .as_deref()
            .map(|s| remaining_setup(self.setup(id)?, s))
            .transpose()?;
        if plan.request.mode == RemovalMode::RemoveContainers {
            ensure!(
                !self.server(&project.server_id)?.read_only,
                "This server is read-only"
            );
            ensure!(
                self.removal_containers(&project, plan.request.service.as_deref())
                    .await?
                    == plan.containers,
                "Container inventory changed. Review a new removal plan"
            );
        }
        let snapshot = if plan.request.backup == BackupChoice::None {
            None
        } else {
            Some(self.snapshot_inner(id)?)
        };
        let database_backup = if plan.request.backup == BackupChoice::ConfigurationAndDatabase {
            Some(self.backup_database_locked(id).await?)
        } else {
            None
        };
        ensure!(
            self.removal_revision(&project)? == plan.revision,
            "Project, schedules or server settings changed during backup. Review a new removal plan"
        );
        let mut journal = Journal {
            plan: plan.clone(),
            project: project.clone(),
            setup: self.setup(id)?,
            schedules: self
                .read()?
                .schedules
                .into_iter()
                .filter(|s| s.project_id == id)
                .collect(),
            status: "prepared".into(),
            snapshot_id: snapshot.map(|s| s.id),
            database_backup_id: database_backup.map(|b| b.id),
            created_at: now(),
        };
        atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
        let pending_path = self.project_dir(id)?.join("removal.pending.json");
        if plan.request.mode == RemovalMode::RemoveContainers && !plan.containers.is_empty() {
            // Recheck after backups, which may take time. Never resolve names again
            // after review: only these immutable full container IDs may be removed.
            ensure!(
                self.removal_containers(&project, plan.request.service.as_deref())
                    .await?
                    == plan.containers,
                "Container inventory changed during backup. Nothing was removed; review the prepared archive"
            );
            ensure!(
                self.removal_revision(&project)? == plan.revision,
                "Project changed during backup. Nothing was removed"
            );
            journal.status = "runtime_pending".into();
            atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
            atomic_write(
                &pending_path,
                &serde_json::to_vec(&json!({"archive_id":plan.id}))?,
            )?;
            let mut stop = self.project_docker(&project, true)?;
            stop.args(["stop", "--timeout", "60"])
                .args(plan.containers.iter().map(|c| c.id.as_str()));
            run(stop, 180).await.context("Stopping containers did not complete. Review the retained removal archive before retrying")?;
            let mut command = self.project_docker(&project, true)?;
            command
                .arg("rm")
                .args(plan.containers.iter().map(|c| c.id.as_str()));
            run(command,120).await.context("Container removal did not complete. The private archive has been retained; inspect exact container IDs before retrying")?;
        }
        journal.status = "state_pending".into();
        atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
        atomic_write(
            &pending_path,
            &serde_json::to_vec(&json!({"archive_id":plan.id}))?,
        )?;
        self.mutate(|data| {
            if let Some(service) = &plan.request.service {
                let updated = data
                    .projects
                    .iter_mut()
                    .find(|p| p.id == id)
                    .context("Project no longer exists")?;
                ensure!(
                    digest(updated)? == digest(&project)?,
                    "Project changed during removal"
                );
                updated.services.retain(|s| s.app != *service);
                updated.custom = true;
                updated.access = "custom".into();
                atomic_write(
                    &self.project_dir(id)?.join("setup.json"),
                    &serde_json::to_vec_pretty(
                        next_setup.as_ref().context("Missing updated setup")?,
                    )?,
                )?;
                data.schedules.retain(|s| {
                    s.project_id != id || !s.action.starts_with(&format!("app:{service}:"))
                });
            } else {
                data.projects.retain(|p| p.id != id);
                data.schedules.retain(|s| s.project_id != id);
            }
            data.activities.push(Activity {
                id: token(8)?,
                project_id: id.into(),
                project_name: project.name.clone(),
                action: "removal".into(),
                status: "succeeded".into(),
                message: if plan.request.mode == RemovalMode::Archive {
                    "Selection archived; running containers and persistent data retained".into()
                } else {
                    "Reviewed containers removed; persistent data retained".into()
                },
                started_at: journal.created_at,
                finished_at: Some(now()),
                read: false,
            });
            Ok(())
        })?;
        journal.status = "completed".into();
        atomic_write(&journal_path, &serde_json::to_vec_pretty(&journal)?)?;
        fs::remove_file(&pending_path)?;
        Ok(
            json!({"archive_id":plan.id,"project_removed":plan.request.service.is_none(),"snapshot_id":journal.snapshot_id,"database_backup_id":journal.database_backup_id,"persistent_data_preserved":true,"container_layer_data_backed_up":false}),
        )
    }
    pub fn removal_archives(&self) -> Result<Vec<Value>> {
        let mut result = vec![];
        for entry in fs::read_dir(self.removal_dir()?)? {
            let path = entry?.path();
            if !path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with(".archive.json"))
            {
                continue;
            }
            let j: Journal = serde_json::from_slice(&fs::read(path)?)?;
            result.push(json!({"id":j.plan.id,"project_id":j.project.id,"project_name":j.project.name,"service":j.plan.request.service,"status":j.status,"created_at":j.created_at,"snapshot_id":j.snapshot_id,"database_backup_id":j.database_backup_id,"can_restore_project":j.status=="completed"&&j.plan.request.service.is_none(),"needs_reconciliation":matches!(j.status.as_str(),"runtime_pending"|"state_pending")||self.project_dir(&j.project.id)?.join("removal.pending.json").exists()}));
        }
        result.sort_by_key(|r| std::cmp::Reverse(r["created_at"].as_u64().unwrap_or(0)));
        Ok(result)
    }
    /// Recover desired state only. Runtime is inspected, never started, stopped,
    /// or deleted. This is the sole bypass for the interrupted-removal lock.
    pub async fn reconcile_removal(&self, archive: &str, confirmation: &str) -> Result<Value> {
        let path = self.removal_path(archive, "archive")?;
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path)?)?;
        ensure!(
            journal.plan.id == archive,
            "Removal archive identity does not match"
        );
        ensure!(
            confirmation == journal.project.name,
            "Type the exact project name to recover its configuration"
        );
        let dir = self.project_dir(&journal.project.id)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("operation.lock"))?;
        lock.try_lock_exclusive()
            .context("A project action is still running")?;
        let marker = dir.join("removal.pending.json");
        let pending: Value = serde_json::from_slice(
            &fs::read(&marker).context("This archive has no interrupted removal to reconcile")?,
        )?;
        ensure!(
            pending["archive_id"] == archive,
            "A different removal needs recovery first"
        );
        ensure!(
            matches!(
                journal.status.as_str(),
                "runtime_pending" | "state_pending" | "completed" | "reconciled"
            ),
            "This archive is not awaiting recovery"
        );
        ensure!(
            !dir.join("migration.json").exists(),
            "Complete migration recovery first"
        );
        let remaining = if journal.plan.request.mode == RemovalMode::RemoveContainers {
            self.removal_containers(&journal.project, journal.plan.request.service.as_deref())
                .await?
        } else {
            vec![]
        };
        journal.setup.validate()?;
        self.mutate(|data| {
            ensure!(
                !data
                    .projects
                    .iter()
                    .filter(
                        |p| p.id != journal.project.id && p.server_id == journal.project.server_id
                    )
                    .flat_map(|p| &p.services)
                    .any(|s| s.port != 0
                        && journal
                            .project
                            .services
                            .iter()
                            .any(|old| old.port == s.port)),
                "An archived port is now used by another project"
            );
            atomic_write(
                &dir.join("setup.json"),
                &serde_json::to_vec_pretty(&journal.setup)?,
            )?;
            data.projects.retain(|p| p.id != journal.project.id);
            data.projects.push(journal.project.clone());
            data.schedules
                .retain(|s| s.project_id != journal.project.id);
            for mut schedule in journal.schedules.clone() {
                schedule.enabled = false;
                data.schedules.push(schedule);
            }
            Ok(())
        })?;
        journal.status = "reconciled".into();
        atomic_write(&path, &serde_json::to_vec_pretty(&journal)?)?;
        fs::remove_file(marker)?;
        Ok(
            json!({"project":journal.project,"configuration_restored":true,"runtime_changed":false,"remaining_containers":remaining,"schedules_enabled":false}),
        )
    }
    pub fn restore_removed_project(&self, archive: &str, confirmation: &str) -> Result<Project> {
        let path = self.removal_path(archive, "archive")?;
        let mut j: Journal = serde_json::from_slice(&fs::read(&path)?)?;
        ensure!(
            j.status == "completed" && j.plan.request.service.is_none(),
            "Only a completed whole-project archive can be restored here"
        );
        ensure!(
            confirmation == j.project.name,
            "Type the exact project name to restore its configuration"
        );
        ensure!(
            self.project_dir(&j.project.id)?.is_dir(),
            "Archived project files are unavailable"
        );
        ensure!(
            !self
                .project_dir(&j.project.id)?
                .join("removal.pending.json")
                .exists(),
            "Reconcile the interrupted removal before restoring this project"
        );
        self.server(&j.project.server_id)?;
        self.mutate(|data|{
            ensure!(!data.projects.iter().any(|p|p.id==j.project.id),"This project is already active");
            ensure!(!data.projects.iter().filter(|p|p.server_id==j.project.server_id).flat_map(|p|&p.services).any(|s|s.port!=0&&j.project.services.iter().any(|old|old.port==s.port)),"An archived port is now used by another project");
            data.projects.push(j.project.clone());
            // Schedules stay disabled until individually reviewed and re-enabled.
            for mut schedule in j.schedules.clone(){schedule.enabled=false;data.schedules.push(schedule);}
            Ok(())
        })?;
        j.status = "restored".into();
        atomic_write(&path, &serde_json::to_vec_pretty(&j)?)?;
        Ok(j.project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CreateProject;
    fn project(store: &Store) -> Project {
        store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Example project".into(),
                apps: vec!["homarr".into(), "uptime-kuma".into()],
            })
            .unwrap()
    }
    fn input(plan: &RemovalPlan) -> RemovalApply {
        RemovalApply {
            plan_id: plan.id.clone(),
            revision: plan.revision.clone(),
            confirmation: plan.confirmation.clone(),
            acknowledge_preserved_data: true,
            acknowledge_no_backup: false,
        }
    }
    #[tokio::test]
    async fn archive_requires_exact_confirmation_and_restores_with_data_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let env = fs::read(store.project_dir(&p.id).unwrap().join(".env")).unwrap();
        let plan = store
            .plan_removal(&p.id, RemovalRequest::default())
            .await
            .unwrap();
        let mut wrong = input(&plan);
        wrong.confirmation = "wrong".into();
        assert!(store.apply_removal(&p.id, wrong).await.is_err());
        let result = store.apply_removal(&p.id, input(&plan)).await.unwrap();
        assert_eq!(result["project_removed"], true);
        assert!(store.project(&p.id).is_err());
        assert_eq!(
            fs::read(store.project_dir(&p.id).unwrap().join(".env")).unwrap(),
            env
        );
        assert!(store.restore_removed_project(&plan.id, "wrong").is_err());
        assert_eq!(
            store.restore_removed_project(&plan.id, &p.name).unwrap().id,
            p.id
        );
        assert!(store.apply_removal(&p.id, input(&plan)).await.is_err());
    }
    #[tokio::test]
    async fn stale_plans_and_failed_backups_leave_project_active() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let plan = store
            .plan_removal(&p.id, RemovalRequest::default())
            .await
            .unwrap();
        store
            .mutate(|d| {
                d.projects[0].name = "Changed".into();
                Ok(())
            })
            .unwrap();
        assert!(store.apply_removal(&p.id, input(&plan)).await.is_err());
        let plan = store
            .plan_removal(&p.id, RemovalRequest::default())
            .await
            .unwrap();
        fs::write(
            store.root.join("snapshots"),
            b"prevent snapshot directory creation",
        )
        .unwrap();
        assert!(store.apply_removal(&p.id, input(&plan)).await.is_err());
        assert!(store.project(&p.id).is_ok());
        assert!(
            !store
                .project_dir(&p.id)
                .unwrap()
                .join("removal.pending.json")
                .exists()
        );
    }
    #[test]
    fn dependencies_and_foreign_containers_are_rejected() {
        let setup = crate::setup::Setup {
            database: None,
            environment: Default::default(),
            files: Default::default(),
            compose: json!({"services":{"db":{"image":"postgres:17"},"web":{"image":"nginx:stable","depends_on":["db"]}}}),
        };
        assert!(remaining_setup(setup, "db").is_err());
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let foreign = json!({"Id":"a".repeat(64),"Config":{"Labels":{"com.docker.compose.project":"foreign","com.docker.compose.service":"homarr","com.docker.compose.oneoff":"False"}}});
        assert!(owned_targets(&[foreign], &p, None).is_err());
    }
    #[tokio::test]
    async fn service_archive_preserves_other_apps_and_configuration() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let plan = store
            .plan_removal(
                &p.id,
                RemovalRequest {
                    service: Some("homarr".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        store.apply_removal(&p.id, input(&plan)).await.unwrap();
        let updated = store.project(&p.id).unwrap();
        assert_eq!(updated.services.len(), 1);
        assert_eq!(updated.services[0].app, "uptime-kuma");
        assert!(
            store.setup(&p.id).unwrap().compose["volumes"]
                .get("homarr-data")
                .is_some()
        );
    }
    #[tokio::test]
    async fn skipping_backup_needs_extra_acknowledgement_and_creates_no_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let plan = store
            .plan_removal(
                &p.id,
                RemovalRequest {
                    service: Some("homarr".into()),
                    backup: BackupChoice::None,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(store.apply_removal(&p.id, input(&plan)).await.is_err());
        let mut request = input(&plan);
        request.acknowledge_no_backup = true;
        store.apply_removal(&p.id, request).await.unwrap();
        assert!(store.read().unwrap().snapshots.is_empty());
    }
    #[tokio::test]
    async fn interrupted_archive_blocks_actions_and_reconciles_without_runtime_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let plan = store
            .plan_removal(
                &p.id,
                RemovalRequest {
                    service: Some("homarr".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        store.apply_removal(&p.id, input(&plan)).await.unwrap();
        let path = store.removal_path(&plan.id, "archive").unwrap();
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        journal.status = "state_pending".into();
        atomic_write(&path, &serde_json::to_vec(&journal).unwrap()).unwrap();
        atomic_write(
            &store
                .project_dir(&p.id)
                .unwrap()
                .join("removal.pending.json"),
            &serde_json::to_vec(&json!({"archive_id":plan.id})).unwrap(),
        )
        .unwrap();
        assert!(store.project_lock(&p.id).is_err());
        assert!(store.reconcile_removal(&plan.id, "wrong").await.is_err());
        let outcome = store.reconcile_removal(&plan.id, &p.name).await.unwrap();
        assert_eq!(outcome["runtime_changed"], false);
        assert_eq!(store.project(&p.id).unwrap().services.len(), 2);
        assert!(store.project_lock(&p.id).is_ok());
    }
    #[tokio::test]
    async fn read_only_servers_allow_archive_but_never_runtime_removal() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let server = store
            .add_server(crate::infrastructure::Server {
                id: "remote".into(),
                name: "Example host".into(),
                provider: crate::infrastructure::Provider::DockerSsh,
                endpoint: "example-host".into(),
                group: String::new(),
                read_only: true,
                app_host: String::new(),
            })
            .unwrap();
        store
            .mutate(|d| {
                d.projects[0].server_id = server.id.clone();
                Ok(())
            })
            .unwrap();
        assert!(
            store
                .plan_removal(
                    &p.id,
                    RemovalRequest {
                        mode: RemovalMode::RemoveContainers,
                        ..Default::default()
                    }
                )
                .await
                .err()
                .unwrap()
                .to_string()
                .contains("read-only")
        );
        assert!(
            store
                .plan_removal(&p.id, RemovalRequest::default())
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    #[ignore = "Creates uniquely named disposable Docker containers and volumes; requires nginx:alpine locally"]
    async fn docker_removal_preserves_volume_data_and_unrelated_containers() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(tmp.path().into()).unwrap();
        let p = project(&store);
        let unique = token(8).unwrap();
        let name = format!("selfhost-removal-check-{unique}");
        let foreign_name = format!("selfhost-removal-foreign-{unique}");
        let volume = format!("selfhost-removal-volume-{unique}");
        let mut created_ids = Vec::<String>::new();
        let mut created_volumes = vec![volume.clone()];
        let outcome: Result<()> = async {
            let mut create_volume = store.project_docker(&p, true)?;
            create_volume.args([
                "volume",
                "create",
                "--label",
                "selfhost.test=removal",
                &volume,
            ]);
            run(create_volume, 15).await?;
            let mut create = store.project_docker(&p, true)?;
            create.args([
                "create",
                "--name",
                &name,
                "--label",
                &format!("com.docker.compose.project=selfhost-{}", p.id),
                "--label",
                "com.docker.compose.service=homarr",
                "--label",
                "com.docker.compose.oneoff=False",
                "--mount",
                &format!("type=volume,source={volume},target=/persistent"),
                "--volume",
                "/anonymous",
                "--entrypoint",
                "/bin/sh",
                "nginx:alpine",
                "-c",
                "printf kept > /persistent/probe; printf kept > /anonymous/probe; exec sleep 3600",
            ]);
            let id = run(create, 30).await?.trim().to_string();
            created_ids.push(id.clone());
            let mut inspect = store.project_docker(&p, false)?;
            inspect.args(["inspect", &id]);
            let values: Vec<Value> = serde_json::from_str(&run(inspect, 15).await?)?;
            let anonymous = values[0]["Mounts"]
                .as_array()
                .context("Missing mounts")?
                .iter()
                .find(|m| m["Destination"] == "/anonymous")
                .and_then(|m| m["Name"].as_str())
                .context("Missing anonymous volume")?
                .to_string();
            created_volumes.push(anonymous);
            let mut start = store.project_docker(&p, true)?;
            start.args(["start", &id]);
            run(start, 15).await?;
            let mut foreign = store.project_docker(&p, true)?;
            foreign.args([
                "create",
                "--name",
                &foreign_name,
                "--label",
                "com.docker.compose.project=unrelated-removal-test",
                "nginx:alpine",
            ]);
            let foreign_id = run(foreign, 30).await?.trim().to_string();
            created_ids.push(foreign_id.clone());
            // Ensure the writer finished before stopping its container.
            let mut probe = store.project_docker(&p, false)?;
            probe.args(["exec", &id, "cat", "/persistent/probe"]);
            ensure!(
                run(probe, 15).await?.trim() == "kept",
                "Probe was not written"
            );
            let plan = store
                .plan_removal(
                    &p.id,
                    RemovalRequest {
                        mode: RemovalMode::RemoveContainers,
                        ..Default::default()
                    },
                )
                .await?;
            ensure!(
                plan.containers.len() == 1 && plan.containers[0].id == id,
                "Review selected unexpected containers"
            );
            store.apply_removal(&p.id, input(&plan)).await?;
            let mut absent = store.project_docker(&p, false)?;
            absent.args(["inspect", &id]);
            ensure!(
                run(absent, 15).await.is_err(),
                "Reviewed container still exists"
            );
            let mut untouched = store.project_docker(&p, false)?;
            untouched.args(["inspect", &foreign_id]);
            run(untouched, 15).await?;
            for stored in &created_volumes {
                let mut verify = store.project_docker(&p, true)?;
                verify.args([
                    "run",
                    "--rm",
                    "--mount",
                    &format!("type=volume,source={stored},target=/read,readonly"),
                    "--entrypoint",
                    "cat",
                    "nginx:alpine",
                    "/read/probe",
                ]);
                ensure!(
                    run(verify, 30).await?.trim() == "kept",
                    "Volume data was not preserved"
                );
            }
            Ok(())
        }
        .await;
        // Cleanup is scoped to resources created by this test. Never prune.
        for id in created_ids {
            let mut command = store.project_docker(&p, true).unwrap();
            command.args(["rm", "--force", &id]);
            let _ = run(command, 15).await;
        }
        for volume in created_volumes {
            let mut command = store.project_docker(&p, true).unwrap();
            command.args(["volume", "rm", &volume]);
            let _ = run(command, 15).await;
        }
        outcome.unwrap();
    }
}
