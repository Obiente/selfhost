//! Directory-first helpers. Runtime files belong to the user; local receipts are optional.
use crate::{
    core::{CreateProject, Project, Store, atomic_write, private_dir},
    setup::Setup,
};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use fs2::FileExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum AppCommand {
    /// Write a new private Compose directory; no dashboard, registration or Docker required
    Init {
        app: String,
        /// Use an installable stack ID instead of a single-container recipe
        #[arg(long, conflicts_with = "method")]
        stack: bool,
        /// Contributor-defined deployment method, such as nextcloud's aio
        #[arg(long)]
        method: Option<String>,
        /// JSON map of stack input values
        #[arg(long)]
        inputs: Option<PathBuf>,
        /// Explicitly acknowledge a deployment requirement (repeat for each ID)
        #[arg(long)]
        ack: Vec<String>,
        /// Start after writing the files; otherwise review them first
        #[arg(long)]
        start: bool,
        /// Explicit image tag/digest from the recipe repository
        #[arg(long, conflicts_with = "stack")]
        image: Option<String>,
        /// Acknowledge that a custom image has not been validated with this recipe
        #[arg(long, requires = "image")]
        allow_untested: bool,
    },
    /// Start the directory's current Compose configuration
    Start,
    /// Stop containers and retain every volume
    Stop,
    /// Restart the directory's containers
    Restart,
    /// Choose versions, review, snapshot configuration and update after confirmation
    Update,
    /// Show Docker Compose status
    Status,
    /// List the recorded recipe's version choices
    Versions { service: String },
    /// Review an image change without touching running containers
    VersionPlan {
        service: String,
        image: String,
        #[arg(long)]
        allow_untested: bool,
    },
    /// Save a reviewed image change; run start separately when ready to recreate
    VersionApply {
        service: String,
        image: String,
        #[arg(long)]
        allow_untested: bool,
        #[arg(long)]
        revision: String,
    },
    /// Show recent logs for a service
    Logs { service: String },
    /// Show frozen recipe actions and integration workflows without running them
    Actions { service: String },
    /// Run a declared recipe action or app workflow
    Action {
        service: String,
        action: String,
        #[arg(long)]
        inputs: Option<PathBuf>,
    },
    /// Read supported native settings with secret fields omitted
    Config {
        service: String,
        #[arg(long)]
        edit: bool,
    },
    /// Preview a JSON map of typed native app settings
    Plan { service: String, file: PathBuf },
    /// Change native settings interactively, or apply an explicit reviewed file
    Apply {
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Set up the app interactively; --inspect only reports saved progress
    Setup {
        service: String,
        #[arg(long)]
        inspect: bool,
    },
    /// Preview first-run setup or an existing API-key connection
    SetupPlan { service: String, file: PathBuf },
    /// Apply reviewed app setup without a dashboard or daemon
    SetupApply {
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Find the authenticated human account before choosing an app administrator
    ConnectAccount {
        service: String,
        file: PathBuf,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        token_env: String,
    },
    /// Review an explicit JSON array of new dashboard links using its saved API key
    SyncPlan { service: String, file: PathBuf },
    /// Add application links interactively using the saved private board
    Sync {
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Preview an app's OIDC client registration from a JSON request
    ConnectPlan { service: String, file: PathBuf },
    /// Connect an identity provider interactively, or apply a reviewed request file
    Connect {
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        token_env: String,
    },
    /// Show the connection receipt, or resume configuring its existing client
    Connection {
        service: String,
        #[arg(long)]
        resume: bool,
    },
    /// List native configuration backups for a service
    Backups { service: String },
    /// Preview a native configuration restore; apply only with the returned revision
    Restore {
        service: String,
        backup: String,
        #[arg(long)]
        revision: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct Directory {
    root: PathBuf,
    project: String,
    opened_revision: String,
}

fn regular(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).context("A required standalone file is missing")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "Standalone files must be regular files, not links"
    );
    ensure!(
        metadata.len() <= 1024 * 1024,
        "Standalone file exceeds 1 MiB"
    );
    Ok(fs::read(path)?)
}
fn directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Standalone directories must not be links"
    );
    Ok(())
}
fn metadata_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Private helper metadata must not contain links"
        );
        ensure!(
            metadata.is_file() || metadata.is_dir(),
            "Unsupported private metadata file type"
        );
        visited += 1;
        ensure!(
            visited <= 20000,
            "Private helper metadata is too large to verify safely"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        }
    }
    Ok(())
}
impl Directory {
    /// Runtime coordinates follow the user's current Compose file; recipe capabilities remain frozen.
    pub(crate) fn refresh_projects(&self, data: &mut crate::core::Data) -> Result<()> {
        let setup = self.read(&self.project)?;
        let project = data
            .projects
            .iter_mut()
            .find(|p| p.id == self.project)
            .context("Directory project missing")?;
        for service in &mut project.services {
            let current = &setup.compose["services"][&service.app];
            service.image = current["image"]
                .as_str()
                .context("Recorded service needs an explicit current image")?
                .into();
            service.port =
                published_ipv4_port(current, service.definition.container_port)?.unwrap_or(0);
        }
        Ok(())
    }
    pub(crate) fn read(&self, id: &str) -> Result<Setup> {
        ensure!(id == self.project, "Directory belongs to another project");
        directory(&self.root)?;
        let compose = crate::setup::parse_compose(&String::from_utf8(regular(
            &self.root.join("compose.yaml"),
        )?)?)?;
        ensure!(
            compose["name"].as_str() == Some(format!("selfhost-{}", self.project).as_str()),
            "Compose project name changed. Restore its original name before using these helpers, or manage the renamed stack directly with Docker Compose"
        );
        // Docker parses .env itself. Preserve its bytes, comments, quoting and interpolation.
        regular(&self.root.join(".env"))?;
        let mut files = BTreeMap::new();
        let path = self.root.join("files");
        if path.exists() {
            directory(&path)?;
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Configuration filename is not UTF-8"))?;
                files.insert(name, String::from_utf8(regular(&entry.path())?)?);
            }
        }
        let setup = Setup {
            compose,
            database: None,
            environment: BTreeMap::new(),
            files,
        };
        setup.validate()?;
        Ok(setup)
    }
    pub(crate) fn revision(&self) -> Result<String> {
        let setup = self.read(&self.project)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                setup,
                regular(&self.root.join(".env"))?
            ))?)
        ))
    }
}
fn published_ipv4_port(service: &Value, target: u16) -> Result<Option<u16>> {
    let Some(ports) = service.get("ports") else {
        return Ok(None);
    };
    let ports = ports.as_array().context("Use a Compose ports list")?;
    let mut selected = None;
    for port in ports {
        let parsed = if let Some(text) = port.as_str() {
            let (mapping, protocol) = text.rsplit_once('/').unwrap_or((text, "tcp"));
            if protocol != "tcp" {
                continue;
            }
            let parts: Vec<_> = mapping.split(':').collect();
            match parts.as_slice() {
                [published, container] => Some((
                    "0.0.0.0".to_owned(),
                    (*published).to_owned(),
                    (*container).to_owned(),
                )),
                [host, published, container] => Some((
                    (*host).to_owned(),
                    (*published).to_owned(),
                    (*container).to_owned(),
                )),
                _ => None,
            }
        } else if port.is_object() {
            if port["protocol"].as_str().unwrap_or("tcp") != "tcp" {
                continue;
            }
            let number = |v: &Value| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            };
            number(&port["published"])
                .zip(number(&port["target"]))
                .map(|(p, t)| (port["host_ip"].as_str().unwrap_or("0.0.0.0").into(), p, t))
        } else {
            None
        };
        let Some((host, published, container)) = parsed else {
            continue;
        };
        if !["127.0.0.1", "0.0.0.0"].contains(&host.as_str())
            || container.parse::<u16>().ok() != Some(target)
        {
            continue;
        }
        let published:u16=published.parse().context("App helpers require a concrete published IPv4 port; resolve variables or ranges in Compose first")?;
        ensure!(
            published > 0,
            "App helpers require a nonzero published port"
        );
        ensure!(
            selected.is_none() || selected == Some(published),
            "Multiple published ports match this app; use one local IPv4 binding for app helpers"
        );
        selected = Some(published);
    }
    Ok(selected)
}
impl Store {
    pub(crate) fn runtime_dir(&self, id: &str) -> Result<PathBuf> {
        if let Some(directory) = &self.standalone {
            ensure!(
                directory.project == id,
                "Directory belongs to another project"
            );
            return Ok(directory.root.clone());
        }
        self.project_dir(id)
    }
    pub(crate) fn save_standalone_setup(&self, id: &str, setup: Setup) -> Result<Project> {
        let context = self
            .standalone
            .as_ref()
            .context("Not a standalone directory")?;
        let before = context.read(id)?;
        setup.validate()?;
        ensure!(
            setup.environment == before.environment && setup.database.is_none(),
            "Edit this directory's .env directly; managed database attachment is unavailable in directory mode"
        );
        ensure!(
            setup.compose["name"] == before.compose["name"],
            "Cannot change the Compose project name"
        );
        ensure!(
            before
                .files
                .keys()
                .all(|name| setup.files.contains_key(name)),
            "Directory helpers never remove user configuration files"
        );
        ensure!(
            context.revision()? == context.opened_revision,
            "Directory changed during this operation. Review a fresh preview"
        );
        self.snapshot_inner(id)?;
        // Save a recovery receipt before multi-file writes. A partial write never silently retries.
        let pending = self.root.join("runtime.pending.json");
        atomic_write(
            &pending,
            &serde_json::to_vec_pretty(
                &json!({"before":before,"after":setup,"environment":String::from_utf8(regular(&context.root.join(".env"))?)?}),
            )?,
        )?;
        for (name, value) in &setup.files {
            if before.files.get(name) != Some(value) {
                let path = context.root.join("files");
                if !path.exists() {
                    private_dir(&path)?;
                }
                directory(&path)?;
                let file = path.join(name);
                if file.exists() {
                    regular(&file)?;
                }
                atomic_write(&file, value.as_bytes())?;
            }
        }
        if setup.compose != before.compose {
            atomic_write(
                &context.root.join("compose.yaml"),
                serde_yaml::to_string(&setup.compose)?.as_bytes(),
            )?;
        }
        fs::remove_file(pending)?;
        self.project(id)
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&regular(path)?).context("Invalid JSON input file")
}
fn print(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
#[cfg(test)]
fn create_files(
    path: &Path,
    app: &str,
    stack: bool,
    method: Option<&str>,
    inputs: BTreeMap<String, Value>,
    ack: Vec<String>,
    catalog: Option<&Path>,
) -> Result<Vec<String>> {
    create_versioned_files(path, app, stack, method, inputs, ack, catalog, None)
}
#[allow(clippy::too_many_arguments)]
fn create_versioned_files(
    path: &Path,
    app: &str,
    stack: bool,
    method: Option<&str>,
    inputs: BTreeMap<String, Value>,
    ack: Vec<String>,
    catalog: Option<&Path>,
    selection: Option<crate::versions::Selection>,
) -> Result<Vec<String>> {
    ensure!(
        selection.is_none() || !stack,
        "Choose version inputs declared by the stack or deployment method instead"
    );
    let output = std::path::absolute(path)?;
    ensure!(
        !output.exists(),
        "Choose a new directory. Existing directories are never overwritten"
    );
    let parent = output.parent().context("Choose an application directory")?;
    ensure!(parent.is_dir(), "Create the parent directory first");
    let temp = tempfile::Builder::new()
        .prefix(".selfhost-init-")
        .tempdir_in(parent)?;
    private_dir(temp.path())?;
    let store = Store::with_catalog(temp.path().join(".selfhost"), catalog)?;
    let default_method = if !stack && method.is_none() {
        let methods = store.deployments()?;
        methods
            .as_array()
            .context("Invalid deployment catalogue")?
            .iter()
            .find(|m| m["app"] == app && m["default"] == true)
            .and_then(|m| m["id"].as_str())
            .map(str::to_owned)
    } else {
        None
    };
    let method = method.or(default_method.as_deref());
    let (project, instructions) = if stack {
        store.install_blueprint(crate::stacks::Install {
            blueprint: app.into(),
            name: app.into(),
            server_id: "local".into(),
            inputs,
            acknowledgements: ack,
        })?
    } else if let Some(method) = method {
        store.create_deployment(crate::deployments::CreateDeployment {
            version: selection,
            app: app.into(),
            method: method.into(),
            name: app.into(),
            server_id: "local".into(),
            inputs,
            acknowledgements: ack,
        })?
    } else {
        ensure!(
            inputs.is_empty() && ack.is_empty(),
            "Use --stack or --method for deployment inputs and acknowledgements"
        );
        (
            store.create_versioned(
                CreateProject {
                    name: app.into(),
                    apps: vec![app.into()],
                    server_id: "local".into(),
                },
                selection
                    .map(|s| BTreeMap::from([(app.into(), s)]))
                    .unwrap_or_default(),
            )?,
            vec![],
        )
    };
    let setup = store.setup(&project.id)?;
    setup.validate()?;
    atomic_write(
        &temp.path().join("compose.yaml"),
        serde_yaml::to_string(&setup.compose)?.as_bytes(),
    )?;
    atomic_write(&temp.path().join(".env"), setup.dotenv().as_bytes())?;
    if !setup.files.is_empty() {
        private_dir(&temp.path().join("files"))?;
    }
    for (name, value) in &setup.files {
        atomic_write(&temp.path().join("files").join(name), value.as_bytes())?;
    }
    // Pin the recipe and identity locally; nothing is registered in the default workspace.
    store.mutate(|data| {
        data.projects[0].custom = true;
        Ok(())
    })?;
    atomic_write(
        &store.project_dir(&project.id)?.join("setup.json"),
        &serde_json::to_vec_pretty(&setup)?,
    )?;
    atomic_write(
        &store.root.join("standalone.json"),
        &serde_json::to_vec_pretty(&json!({"schema":1,"project":project.id}))?,
    )?;
    let readme = format!(
        "This is an ordinary Docker Compose application.\n\nFrom this directory:\n  docker compose -f compose.yaml --env-file .env up -d\n  docker compose -f compose.yaml --env-file .env pull\n  docker compose -f compose.yaml --env-file .env up -d\n  docker compose -f compose.yaml --env-file .env logs\n\nCompose project: selfhost-{}\nKeep this name to preserve the association with your containers and volumes.\nDo not override it with COMPOSE_PROJECT_NAME or --project-name.\n\nOptional helpers:\n  selfhost app start\n  selfhost app update\n  selfhost app actions SERVICE\n\nEdit compose.yaml, .env and files/ directly. .selfhost contains private\nrecipe metadata, configuration backups and integration recovery receipts.\nDeleting .selfhost does not affect Docker, but disables optional Selfhost\nhelpers and deletes those receipts. No account, dashboard or background\nSelfhost process is needed. Keep this entire directory private.\nConfiguration snapshots exclude volume data. Back up application data\nwith its native backup tools before upgrades. Do not use down -v unless\nyou intend to delete its data.\n",
        project.id
    );
    atomic_write(&temp.path().join("README.txt"), readme.as_bytes())?;
    // Reserve the final destination atomically, then move only our staged children.
    fs::create_dir(&output).context("Destination was created concurrently; nothing overwritten")?;
    private_dir(&output)?;
    for item in fs::read_dir(temp.path())? {
        let item = item?;
        fs::rename(item.path(), output.join(item.file_name()))?;
    }
    Ok(instructions)
}
fn open(path: &Path, catalog: Option<&Path>) -> Result<(Store, Project, fs::File)> {
    let root = fs::canonicalize(path).context("Standalone directory not found")?;
    directory(&root)?;
    let metadata = root.join(".selfhost");
    directory(&metadata).context("Optional .selfhost metadata is missing. Use Docker Compose directly; do not initialize over this directory")?;
    metadata_tree(&metadata)?;
    let lock_path = metadata.join("cli.lock");
    if lock_path.exists() {
        regular(&lock_path)?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock_exclusive()
        .context("Another Selfhost helper is using this directory")?;
    ensure!(
        !metadata.join("runtime.pending.json").exists(),
        "An interrupted configuration write needs review. Its before/after files are saved privately in .selfhost/runtime.pending.json. Reconcile those files, then remove that receipt before retrying"
    );
    regular(&metadata.join("state.json"))?;
    let marker: Value = read_json(&metadata.join("standalone.json"))?;
    ensure!(
        marker["schema"] == 1,
        "Unsupported directory-helper metadata version"
    );
    private_dir(&metadata)?;
    let mut store = Store {
        root: metadata,
        catalog: crate::catalog::load(catalog)?,
        standalone: None,
    };
    let data = store.read()?;
    ensure!(
        data.projects.len() == 1 && data.schedules.is_empty() && data.servers.is_empty(),
        "Invalid standalone metadata; expected one local project and no management services"
    );
    let project = data.projects.into_iter().next().unwrap();
    ensure!(
        marker["project"] == project.id,
        "Directory-helper identity does not match its project record"
    );
    ensure!(
        project.custom && project.server_id == "local",
        "Directory metadata must describe one local Compose setup"
    );
    directory(&store.root.join("projects"))?;
    directory(&store.project_dir(&project.id)?)?;
    let mut context = Directory {
        root,
        project: project.id.clone(),
        opened_revision: String::new(),
    };
    context.opened_revision = context.revision()?;
    let current = context.read(&project.id)?;
    ensure!(
        project
            .services
            .iter()
            .all(|s| current.compose["services"].get(&s.app).is_some()),
        "A recorded service was removed or renamed; use Docker Compose directly for this changed topology"
    );
    store.standalone = Some(context);
    let project = store.project(&project.id)?;
    Ok((store, project, lock))
}

pub async fn execute(path: &Path, catalog: Option<&Path>, command: AppCommand) -> Result<()> {
    if let AppCommand::Init {
        app,
        stack,
        mut method,
        inputs,
        mut ack,
        start,
        image,
        allow_untested,
    } = command
    {
        let guided = inputs.is_none() && io::stdin().is_terminal() && io::stdout().is_terminal();
        let values = if let Some(file) = &inputs {
            read_json(file)?
        } else if guided {
            let temp = tempfile::tempdir()?;
            let prompt_store = Store::with_catalog(temp.path().join("data"), catalog)?;
            crate::app_assist::init_inputs(&prompt_store, &app, stack, &mut method, &mut ack)?
        } else {
            BTreeMap::new()
        };
        if guided {
            crate::guided::review(
                &json!({"application":app,"deployment":method,"inputs":values,"start_after_creation":start}),
            );
            if !crate::guided::confirm("Create this application directory?", false)? {
                println!("Cancelled. No application files were created.");
                return Ok(());
            }
        }
        let instructions = create_versioned_files(
            path,
            &app,
            stack,
            method.as_deref(),
            values,
            ack,
            catalog,
            image.map(|image| crate::versions::Selection {
                image,
                allow_untested,
            }),
        )?;
        let (store, project, _lock) = open(path, catalog)?;
        print(
            &json!({"created":true,"compose":"compose.yaml","compose_project":format!("selfhost-{}",project.id),"instructions":instructions,"next":["docker compose -f compose.yaml --env-file .env up -d","selfhost app start","selfhost app update"],"dashboard_required":false,"started":false}),
        )?;
        if start {
            store.action(&project.id, "start").await?;
            println!("Application started. No Selfhost process needs to stay running.");
            if guided {
                for service in &project.services {
                    if service.definition.onboarding.is_some()
                        && crate::guided::confirm(
                            &format!("Set up {} now?", service.definition.name),
                            true,
                        )?
                    {
                        crate::app_assist::setup(&store, &project.id, &service.app).await?;
                    }
                    if service
                        .definition
                        .integration
                        .as_ref()
                        .is_some_and(|profile| profile.oidc_client.is_some())
                        && crate::guided::confirm(
                            &format!(
                                "Connect {} to an identity provider?",
                                service.definition.name
                            ),
                            false,
                        )?
                    {
                        crate::app_assist::connect(&store, &project.id, &service.app).await?;
                    }
                }
            }
        }
        return Ok(());
    }
    let (store, project, _lock) = open(path, catalog)?;
    let id = &project.id;
    match command {
        AppCommand::Init { .. } => unreachable!(),
        AppCommand::Versions { service } => {
            let app = project
                .services
                .iter()
                .find(|s| s.app == service)
                .context("Service not found")?;
            print(&crate::versions::options(&app.definition))?;
        }
        AppCommand::VersionPlan {
            service,
            image,
            allow_untested,
        } => print(&store.version_plan(
            id,
            &service,
            &crate::versions::Selection {
                image,
                allow_untested,
            },
        )?)?,
        AppCommand::VersionApply {
            service,
            image,
            allow_untested,
            revision,
        } => print(&store.version_apply(
            id,
            &service,
            &crate::versions::Selection {
                image,
                allow_untested,
            },
            &revision,
        )?)?,
        AppCommand::Update => crate::app_assist::update(&store, id).await?,
        AppCommand::Start | AppCommand::Stop | AppCommand::Restart => {
            let action = match command {
                AppCommand::Start => "start",
                AppCommand::Stop => "stop",
                AppCommand::Restart => "restart",
                _ => "refresh",
            };
            store.action(id, action).await?;
            println!(
                "Application {action} completed. Compose and environment files remain user-owned."
            );
        }
        AppCommand::Status => {
            println!("{}", store.docker(&project, &["ps", "--all"]).await?);
        }
        AppCommand::Logs { service } => {
            println!("{}", store.service_logs(id, &service).await?);
        }
        AppCommand::Actions { service } => {
            let definition = &project
                .services
                .iter()
                .find(|s| s.app == service)
                .context("Service not found")?
                .definition;
            print(
                &json!({"recipe_actions":definition.actions,"integration":definition.integration}),
            )?;
        }
        AppCommand::Action {
            service,
            action,
            inputs,
        } => {
            if inputs.is_none() {
                return crate::app_assist::action(&store, id, &service, &action).await;
            }
            if project
                .services
                .iter()
                .find(|s| s.app == service)
                .is_some_and(|s| s.definition.actions.iter().any(|a| a.id == action))
            {
                ensure!(inputs.is_none(), "This recipe action does not take inputs");
                println!("{}", store.service_action(id, &service, &action).await?);
            } else {
                let values = inputs
                    .as_deref()
                    .map(read_json)
                    .transpose()?
                    .unwrap_or_default();
                let result =
                    Box::pin(store.integration_action(id, &service, &action, values)).await?;
                print(&result)?;
                ensure!(result["ok"] == true, "App workflow did not complete");
            }
        }
        AppCommand::Config { service, edit } => {
            if edit {
                crate::app_assist::settings(&store, id, &service).await?;
            } else {
                print(&Box::pin(store.integration_state(id, &service)).await?)?;
            }
        }
        AppCommand::Plan { service, file } => {
            print(&Box::pin(store.integration_plan(id, &service, &read_json(&file)?)).await?)?
        }
        AppCommand::Apply {
            service,
            file,
            revision,
        } => {
            if let Some(file) = file {
                let revision = revision.context("File automation requires --revision")?;
                print(
                    &Box::pin(store.integration_apply(id, &service, read_json(&file)?, &revision))
                        .await?,
                )?;
            } else {
                crate::app_assist::settings(&store, id, &service).await?;
            }
        }
        AppCommand::Setup { service, inspect } => {
            if inspect {
                print(&store.onboarding_info(id, &service)?)?;
            } else {
                crate::app_assist::setup(&store, id, &service).await?;
            }
        }
        AppCommand::SetupPlan { service, file } => print(
            &Box::pin(store.onboarding_plan(id, &service, crate::onboarding::read_request(&file)?))
                .await?,
        )?,
        AppCommand::SetupApply {
            service,
            file,
            revision,
        } => {
            if let Some(file) = file {
                let revision = revision.context("File automation requires --revision")?;
                print(
                    &Box::pin(store.onboarding_apply(
                        id,
                        &service,
                        crate::onboarding::read_request(&file)?,
                        &revision,
                    ))
                    .await?,
                )?;
            } else {
                crate::app_assist::setup(&store, id, &service).await?;
            }
        }
        AppCommand::ConnectAccount {
            service,
            file,
            token_env,
        } => {
            let mut request: crate::connections::ConnectRequest = read_json(&file)?;
            ensure!(
                request.credential.is_empty(),
                "Use the provider token environment variable"
            );
            request.credential =
                std::env::var(token_env).context("Set the provider token environment variable")?;
            print(&Box::pin(store.connection_account(id, &service, &request)).await?)?;
        }
        AppCommand::SyncPlan { service, file } => {
            let request = crate::onboarding::OnboardingRequest {
                mode: "sync".into(),
                inputs: BTreeMap::new(),
                apps: read_json(&file)?,
            };
            print(&Box::pin(store.onboarding_plan(id, &service, request)).await?)?;
        }
        AppCommand::Sync {
            service,
            file,
            revision,
        } => {
            let Some(file) = file else {
                return crate::app_assist::sync(&store, id, &service).await;
            };
            let revision = revision.context("File automation requires --revision")?;
            let request = crate::onboarding::OnboardingRequest {
                mode: "sync".into(),
                inputs: BTreeMap::new(),
                apps: read_json(&file)?,
            };
            print(&Box::pin(store.onboarding_apply(id, &service, request, &revision)).await?)?;
        }
        AppCommand::ConnectPlan { service, file } => {
            print(&store.connection_plan(id, &service, &read_json(&file)?)?)?
        }
        AppCommand::Connect {
            service,
            file,
            revision,
            token_env,
        } => {
            let Some(file) = file else {
                return crate::app_assist::connect(&store, id, &service).await;
            };
            let revision = revision.context("File automation requires --revision")?;
            let mut request: crate::connections::ConnectRequest = read_json(&file)?;
            ensure!(
                request.credential.is_empty(),
                "Keep provider credentials out of the request file; use --token-env"
            );
            request.revision = revision;
            request.credential = std::env::var(token_env)
                .context("Provider token environment variable is not set")?;
            let result = Box::pin(store.connection_create(id, &service, request)).await?;
            print(&result)?;
            ensure!(
                result["action"]["ok"] == true,
                "App connection needs attention; use connection --resume after checking the saved receipt"
            );
        }
        AppCommand::Connection { service, resume } => {
            let result = if resume {
                Box::pin(store.connection_resume(id, &service)).await?
            } else {
                store.connection_state(id, &service)?
            };
            print(&result)?;
            if resume {
                ensure!(
                    result["action"]["ok"] == true,
                    "App connection needs attention"
                );
            }
        }
        AppCommand::Backups { service } => print(&json!(store.integration_backups(id, &service)?))?,
        AppCommand::Restore {
            service,
            backup,
            revision,
        } => {
            if let Some(revision) = revision {
                print(
                    &Box::pin(store.integration_restore(id, &service, &backup, Some(&revision)))
                        .await?,
                )?;
            } else {
                crate::guided::interactive()?;
                let plan = Box::pin(store.integration_restore(id, &service, &backup, None)).await?;
                crate::guided::review(&crate::app_assist::settings_review(
                    &plan,
                    &store.integration(id, &service)?,
                ));
                if crate::guided::confirm("Restore these application settings?", false)? {
                    let revision = plan["revision"]
                        .as_str()
                        .context("Restore plan has no revision")?;
                    crate::guided::review(
                        &Box::pin(store.integration_restore(id, &service, &backup, Some(revision)))
                            .await?,
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialization_selects_default_deployment_with_portable_config_files() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("configured");
        create_files(&dir, "node-red", false, None, BTreeMap::new(), vec![], None).unwrap();
        assert!(dir.join("compose.yaml").is_file());
        assert!(dir.join("files").is_dir());
        let compose = fs::read_to_string(dir.join("compose.yaml")).unwrap();
        assert!(compose.contains("node-red-configured"));
        let simple = temp.path().join("basic");
        create_files(
            &simple,
            "node-red",
            false,
            Some("docker"),
            BTreeMap::new(),
            vec![],
            None,
        )
        .unwrap();
        let compose = fs::read_to_string(simple.join("compose.yaml")).unwrap();
        assert!(!compose.contains("node-red-configured"));
    }
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("app");
        create_files(&dir, "homarr", false, None, BTreeMap::new(), vec![], None).unwrap();
        (temp, dir)
    }
    #[test]
    fn edited_compose_runtime_coordinates_replace_frozen_metadata_without_writes() {
        let (_temp, dir) = fixture();
        let state = fs::read(dir.join(".selfhost/state.json")).unwrap();
        let mut compose =
            crate::setup::parse_compose(&fs::read_to_string(dir.join("compose.yaml")).unwrap())
                .unwrap();
        compose["services"]["homarr"]["ports"] = json!(["127.0.0.1:18995:7575"]);
        compose["services"]["homarr"]["image"] = json!("ghcr.io/homarr-labs/homarr:v1.77.3");
        fs::write(
            dir.join("compose.yaml"),
            serde_yaml::to_string(&compose).unwrap(),
        )
        .unwrap();
        let (store, project, _lock) = open(&dir, None).unwrap();
        assert_eq!(project.services[0].port, 18995);
        assert_eq!(
            store.project(&project.id).unwrap().services[0].image,
            "ghcr.io/homarr-labs/homarr:v1.77.3"
        );
        assert_eq!(
            project.services[0].definition.image,
            "ghcr.io/homarr-labs/homarr:v1.77.2"
        );
        assert_eq!(fs::read(dir.join(".selfhost/state.json")).unwrap(), state);
        assert_eq!(
            published_ipv4_port(
                &json!({"ports":[{"host_ip":"127.0.0.1","published":"18996","target":7575}]}),
                7575
            )
            .unwrap(),
            Some(18996)
        );
        for ports in [
            json!(["127.0.0.1:18995:9999"]),
            json!(["127.0.0.1:18995:7575/udp"]),
            json!(["[::1]:18995:7575"]),
            json!(["192.0.2.1:18995:7575"]),
        ] {
            assert_eq!(
                published_ipv4_port(&json!({"ports":ports}), 7575).unwrap(),
                None
            );
        }
        assert!(
            published_ipv4_port(
                &json!({"ports":["127.0.0.1:18995:7575","0.0.0.0:18996:7575"]}),
                7575
            )
            .is_err()
        );
    }
    #[test]
    fn version_changes_preserve_raw_environment_and_user_compose_fields() {
        let (_temp, dir) = fixture();
        let mut env = fs::read_to_string(dir.join(".env")).unwrap();
        env.push_str("\n# kept exactly\nUSER_VALUE='literal-$value # comment'\n");
        fs::write(dir.join(".env"), &env).unwrap();
        let mut compose =
            crate::setup::parse_compose(&fs::read_to_string(dir.join("compose.yaml")).unwrap())
                .unwrap();
        compose["x-user-note"] = json!("preserve");
        fs::write(
            dir.join("compose.yaml"),
            serde_yaml::to_string(&compose).unwrap(),
        )
        .unwrap();
        let (store, project, _lock) = open(&dir, None).unwrap();
        let choice = crate::versions::Selection {
            image: "ghcr.io/homarr-labs/homarr:v1.77.3".into(),
            allow_untested: true,
        };
        let plan = store.version_plan(&project.id, "homarr", &choice).unwrap();
        store
            .version_apply(
                &project.id,
                "homarr",
                &choice,
                plan["revision"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(fs::read_to_string(dir.join(".env")).unwrap(), env);
        let after =
            crate::setup::parse_compose(&fs::read_to_string(dir.join("compose.yaml")).unwrap())
                .unwrap();
        assert_eq!(after["x-user-note"], "preserve");
        assert_eq!(
            after["services"]["homarr"]["volumes"],
            compose["services"]["homarr"]["volumes"]
        );
        assert_eq!(after["services"]["homarr"]["image"], choice.image);
    }
    #[test]
    fn plain_files_survive_without_optional_metadata_and_init_never_overwrites() {
        let (_temp, dir) = fixture();
        assert!(Store::open(dir.join(".selfhost")).is_err());
        let original = fs::read(dir.join(".env")).unwrap();
        assert!(create_files(&dir, "homarr", false, None, BTreeMap::new(), vec![], None).is_err());
        assert_eq!(fs::read(dir.join(".env")).unwrap(), original);
        fs::rename(dir.join(".selfhost"), dir.join("saved-metadata")).unwrap();
        let compose =
            crate::setup::parse_compose(&fs::read_to_string(dir.join("compose.yaml")).unwrap())
                .unwrap();
        assert!(compose["name"].as_str().unwrap().starts_with("selfhost-p-"));
        assert!(compose["services"]["homarr"].is_object());
        assert_eq!(fs::read(dir.join(".env")).unwrap(), original);
        assert!(open(&dir, None).is_err());
        assert!(!dir.join(".selfhost").exists());
    }
    #[test]
    fn user_edits_are_authoritative_and_render_never_replaces_them() {
        let (_temp, dir) = fixture();
        let (store, project, _lock) = open(&dir, None).unwrap();
        let mut compose = store.setup(&project.id).unwrap().compose;
        compose["services"]["homarr"]["environment"]["USER_SETTING"] = json!("kept");
        fs::write(
            dir.join("compose.yaml"),
            serde_yaml::to_string(&compose).unwrap(),
        )
        .unwrap();
        let env = b"# user comments\nAPP_SECRET='literal$value'\n";
        fs::write(dir.join(".env"), env).unwrap();
        store.write_compose(&project).unwrap();
        assert_eq!(store.setup(&project.id).unwrap().compose, compose);
        assert_eq!(fs::read(dir.join(".env")).unwrap(), env);
        assert!(
            store
                .save_standalone_setup(&project.id, store.setup(&project.id).unwrap())
                .is_err()
        );
        compose["name"] = json!("different-volume-owner");
        fs::write(
            dir.join("compose.yaml"),
            serde_yaml::to_string(&compose).unwrap(),
        )
        .unwrap();
        assert!(store.setup(&project.id).is_err());
    }
    #[test]
    fn configuration_write_preserves_dotenv_and_saves_recovery_snapshot() {
        let (_temp, dir) = fixture();
        let env = b"# preserve exact bytes\nAPP_SECRET='literal$value'\n";
        fs::write(dir.join(".env"), env).unwrap();
        let (store, project, _lock) = open(&dir, None).unwrap();
        let mut setup = store.setup(&project.id).unwrap();
        setup
            .files
            .insert("settings.yaml".into(), "enabled: true\n".into());
        store.save_standalone_setup(&project.id, setup).unwrap();
        assert_eq!(fs::read(dir.join(".env")).unwrap(), env);
        assert_eq!(store.read().unwrap().snapshots.len(), 1);
        assert_eq!(
            fs::read_to_string(dir.join("files/settings.yaml")).unwrap(),
            "enabled: true\n"
        );
        assert!(!store.root.join("runtime.pending.json").exists());
    }
    #[test]
    fn interrupted_write_and_concurrent_helper_fail_closed() {
        let (_temp, dir) = fixture();
        let (store, _, lock) = open(&dir, None).unwrap();
        assert!(open(&dir, None).is_err());
        drop(lock);
        fs::write(store.root.join("runtime.pending.json"), "{}").unwrap();
        assert!(open(&dir, None).is_err());
    }

    #[test]
    fn connection_plan_binds_current_files_across_helper_invocations() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("cloud");
        create_files(
            &dir,
            "nextcloud",
            false,
            None,
            BTreeMap::new(),
            vec![],
            None,
        )
        .unwrap();
        let request: crate::connections::ConnectRequest = serde_json::from_value(json!({
            "provider":"zitadel", "issuer":"https://identity.example.com", "provider_project":"example-project", "app_url":"https://files.example.com", "name":"Cloud login"
        })).unwrap();
        let previous = {
            let (store, project, _lock) = open(&dir, None).unwrap();
            store
                .connection_plan(&project.id, "nextcloud", &request)
                .unwrap()
        };
        fs::write(dir.join(".env"), "# changed by the owner\n").unwrap();
        let (store, project, _lock) = open(&dir, None).unwrap();
        let next = store
            .connection_plan(&project.id, "nextcloud", &request)
            .unwrap();
        assert_ne!(previous["revision"], next["revision"]);
        assert!(
            !store
                .project_dir(&project.id)
                .unwrap()
                .join("connections")
                .exists()
        );
    }

    #[tokio::test]
    async fn native_yaml_plan_rejects_stale_env_and_applies_without_docker() {
        let (_temp, dir) = fixture();
        {
            let (store, project, _lock) = open(&dir, None).unwrap();
            let profile = serde_json::from_slice(
                &crate::catalog::BuiltinCatalog::get("integrations/zitadel.json")
                    .unwrap()
                    .data,
            )
            .unwrap();
            store
                .mutate(|data| {
                    data.projects[0].services[0].definition.integration = Some(profile);
                    Ok(())
                })
                .unwrap();
            let mut compose = store.setup(&project.id).unwrap().compose;
            compose["services"]["homarr"]["command"] =
                json!(["start", "--config", "/config/zitadel.yaml"]);
            compose["services"]["homarr"]["volumes"]
                .as_array_mut()
                .unwrap()
                .push(json!("./files/zitadel.yaml:/config/zitadel.yaml:ro"));
            fs::write(
                dir.join("compose.yaml"),
                serde_yaml::to_string(&compose).unwrap(),
            )
            .unwrap();
            fs::create_dir(dir.join("files")).unwrap();
            fs::write(
                dir.join("files/zitadel.yaml"),
                "ExternalDomain: identity.example.com\nLog:\n  Level: info\n",
            )
            .unwrap();
        }
        let changes = BTreeMap::from([("log-level".into(), json!("warn"))]);
        let old = {
            let (store, project, _lock) = open(&dir, None).unwrap();
            store
                .integration_plan(&project.id, "homarr", &changes)
                .await
                .unwrap()
        };
        let env = b"# keep this comment\nUNCHANGED='special$value'\n";
        fs::write(dir.join(".env"), env).unwrap();
        let (store, project, _lock) = open(&dir, None).unwrap();
        assert!(
            store
                .integration_apply(
                    &project.id,
                    "homarr",
                    changes.clone(),
                    old["revision"].as_str().unwrap()
                )
                .await
                .is_err()
        );
        let plan = store
            .integration_plan(&project.id, "homarr", &changes)
            .await
            .unwrap();
        let result = store
            .integration_apply(
                &project.id,
                "homarr",
                changes,
                plan["revision"].as_str().unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result["verified"], true);
        assert_eq!(fs::read(dir.join(".env")).unwrap(), env);
        assert!(
            fs::read_to_string(dir.join("files/zitadel.yaml"))
                .unwrap()
                .contains("warn")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_linked_private_metadata_before_any_external_write() {
        use std::os::unix::fs::symlink;
        let (temp, dir) = fixture();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, dir.join(".selfhost/snapshots")).unwrap();
        assert!(open(&dir, None).is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}
