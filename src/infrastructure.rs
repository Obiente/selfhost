use crate::core::{Project, Store, now, run, token};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;

pub fn local_id() -> String {
    "local".into()
}
fn read_only_default() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Local,
    DockerSsh,
    DockerContext,
    ProxmoxSsh,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    /// SSH config alias. Credentials and host trust remain in OpenSSH.
    pub endpoint: String,
    #[serde(default)]
    pub group: String,
    #[serde(default = "read_only_default")]
    pub read_only: bool,
    /// Optional browser-reachable DNS name for apps bound to the server's LAN.
    #[serde(default)]
    pub app_host: String,
}
fn local_endpoint() -> Result<&'static str> {
    static ENDPOINT: std::sync::OnceLock<std::result::Result<String, String>> =
        std::sync::OnceLock::new();
    let value=ENDPOINT.get_or_init(|| {
        let output=std::process::Command::new("docker").args(["context","inspect","--format","{{.Endpoints.docker.Host}}"])
            .env_remove("DOCKER_HOST").output().map_err(|_|"Docker is unavailable".to_string())?;
        if !output.status.success(){return Err("Could not resolve the local Docker context".into())}
        let host=String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !(host.starts_with("unix://")||host.starts_with("npipe://")){return Err("The current Docker context is remote. Select a local context before starting selfhost; add remote contexts under Infrastructure.".into())}
        Ok(host)
    });
    value.as_deref().map_err(|e| anyhow::anyhow!("{e}"))
}
impl Server {
    pub fn local() -> Self {
        Self {
            id: local_id(),
            name: "This computer".into(),
            provider: Provider::Local,
            endpoint: String::new(),
            group: String::new(),
            read_only: false,
            app_host: "localhost".into(),
        }
    }
    pub fn require_docker(&self) -> Result<()> {
        ensure!(
            self.provider != Provider::ProxmoxSsh,
            "Choose a Docker server. Connect to a VM or LXC separately through its SSH alias."
        );
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.provider != Provider::Local && self.id != "local",
            "The local connection cannot be replaced"
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 64,
            "Choose a server name up to 64 characters"
        );
        ensure!(
            self.group.chars().count() <= 64,
            "Group names can contain up to 64 characters"
        );
        validate_alias(&self.endpoint)?;
        ensure!(
            self.app_host.is_empty() || valid_hostname(&self.app_host),
            "App hostname must be a DNS name or IPv4 address, without a scheme or port"
        );
        Ok(())
    }
    pub fn docker_command(&self, write: bool) -> Result<Command> {
        self.require_docker()?;
        ensure!(
            !write || !self.read_only,
            "This server is read-only. Actions are disabled."
        );
        let mut cmd = Command::new("docker");
        if self.provider == Provider::Local {
            cmd.env_remove("DOCKER_CONTEXT")
                .env_remove("DOCKER_HOST")
                .env_remove("DOCKER_TLS_VERIFY")
                .env_remove("DOCKER_CERT_PATH")
                .args(["--host", local_endpoint()?]);
        }
        if self.provider == Provider::DockerContext {
            validate_alias(&self.endpoint)?;
            cmd.env_remove("DOCKER_HOST")
                .env_remove("DOCKER_CONTEXT")
                .args(["--context", &self.endpoint]);
        }
        if self.provider == Provider::DockerSsh {
            validate_alias(&self.endpoint)?;
            // Explicit CLI selection wins over ambient Docker context settings.
            cmd.env_remove("DOCKER_CONTEXT")
                .env_remove("DOCKER_HOST")
                .env_remove("DOCKER_TLS_VERIFY")
                .env_remove("DOCKER_CERT_PATH")
                .args(["--host", &format!("ssh://{}", self.endpoint)]);
        }
        Ok(cmd)
    }
    pub(crate) fn ssh_command(&self, remote: &str) -> Result<Command> {
        validate_alias(&self.endpoint)?;
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=10",
            "--",
            &self.endpoint,
            remote,
        ]);
        Ok(cmd)
    }
}
fn valid_hostname(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
}
fn validate_alias(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 128
            && !s.starts_with('-')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Use a configured SSH host alias, without spaces, user@, or shell syntax"
    );
    Ok(())
}
fn json_lines(text: &str) -> Result<Vec<Value>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| Ok(serde_json::from_str(l)?))
        .collect()
}

impl Store {
    pub fn servers(&self) -> Result<Vec<Server>> {
        let mut servers = vec![Server::local()];
        servers.extend(self.read()?.servers);
        Ok(servers)
    }
    pub fn server(&self, id: &str) -> Result<Server> {
        ensure!(
            id != "local"
                || !matches!(
                    self.local_docker_mode()?,
                    crate::dashboard::LocalDockerMode::Disabled
                ),
            "Local Docker is disabled. Choose a remote host or enable local Docker under Infrastructure"
        );
        self.servers()?
            .into_iter()
            .find(|s| s.id == id)
            .context("Server connection not found")
    }
    pub fn add_server(&self, mut server: Server) -> Result<Server> {
        server.id.clear();
        server.validate()?;
        server.name = server.name.trim().into();
        server.id = format!("s-{}", token(6)?);
        self.mutate(|data| {
            ensure!(
                !data
                    .servers
                    .iter()
                    .any(|s| s.provider == server.provider && s.endpoint == server.endpoint),
                "This connection is already saved"
            );
            data.servers.push(server.clone());
            Ok(server)
        })
    }
    pub fn edit_server(&self, id: &str, server: Server) -> Result<Server> {
        server.validate()?;
        self.mutate(|data| {
            let current = data
                .servers
                .iter_mut()
                .find(|s| s.id == id)
                .context("Connection not found")?;
            ensure!(
                current.provider == server.provider && current.endpoint == server.endpoint,
                "Create a separate connection to change a destination"
            );
            current.name = server.name.trim().into();
            current.group = server.group;
            current.app_host = server.app_host;
            current.read_only = server.read_only;
            Ok(current.clone())
        })
    }
    pub(crate) fn project_docker(&self, project: &Project, write: bool) -> Result<Command> {
        self.server(&project.server_id)?.docker_command(write)
    }
    pub async fn infrastructure_inventory(&self, id: &str) -> Result<Value> {
        if id == "local" && !self.monitor_local_docker()? {
            return Ok(
                json!({"available":true,"provider":"local","resources":[],"optional":true,"message":"Local Docker is not in use. Connect a remote host or enable local monitoring."}),
            );
        }
        let server = self.server(id)?;
        if server.provider == Provider::ProxmoxSsh {
            let resources: Value = serde_json::from_str(
                &run(
                    server.ssh_command("pvesh get /cluster/resources --output-format json")?,
                    30,
                )
                .await?,
            )?;
            ensure!(resources.is_array(), "Invalid Proxmox inventory response");
            // Do not expose unrelated resource fields or raw command output in persistent state.
            let resources: Vec<Value> = resources
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    json!({
                        "id": r["id"], "type": r["type"], "name": r["name"], "node": r["node"],
                        "template":r["template"],"vmid": r["vmid"], "status": r["status"], "cpu": r["cpu"], "mem": r["mem"],
                        "maxmem": r["maxmem"], "disk": r["disk"], "maxdisk": r["maxdisk"]
                    })
                })
                .collect();
            return Ok(
                json!({"available":true,"provider":server.provider,"resources":resources,"sampled_at":now()}),
            );
        }
        let mut cmd = server.docker_command(false)?;
        cmd.args(["info", "--format", "{{json .}}"]);
        let info: Value = serde_json::from_str(run(cmd, 20).await?.trim())?;
        let mut cmd = server.docker_command(false)?;
        cmd.args(["ps", "-a", "--format", "{{json .}}"]);
        let resources = json_lines(&run(cmd, 20).await?)?;
        let mut nodes = vec![];
        let mut services = vec![];
        if info["Swarm"]["ControlAvailable"].as_bool() == Some(true) {
            let mut cmd = server.docker_command(false)?;
            cmd.args(["node", "ls", "--format", "{{json .}}"]);
            nodes = json_lines(&run(cmd, 20).await?)?;
            let mut cmd = server.docker_command(false)?;
            cmd.args(["service", "ls", "--format", "{{json .}}"]);
            services = json_lines(&run(cmd, 20).await?)?;
        }
        Ok(
            json!({"available":true,"provider":server.provider,"name":info["Name"],"engine_id":info["ID"],
            "version":info["ServerVersion"],"os":info["OSType"],"architecture":info["Architecture"],
            "swarm":info["Swarm"],"resources":resources,"nodes":nodes,"services":services,"sampled_at":now()}),
        )
    }
    /// This plan does not stop services, pull images, create volumes or change placement.
    pub async fn relocation_plan(&self, id: &str, destination: &str) -> Result<Value> {
        let project = self.project(id)?;
        let source = self.server(&project.server_id)?;
        let target = self.server(destination)?;
        target.require_docker()?;
        ensure!(source.id != target.id, "Choose a different server");
        let mut blockers: Vec<String> = vec![];
        if project.custom {
            blockers
                .push("Custom setups must be exported with their data and moved manually.".into());
        }
        if source.read_only || target.read_only {
            blockers.push("Both connections must allow changes before a move can run.".into());
        }
        let (from, to) = tokio::join!(
            self.infrastructure_inventory(&source.id),
            self.infrastructure_inventory(&target.id)
        );
        let from = match from {
            Ok(v) => Some(v),
            Err(_) => {
                blockers.push("Source connection could not be verified.".into());
                None
            }
        };
        let to = match to {
            Ok(v) => Some(v),
            Err(_) => {
                blockers.push("Destination connection could not be verified.".into());
                None
            }
        };
        if let (Some(a), Some(b)) = (&from, &to) {
            if a["engine_id"] == b["engine_id"] {
                blockers.push("These connections point to the same Docker engine.".into());
            }
            if a["architecture"] != b["architecture"] || a["os"] != b["os"] {
                blockers.push(
                    "Different platforms require image compatibility and data-format checks."
                        .into(),
                );
            }
            if a["swarm"]["LocalNodeState"] == "active" || b["swarm"]["LocalNodeState"] == "active"
            {
                blockers.push("Swarm placement needs an orchestrator migration plan; Compose moves are not cluster rescheduling.".into());
            }
        }
        let name = format!("selfhost-{}", project.id);
        let mut existing_data = false;
        for (server, side) in [(&source, "source"), (&target, "destination")] {
            let mut cmd = server.docker_command(false)?;
            cmd.args(["volume", "ls", "--format", "{{.Name}}"]);
            match run(cmd, 20).await {
                Ok(output) => {
                    if project.services.iter().any(|s| {
                        output
                            .lines()
                            .any(|v| v == format!("{name}_{}-data", s.app))
                    }) {
                        if side == "source" {
                            existing_data = true;
                        } else {
                            blockers.push("Destination already contains project volumes; they will not be overwritten.".into());
                        }
                    }
                }
                Err(_) => blockers.push(format!("Could not inspect {side} volumes.")),
            }
        }
        let has_containers = |inventory: &Option<Value>| {
            inventory
                .as_ref()
                .and_then(|v| v["resources"].as_array())
                .is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["Labels"]
                            .as_str()
                            .unwrap_or("")
                            .split(',')
                            .any(|l| l == format!("com.docker.compose.project={name}"))
                    })
                })
        };
        if has_containers(&to) {
            blockers.push("Destination already contains containers for this project.".into());
        }
        let deployed = existing_data || has_containers(&from);
        if self
            .read()?
            .projects
            .iter()
            .filter(|p| p.id != id && p.server_id == destination)
            .any(|p| {
                p.services
                    .iter()
                    .any(|s| project.services.iter().any(|ours| ours.port == s.port))
            })
        {
            blockers.push("A configured project uses one of these destination ports.".into());
        }
        Ok(
            json!({"project_id":id,"source":source.name,"destination":target.name,"destination_id":destination,
            "can_assign":blockers.is_empty() && !deployed,"can_migrate":blockers.is_empty() && deployed,"blockers":blockers,"has_data":existing_data,
            "volumes":project.services.iter().map(|s| json!({"name":format!("{name}_{}-data",s.app),"path":s.definition.data_path})).collect::<Vec<_>>(),
            "ports":project.services.iter().map(|s|s.port).collect::<Vec<_>>(),
            "steps":["Check platform and configured port conflicts; review available disk space","Save configuration and transfer exact source images","Stop source services and archive declared volumes","Copy and verify every declared volume","Start destination and verify service health","Switch project placement; update DNS or proxy routes separately"],
            "note":"A data move stops the source during transfer. Destination volumes must be empty. Source containers and data are retained for recovery."}),
        )
    }
    pub async fn assign_project(&self, id: &str, destination: &str) -> Result<Project> {
        let _lock = self.project_lock(id)?;
        let plan = self.relocation_plan(id, destination).await?;
        ensure!(
            plan["can_assign"] == true,
            "Relocation has unresolved blockers; review the plan"
        );
        self.snapshot_inner(id)?;
        self.mutate(|data| {
            let project = data
                .projects
                .iter_mut()
                .find(|p| p.id == id)
                .context("Project not found")?;
            project.server_id = destination.into();
            Ok(project.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ssh_option_and_shell_injection() {
        for alias in [
            "-oProxyCommand=bad",
            "user@host",
            "host;id",
            "host name",
            "$(id)",
            "host\n",
        ] {
            assert!(validate_alias(alias).is_err(), "{alias}");
        }
        assert!(validate_alias("production-vm.example.test").is_ok());
    }
    #[test]
    fn read_only_is_enforced_at_command_boundary() {
        let server = Server {
            provider: Provider::DockerSsh,
            endpoint: "test-host".into(),
            read_only: true,
            ..Server::local()
        };
        assert!(server.docker_command(true).is_err());
        let command = server.docker_command(false).unwrap();
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["--host", "ssh://test-host"]);
    }
    #[test]
    fn legacy_state_still_loads_local_projects() {
        let data:crate::core::Data = serde_json::from_value(json!({"schema":1,"projects":[{"id":"p-test","name":"test","services":[],"access":"local","created_at":0}],"activities":[],"snapshots":[],"schedules":[]})).unwrap();
        assert_eq!(data.projects[0].server_id, "local");
        assert!(data.servers.is_empty());
    }
}
