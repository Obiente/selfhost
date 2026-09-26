use crate::{
    core::{Store, atomic_write, now, run, token},
    infrastructure::{Provider, Server},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

const MAX_ARCHIVE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub struct MoveJob {
    pub id: String,
    pub project_id: String,
    pub source: String,
    pub destination: String,
    pub stage: String,
    pub status: String,
    pub started_at: u64,
    pub message: String,
    pub running_services: Vec<String>,
    pub helpers: Vec<(String, String)>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestMove {
    pub source_node: String,
    pub target_node: String,
    pub kind: String,
    pub vmid: u32,
}
fn part(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Invalid resource identifier"
    );
    Ok(())
}

async fn docker(server: &Server, args: &[&str], write: bool) -> Result<String> {
    let mut c = server.docker_command(write)?;
    c.args(args);
    run(c, 600).await
}
async fn inspect(server: &Server, kind: &str, id: &str) -> Result<Value> {
    let text = docker(server, &[kind, "inspect", id], false).await?;
    let rows: Vec<Value> = serde_json::from_str(&text)?;
    rows.into_iter().next().context("Resource not found")
}

/// Binary archives never enter JSON, command output or logs. They remain in private local storage.
async fn archive_export(mut cmd: Command, path: &Path) -> Result<()> {
    let errors = tempfile::tempfile()?;
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::from(errors))
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd.spawn()?;
    let output = child.stdout.take().context("Archive stream unavailable")?;
    let mut output = output.take(MAX_ARCHIVE_BYTES + 1);
    let mut file = tokio::fs::File::create(path).await?;
    let result = tokio::time::timeout(Duration::from_secs(1800), async {
        let count = tokio::io::copy(&mut output, &mut file).await?;
        ensure!(
            count <= MAX_ARCHIVE_BYTES,
            "Archive exceeds the 20 GiB transfer limit"
        );
        file.sync_all().await?;
        ensure!(child.wait().await?.success(), "Archive export failed");
        Ok::<_, anyhow::Error>(())
    })
    .await;
    if !matches!(&result, Ok(Ok(()))) {
        let _ = child.kill().await;
    }
    result.context("Archive export timed out")?
}
async fn archive_import(mut cmd: Command, path: &Path) -> Result<()> {
    cmd.stdin(Stdio::from(fs::File::open(path)?));
    cmd.stdout(Stdio::from(tempfile::tempfile()?))
        .stderr(Stdio::from(tempfile::tempfile()?))
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd.spawn()?;
    let status = tokio::time::timeout(Duration::from_secs(1800), child.wait())
        .await
        .context("Archive import timed out")??;
    ensure!(status.success(), "Archive import failed");
    Ok(())
}
/// Compare payload, metadata and links, independent of tar entry order.
fn archive_manifest(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut archive = tar::Archive::new(fs::File::open(path)?);
    let mut manifest = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        ensure!(
            !path.is_absolute()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "Archive contains an unsafe path"
        );
        let name = path
            .components()
            .filter_map(|c| {
                if let std::path::Component::Normal(v) = c {
                    Some(v.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let h = entry.header();
        let kind = h.entry_type();
        ensure!(
            kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link(),
            "Special files require an application-specific migration"
        );
        let mut hash = Sha256::new();
        hash.update([kind.as_byte()]);
        hash.update(h.mode()?.to_le_bytes());
        hash.update(h.uid()?.to_le_bytes());
        hash.update(h.gid()?.to_le_bytes());
        if let Some(link) = entry.link_name()? {
            hash.update(link.to_string_lossy().as_bytes());
        }
        let mut buf = [0u8; 65536];
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        ensure!(
            manifest
                .insert(name, format!("{:x}", hash.finalize()))
                .is_none(),
            "Duplicate archive path"
        );
    }
    Ok(manifest)
}

fn assess_guest_config(
    config: &Value,
    storage: &[Value],
    network: &[Value],
) -> Result<(Vec<Value>, Vec<String>)> {
    let mut blockers = vec![];
    let mut disks = vec![];
    for (key, value) in config.as_object().context("Invalid guest configuration")? {
        let text = value.as_str().unwrap_or("");
        if key.starts_with("hostpci")
            || key.starts_with("usb")
            || key == "args"
            || key == "hookscript"
            || key.starts_with("lxc.mount")
            || key.starts_with("lxc.cgroup")
        {
            blockers.push(format!("{key} requires a manual compatibility review."));
        }
        if key.starts_with("net") {
            for field in text.split(',') {
                if let Some(bridge) = field.strip_prefix("bridge=")
                    && !network.iter().any(|n| n["iface"] == bridge)
                {
                    blockers.push(format!("Destination is missing bridge {bridge}."));
                }
            }
        }
        if key == "rootfs"
            || key.starts_with("mp")
            || key.starts_with("scsi") && key != "scsihw"
            || key.starts_with("virtio")
            || key.starts_with("sata")
            || key.starts_with("ide")
            || key == "efidisk0"
            || key == "tpmstate0"
        {
            if text.starts_with("none") {
                continue;
            }
            let volume = text.split(',').next().unwrap_or("");
            if let Some((pool, _)) = volume.split_once(':') {
                disks.push(json!({"device":key,"volume":volume,"storage":pool}));
                if !storage.iter().any(|s| {
                    s["storage"] == pool
                        && (s["active"] == 1 || s["active"] == true)
                        && (s["enabled"] == 1 || s["enabled"] == true)
                }) {
                    blockers.push(format!("Destination storage {pool} is not available. Storage remapping is required."));
                }
            } else {
                blockers.push(format!(
                    "{key} is a bind mount or local device; it cannot be copied automatically."
                ));
            }
        }
    }
    Ok((disks, blockers))
}

impl Store {
    fn job_path(&self, id: &str) -> Result<PathBuf> {
        part(id)?;
        Ok(self.root.join("migrations").join(format!("{id}.json")))
    }
    fn save_job(&self, job: &MoveJob) -> Result<()> {
        let path = self.job_path(&job.id)?;
        fs::create_dir_all(path.parent().unwrap())?;
        atomic_write(&path, &serde_json::to_vec_pretty(job)?)?;
        Ok(())
    }
    pub fn move_jobs(&self) -> Result<Vec<MoveJob>> {
        let dir = self.root.join("migrations");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut jobs = vec![];
        for f in fs::read_dir(dir)? {
            let p = f?.path();
            if p.extension().is_some_and(|e| e == "json") {
                jobs.push(serde_json::from_slice::<MoveJob>(&fs::read(p)?)?);
            }
        }
        jobs.sort_by_key(|j| std::cmp::Reverse(j.started_at));
        Ok(jobs)
    }
    pub async fn begin_move(&self, id: &str, destination: &str) -> Result<MoveJob> {
        let lock = self.project_lock(id)?;
        let plan = self.relocation_plan(id, destination).await?;
        ensure!(
            plan["can_migrate"] == true,
            "Move preflight: {}",
            plan["blockers"]
        );
        let project = self.project(id)?;
        let job = MoveJob {
            id: format!("m-{}", token(6)?),
            project_id: id.into(),
            source: project.server_id.clone(),
            destination: destination.into(),
            stage: "preflight".into(),
            status: "running".into(),
            started_at: now(),
            message: String::new(),
            running_services: vec![],
            helpers: vec![],
        };
        self.save_job(&job)?;
        atomic_write(
            &self.project_dir(id)?.join("migration.json"),
            &serde_json::to_vec(&job.id)?,
        )?;
        let store = self.clone();
        let background = job.clone();
        tokio::spawn(async move {
            let _lock = lock;
            let mut job = background;
            let outcome = store.move_inner(&mut job).await;
            if let Err(error) = outcome {
                eprintln!("Migration {} failed: {error:#}", job.id);
                job.status = "needs_recovery".into();
                job.message = format!(
                    "Move stopped during {}. Source data is retained. Review the destination before recovering the source.",
                    job.stage
                );
            } else {
                job.status = "succeeded".into();
                job.stage = "completed".into();
                job.message =
                    "Destination verified. Source containers and volumes retained, stopped.".into();
            }
            let _ = store.save_job(&job);
        });
        Ok(job)
    }
    async fn move_inner(&self, job: &mut MoveJob) -> Result<()> {
        let original = self.project(&job.project_id)?;
        let source = self.server(&job.source)?;
        let target = self.server(&job.destination)?;
        source.docker_command(true)?;
        target.docker_command(true)?;
        let mut destination = original.clone();
        destination.server_id = target.id.clone();
        let work = self.root.join("migrations").join(&job.id);
        fs::create_dir_all(&work)?;
        let mut image_ids = BTreeMap::new();
        let mut source_containers = BTreeMap::new();
        // Every declared volume and container must belong to this project. Reject plugins and bind mounts.
        for service in &original.services {
            let volume = format!("selfhost-{}_{}-data", original.id, service.app);
            let v = inspect(&source, "volume", &volume).await?;
            ensure!(
                v["Driver"] == "local" && v["Options"].as_object().is_none_or(|m| m.is_empty()),
                "Volume driver needs a dedicated migration adapter"
            );
            ensure!(
                v["Labels"]["com.docker.compose.project"] == format!("selfhost-{}", original.id),
                "Volume ownership does not match the project"
            );
            let ids = docker(
                &source,
                &[
                    "ps",
                    "-a",
                    "--filter",
                    &format!("label=com.docker.compose.project=selfhost-{}", original.id),
                    "--filter",
                    &format!("label=com.docker.compose.service={}", service.app),
                    "--format",
                    "{{.ID}}",
                ],
                false,
            )
            .await?;
            let ids: Vec<_> = ids.lines().collect();
            ensure!(
                ids.len() == 1,
                "Expected exactly one source container per service"
            );
            source_containers.insert(service.app.clone(), ids[0].to_string());
            let c = inspect(&source, "container", ids[0]).await?;
            ensure!(
                c["Mounts"].as_array().is_some_and(|m| m.len() == 1
                    && m.iter().all(|v| v["Type"] == "volume"
                        && v["Name"] == volume
                        && v["Destination"] == service.definition.data_path)),
                "Undeclared mounts require an explicit migration recipe"
            );
            if c["State"]["Running"] == true {
                job.running_services.push(service.app.clone());
            }
            image_ids.insert(
                service.app.clone(),
                c["Image"]
                    .as_str()
                    .context("Source image missing")?
                    .to_string(),
            );
        }
        self.save_job(job)?;
        self.snapshot_inner(&original.id)?;
        job.stage = "copy_images".into();
        self.save_job(job)?;
        for (app, id) in &image_ids {
            let path = work.join(format!("{app}-image.tar"));
            let mut cmd = source.docker_command(false)?;
            cmd.args(["image", "save", id]);
            archive_export(cmd, &path).await?;
            let mut cmd = target.docker_command(true)?;
            cmd.args(["image", "load"]);
            archive_import(cmd, &path).await?;
            let loaded = inspect(&target, "image", id).await?;
            ensure!(loaded["Id"] == *id, "Loaded image digest mismatch");
            fs::remove_file(path)?;
        }
        job.stage = "stop_source".into();
        self.save_job(job)?;
        self.write_compose(&original)?;
        self.docker(&original, &["stop", "--timeout", "60"]).await?;
        for id in source_containers.values() {
            let c = inspect(&source, "container", id).await?;
            ensure!(
                c["State"]["Running"] == false
                    && c["State"]["OOMKilled"] != true
                    && c["State"]["ExitCode"] != 137,
                "Source did not shut down cleanly; recover it and resolve the shutdown issue before moving"
            );
        }
        for service in &original.services {
            job.stage = format!("copy_volume_{}", service.app);
            self.save_job(job)?;
            let volume = format!("selfhost-{}_{}-data", original.id, service.app);
            let image = &image_ids[&service.app];
            let src = format!("{}-src-{}", job.id, service.app);
            let dst = format!("{}-dst-{}", job.id, service.app);
            // Record helper identities before creation, so recovery can clean up after interruption.
            job.helpers.push((source.id.clone(), src.clone()));
            job.helpers.push((target.id.clone(), dst.clone()));
            self.save_job(job)?;
            docker(
                &source,
                &[
                    "create",
                    "--name",
                    &src,
                    "--label",
                    &format!("selfhost.migration={}", job.id),
                    "--network",
                    "none",
                    "--mount",
                    &format!(
                        "type=volume,source={volume},target=/selfhost-data,readonly,volume-nocopy"
                    ),
                    "--entrypoint",
                    "/bin/true",
                    image,
                ],
                true,
            )
            .await?;
            let path = work.join(format!("{}-data.tar", service.app));
            let mut cmd = source.docker_command(false)?;
            cmd.args(["cp", "-a", &format!("{src}:/selfhost-data/."), "-"]);
            archive_export(cmd, &path).await?;
            let expected = archive_manifest(&path)?;
            // Never use an existing target volume, even if another process created it since preflight.
            let names = docker(&target, &["volume", "ls", "--format", "{{.Name}}"], false).await?;
            ensure!(
                !names.lines().any(|v| v == volume),
                "Destination volume appeared during migration"
            );
            docker(
                &target,
                &[
                    "volume",
                    "create",
                    "--label",
                    &format!("com.docker.compose.project=selfhost-{}", original.id),
                    "--label",
                    &format!("com.docker.compose.volume={}-data", service.app),
                    "--label",
                    &format!("selfhost.migration={}", job.id),
                    &volume,
                ],
                true,
            )
            .await?;
            ensure!(
                inspect(&target, "volume", &volume).await?["Labels"]["selfhost.migration"]
                    == job.id,
                "Destination volume was created by another operation"
            );
            docker(
                &target,
                &[
                    "create",
                    "--name",
                    &dst,
                    "--label",
                    &format!("selfhost.migration={}", job.id),
                    "--network",
                    "none",
                    "--mount",
                    &format!("type=volume,source={volume},target=/selfhost-data,volume-nocopy"),
                    "--entrypoint",
                    "/bin/true",
                    image,
                ],
                true,
            )
            .await?;
            let mut cmd = target.docker_command(true)?;
            cmd.args(["cp", "-a", "-", &format!("{dst}:/selfhost-data")]);
            archive_import(cmd, &path).await?;
            let verify = work.join(format!("{}-verify.tar", service.app));
            let mut cmd = target.docker_command(false)?;
            cmd.args(["cp", "-a", &format!("{dst}:/selfhost-data/."), "-"]);
            archive_export(cmd, &verify).await?;
            ensure!(
                expected == archive_manifest(&verify)?,
                "Destination volume verification failed"
            );
            for (server, name) in [(&source, &src), (&target, &dst)] {
                docker(server, &["rm", "-v", name], true).await?;
            }
            fs::remove_file(path)?;
            fs::remove_file(verify)?;
        }
        job.stage = "start_destination".into();
        self.save_job(job)?;
        for service in &mut destination.services {
            service.image = image_ids[&service.app].clone();
        }
        self.write_compose(&destination)?;
        self.docker(&destination, &["create", "--pull", "never"])
            .await?;
        if !job.running_services.is_empty() {
            let mut args = vec!["start", "--wait", "--wait-timeout", "120"];
            args.extend(job.running_services.iter().map(String::as_str));
            self.docker(&destination, &args).await?;
        }
        for service in &destination.services {
            let output = docker(
                &target,
                &[
                    "ps",
                    "-a",
                    "--filter",
                    &format!("label=com.docker.compose.project=selfhost-{}", original.id),
                    "--filter",
                    &format!("label=com.docker.compose.service={}", service.app),
                    "--format",
                    "{{.ID}}",
                ],
                false,
            )
            .await?;
            ensure!(
                output.lines().count() == 1,
                "Destination service was not created"
            );
            let state = inspect(&target, "container", output.trim()).await?;
            ensure!(
                state["State"]["Running"].as_bool()
                    == Some(job.running_services.contains(&service.app)),
                "Destination service state differs from the source"
            );
        }
        // Keep desired tags for future updates, while the migration starts the exact source images.
        let mut desired = original.clone();
        desired.server_id = target.id.clone();
        self.write_compose(&desired)?;
        self.mutate(|data| {
            let p = data
                .projects
                .iter_mut()
                .find(|p| p.id == original.id)
                .context("Project missing")?;
            p.server_id = target.id.clone();
            Ok(())
        })?;
        fs::remove_file(self.project_dir(&original.id)?.join("migration.json"))?;
        Ok(())
    }
    pub async fn recover_move(&self, id: &str) -> Result<MoveJob> {
        let mut job: MoveJob = serde_json::from_slice(&fs::read(self.job_path(id)?)?)?;
        ensure!(
            job.status != "succeeded" && job.status != "recovered",
            "A completed move cannot be rolled back by this recovery action"
        );
        let project = self.project(&job.project_id)?;
        ensure!(
            project.server_id == job.source,
            "Placement already changed; manual recovery is required"
        );
        // Bypass the persisted migration guard, but take the same live operation lock.
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.project_dir(&project.id)?.join("operation.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).context("Migration is still running")?;
        let mut target = project.clone();
        target.server_id = job.destination.clone();
        self.write_compose(&project)?;
        self.docker(&target, &["stop", "--timeout", "60"])
            .await
            .context(
                "Could not verify that the destination is stopped; source has not been restarted",
            )?;
        for (server_id, name) in &job.helpers {
            let server = self.server(server_id)?;
            if let Ok(helper) = inspect(&server, "container", name).await {
                ensure!(
                    helper["Config"]["Labels"]["selfhost.migration"] == job.id,
                    "Helper ownership changed"
                );
                docker(&server, &["rm", "-v", name], true).await?;
            }
        }
        if !job.running_services.is_empty() {
            let mut args = vec!["start", "--wait", "--wait-timeout", "120"];
            args.extend(job.running_services.iter().map(String::as_str));
            self.docker(&project, &args).await?;
        }
        fs::remove_file(self.project_dir(&project.id)?.join("migration.json"))?;
        job.status = "recovered".into();
        job.message="Source restored. Destination data is retained for inspection and must be removed deliberately before retrying.".into();
        self.save_job(&job)?;
        Ok(job)
    }

    pub async fn guest_move_plan(&self, server_id: &str, input: &GuestMove) -> Result<Value> {
        part(&input.source_node)?;
        part(&input.target_node)?;
        ensure!(
            matches!(input.kind.as_str(), "qemu" | "lxc"),
            "Choose VM or LXC"
        );
        ensure!(input.vmid >= 100, "Invalid guest ID");
        let server = self.server(server_id)?;
        ensure!(
            server.provider == Provider::ProxmoxSsh,
            "Choose a Proxmox connection"
        );
        let inventory = self.infrastructure_inventory(server_id).await?;
        let rows = inventory["resources"]
            .as_array()
            .context("Missing cluster inventory")?;
        let guest = rows
            .iter()
            .find(|r| {
                r["vmid"] == input.vmid && r["type"] == input.kind && r["node"] == input.source_node
            })
            .context("Guest not found on the source node")?;
        let mut blockers = vec![];
        if server.read_only {
            blockers.push("Connection is read-only.".to_string());
        }
        if guest["status"] != "stopped" {
            blockers.push(
                "Shut down this guest before migration. Running-guest migration is not enabled."
                    .into(),
            );
        }
        if input.source_node == input.target_node {
            blockers.push("Choose a different node.".into());
        }
        if !rows.iter().any(|r| {
            r["type"] == "node" && r["node"] == input.target_node && r["status"] == "online"
        }) {
            blockers.push("Destination node is not online in this cluster.".into());
        }
        let source_online = rows.iter().any(|r| {
            r["type"] == "node" && r["node"] == input.source_node && r["status"] == "online"
        });
        let target_online = rows.iter().any(|r| {
            r["type"] == "node" && r["node"] == input.target_node && r["status"] == "online"
        });
        if !source_online {
            blockers.push("Source node is not online in this cluster.".into());
        }
        if !source_online || !target_online || input.source_node == input.target_node {
            return Ok(
                json!({"can_migrate":false,"blockers":blockers,"disks":[],"guest":guest,"destination":input.target_node,"note":"Both nodes must be reachable before storage and network checks can run."}),
            );
        }
        let command = format!(
            "pvesh get /nodes/{}/{}/{}/config --output-format json",
            input.source_node, input.kind, input.vmid
        );
        let config: Value = serde_json::from_str(&run(server.ssh_command(&command)?, 30).await?)?;
        if config.get("lock").is_some() {
            blockers.push("Guest is locked by another operation.".into());
        }
        let command = format!(
            "pvesh get /nodes/{}/storage --output-format json",
            input.target_node
        );
        let storage: Vec<Value> =
            serde_json::from_str(&run(server.ssh_command(&command)?, 30).await?)?;
        let command = format!(
            "pvesh get /nodes/{}/network --output-format json",
            input.target_node
        );
        let network: Vec<Value> =
            serde_json::from_str(&run(server.ssh_command(&command)?, 30).await?)?;
        let (disks, compatibility) = assess_guest_config(&config, &storage, &network)?;
        blockers.extend(compatibility);
        Ok(
            json!({"can_migrate":blockers.is_empty(),"blockers":blockers,"disks":disks,"guest":guest,"destination":input.target_node,"note":"Offline migration within this Proxmox cluster. Storage IDs and network bridges must match. Proxmox validates disk capacity and guest constraints before execution. Cross-cluster migration requires explicit storage and bridge mapping."}),
        )
    }
    pub async fn migrate_guest(&self, server_id: &str, input: &GuestMove) -> Result<Value> {
        let plan = self.guest_move_plan(server_id, input).await?;
        ensure!(
            plan["can_migrate"] == true,
            "Guest migration has unresolved blockers"
        );
        let server = self.server(server_id)?;
        ensure!(!server.read_only, "Read-only connection");
        let cmd = format!(
            "pvesh create /nodes/{}/{}/{}/migrate --target {} --output-format json",
            input.source_node, input.kind, input.vmid, input.target_node
        );
        let task: Value = serde_json::from_str(&run(server.ssh_command(&cmd)?, 60).await?)?;
        ensure!(
            task.as_str().is_some_and(|v| v.starts_with("UPID:")),
            "Proxmox did not return a task identifier; check its task history before retrying"
        );
        let receipt = json!({"id":format!("g-{}",token(6)?),"task":task,"node":input.source_node,"server_id":server_id,"destination":input.target_node,"vmid":input.vmid,"kind":input.kind,"status":"submitted","started_at":now()});
        let dir = self.root.join("guest-migrations");
        fs::create_dir_all(&dir)?;
        atomic_write(
            &dir.join(format!("{}.json", receipt["id"].as_str().unwrap())),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(receipt)
    }
    pub fn guest_moves(&self) -> Result<Vec<Value>> {
        let dir = self.root.join("guest-migrations");
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut rows = vec![];
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                rows.push(serde_json::from_slice::<Value>(&fs::read(path)?)?);
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r["started_at"].as_u64().unwrap_or(0)));
        Ok(rows)
    }
    pub async fn guest_task(&self, server_id: &str, node: &str, upid: &str) -> Result<Value> {
        part(node)?;
        ensure!(
            upid.starts_with("UPID:")
                && upid.len() < 512
                && upid
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b":@!._-".contains(&b)),
            "Invalid Proxmox task identifier"
        );
        let server = self.server(server_id)?;
        ensure!(
            server.provider == Provider::ProxmoxSsh,
            "Choose a Proxmox connection"
        );
        let cmd = format!("pvesh get /nodes/{node}/tasks/{upid}/status --output-format json");
        Ok(serde_json::from_str(
            &run(server.ssh_command(&cmd)?, 30).await?,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Requires an explicitly created disposable selfhost-migration-test Docker context"]
    async fn disposable_docker_move_and_recovery() -> Result<()> {
        let destination_context = std::env::var("SELFHOST_TEST_CONTEXT")
            .context("Set SELFHOST_TEST_CONTEXT to the disposable context")?;
        ensure!(
            destination_context == "selfhost-migration-test",
            "Only the disposable test context is permitted"
        );
        let endpoint = std::process::Command::new("docker")
            .args([
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ])
            .output()?;
        let endpoint = String::from_utf8(endpoint.stdout)?;
        ensure!(
            endpoint.starts_with("npipe:") || endpoint.starts_with("unix:"),
            "Source test engine must be local"
        );
        let temp = tempfile::tempdir()?;
        let mut store = Store::open(temp.path().into())?;
        let mut recipe = store.catalog[0].clone();
        recipe.id = "migration-fixture".into();
        recipe.name = "Migration fixture".into();
        recipe.image = "nginx:alpine".into();
        recipe.data_path = "/usr/share/nginx/html".into();
        recipe.port = 28761;
        recipe.container_port = 80;
        recipe.secrets.clear();
        recipe.environment.clear();
        recipe.actions.clear();
        recipe.dashboard = None;
        store.catalog = vec![recipe];
        let target = store.add_server(Server {
            id: String::new(),
            name: "Disposable destination".into(),
            provider: Provider::DockerContext,
            endpoint: destination_context,
            group: "Tests".into(),
            read_only: false,
            app_host: String::new(),
        })?;
        for (fail, initially_running) in [(false, true), (false, false), (true, true)] {
            let project = store.create(crate::core::CreateProject {
                server_id: "local".into(),
                name: "Synthetic migration test".into(),
                apps: vec!["migration-fixture".into()],
            })?;
            let source = Server::local();
            let result:Result<()> = async {
                store.action(&project.id,"start").await?;
                let container=docker(&source,&["ps","--filter",&format!("label=com.docker.compose.project=selfhost-{}",project.id),"--format","{{.ID}}"],false).await?;
                docker(&source,&["exec",container.trim(),"sh","-c","printf 'synthetic persistent content' > /usr/share/nginx/html/probe.txt; chmod 640 /usr/share/nginx/html/probe.txt"],true).await?;
                if fail {docker(&source,&["exec",container.trim(),"mkfifo","/usr/share/nginx/html/special-file"],true).await?;}
                if !initially_running {store.action(&project.id,"stop").await?;}
                let job=store.begin_move(&project.id,&target.id).await?;
                let final_job=tokio::time::timeout(Duration::from_secs(180),async {loop{let job=store.move_jobs()?.into_iter().find(|j|j.id==job.id).context("Missing job")?;if job.status!="running" {break Ok::<_,anyhow::Error>(job)}tokio::time::sleep(Duration::from_millis(500)).await;}}).await??;
                if fail {
                    ensure!(final_job.status=="needs_recovery","Special-file transfer must fail safely");
                    ensure!(store.action(&project.id,"start").await.is_err(),"Persisted migration guard must block ordinary actions");
                    store.recover_move(&job.id).await?;
                    ensure!(store.project(&project.id)?.server_id=="local","Recovery must retain source placement");
                    ensure!(inspect(&source,"container",container.trim()).await?["State"]["Running"]==true,"Source was not recovered");
                } else {
                    ensure!(final_job.status=="succeeded","Migration failed at {}: {}",final_job.stage,final_job.message);
                    ensure!(store.project(&project.id)?.server_id==target.id,"Placement not switched");
                    ensure!(inspect(&source,"container",container.trim()).await?["State"]["Running"]==false,"Source must remain stopped");
                    let dest=docker(&target,&["ps","-a","--filter",&format!("label=com.docker.compose.project=selfhost-{}",project.id),"--format","{{.ID}}"],false).await?;
                    ensure!(inspect(&target,"container",dest.trim()).await?["State"]["Running"]==initially_running,"Stopped service state was not preserved");
                    if !initially_running {docker(&target,&["start",dest.trim()],true).await?;}
                    let content=docker(&target,&["exec",dest.trim(),"cat","/usr/share/nginx/html/probe.txt"],true).await?;
                    ensure!(content=="synthetic persistent content","Persistent data changed");
                }
                Ok(())
            }.await;
            for server in [&source, &target] {
                let mut cleanup = project.clone();
                cleanup.server_id = server.id.clone();
                store.write_compose(&cleanup)?;
                let _ = store
                    .docker(&cleanup, &["down", "--volumes", "--remove-orphans"])
                    .await;
            }
            store.mutate(|data| {
                data.projects.retain(|p| p.id != project.id);
                Ok(())
            })?;
            result?;
        }
        Ok(())
    }
    #[test]
    fn proxmox_preflight_rejects_unmapped_storage_network_and_passthrough() {
        let config = json!({"rootfs":"pool:subvol-101-disk-0,size=8G","mp0":"/host/private,mp=/data","net0":"name=eth0,bridge=vmbr9","hostpci0":"0000:01:00"});
        let (_, blockers) = assess_guest_config(&config, &[], &[]).unwrap();
        assert!(blockers.iter().any(|b| b.contains("pool")));
        assert!(blockers.iter().any(|b| b.contains("vmbr9")));
        assert!(blockers.iter().any(|b| b.contains("mp0")));
        assert!(blockers.iter().any(|b| b.contains("hostpci0")));
        let config = json!({"scsi0":"pool:vm-101-disk-0,size=8G","net0":"virtio=00:11:22:33:44:55,bridge=vmbr0"});
        let (disks, blockers) = assess_guest_config(
            &config,
            &[json!({"storage":"pool","active":1,"enabled":1})],
            &[json!({"iface":"vmbr0"})],
        )
        .unwrap();
        assert!(blockers.is_empty());
        assert_eq!(disks.len(), 1);
    }
    #[test]
    fn manifests_detect_content_and_permission_changes() {
        let dir = tempfile::tempdir().unwrap();
        let make = |name: &str, body: &[u8], mode: u32| {
            let p = dir.path().join(name);
            let mut builder = tar::Builder::new(fs::File::create(&p).unwrap());
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(mode);
            h.set_uid(1000);
            h.set_gid(1000);
            h.set_cksum();
            builder.append_data(&mut h, "data/file", body).unwrap();
            builder.finish().unwrap();
            p
        };
        let a = make("a.tar", b"persistent data", 0o600);
        let b = make("b.tar", b"persistent data", 0o600);
        let c = make("c.tar", b"changed", 0o600);
        let d = make("d.tar", b"persistent data", 0o644);
        assert_eq!(archive_manifest(&a).unwrap(), archive_manifest(&b).unwrap());
        assert_ne!(archive_manifest(&a).unwrap(), archive_manifest(&c).unwrap());
        assert_ne!(archive_manifest(&a).unwrap(), archive_manifest(&d).unwrap());
    }
}
