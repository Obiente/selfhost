mod access_diagnostics;
mod adoption;
mod app_assist;
mod auth;
mod backups;
mod catalog;
mod connections;
mod core;
mod dashboard;
mod database;
mod database_mongo;
mod database_mysql;
mod deployments;
mod guided;
mod identity_assist;
mod identity_setup;
mod infrastructure;
mod infrastructure_assist;
mod integrations;
mod migration;
mod networking;
mod onboarding;
mod removal;
mod server;
mod setup;
mod ssh_keys;
mod stacks;
mod standalone;
mod tasks;
mod tui;
mod updates;
mod versions;
mod workflow;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use core::{CreateProject, Schedule, Store};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "selfhost",
    bin_name = "selfhost",
    version,
    about = "A home for the services you run"
)]
struct Cli {
    /// Directory for projects, secrets, and configuration snapshots
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    /// Additional TOML service manifests; matching ids override recipes for new projects
    #[arg(long, global = true)]
    catalog_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    /// Guided setup for apps, servers, databases, proxies and recurring tasks
    Guide,
    /// Connect a Docker or Proxmox server interactively
    ServerAdd,
    /// Create and authorize dedicated SSH keys for servers and proxies
    Ssh {
        #[command(subcommand)]
        command: ssh_keys::SshCommand,
    },
    /// Review and run automatic app tasks and service-link triggers
    Task {
        #[command(subcommand)]
        command: tasks::TaskCommand,
    },
    /// Set up and reuse apps in ordinary Compose directories, without a dashboard
    App {
        #[arg(long, default_value = ".", global = true)]
        directory: PathBuf,
        #[command(subcommand)]
        command: standalone::AppCommand,
    },
    /// Keep the dashboard running and configure access through a reverse proxy
    Dashboard {
        #[command(subcommand)]
        command: dashboard::DashboardCommand,
    },
    /// Check for and install a reviewed Selfhost update
    Update {
        #[command(subcommand)]
        command: Option<UpdateCommands>,
    },
    #[command(hide = true)]
    UpdateHelper {
        #[arg(long)]
        job: String,
    },
    /// List contributor-defined deployment methods
    Deployments,
    /// Choose an app and deployment method; an input file is optional for automation
    Deploy {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Connect an identity provider or inspect its existing configuration directory
    Identity {
        #[command(subcommand)]
        command: IdentityCommands,
    },
    /// Link, inspect and grant specific actions on independently managed apps
    Existing {
        #[command(subcommand)]
        command: ExistingCommands,
    },
    /// Review removal, choose backups and restore archived project records
    Removal {
        #[command(subcommand)]
        command: RemovalCommands,
    },
    /// List installable multi-service stacks
    Stacks,
    /// Create a portable stack from an installation JSON file, without starting it
    StackCreate { file: Option<PathBuf> },
    /// List connected proxies, networks and provider capabilities
    Networking,
    /// Connect a reverse proxy with guided questions
    ProxyAdd { file: Option<PathBuf> },
    /// Update a saved proxy connection from a private JSON file containing its ID
    ProxyEdit { file: Option<PathBuf> },
    /// Choose a proxy and service, then review and apply its route
    RouteSetup,
    /// Register an existing private network and its access-policy reference
    NetworkAdd { file: Option<PathBuf> },
    /// Preview native proxy configuration for a route JSON file
    RoutePlan { file: PathBuf },
    /// Apply a reviewed route on a writable proxy
    RouteApply {
        file: PathBuf,
        #[arg(long)]
        revision: String,
    },
    /// Test upstream HTTPS from the proxy host
    RouteProbe { file: PathBuf },
    /// Configure Selfhost's own OIDC login from a private JSON file; omit file to show redacted settings
    LoginConfig { file: Option<PathBuf> },
    /// List saved native app configuration changes
    AppBackups { project: String, service: String },
    /// Preview a native configuration restore, then apply with the returned revision
    AppRestore {
        project: String,
        service: String,
        backup: String,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Inspect available automatic application setup and saved progress
    AppSetup {
        project: String,
        service: String,
        #[arg(long)]
        inspect: bool,
    },
    /// Choose service links and add them to a configured app dashboard
    AppSync { project: String, service: String },
    /// Select app versions, review backups and upgrade a project
    AppUpdate { project: String },
    /// Preview declarative first-run setup or existing API-key connection
    AppSetupPlan {
        project: String,
        service: String,
        file: PathBuf,
    },
    /// Apply reviewed app setup; secrets may be supplied through environment variables
    AppSetupApply {
        project: String,
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Find the authenticated human account before choosing an app administrator
    AppConnectAccount {
        project: String,
        service: String,
        file: PathBuf,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        token_env: String,
    },
    /// Review registration of an OIDC client from a connection JSON file
    AppConnectPlan {
        project: String,
        service: String,
        file: PathBuf,
    },
    /// Create the reviewed client and configure the app; read the token from an environment variable
    AppConnect {
        project: String,
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        token_env: String,
    },
    /// Read connection progress, or resume configuring its existing client
    AppConnection {
        project: String,
        service: String,
        #[arg(long)]
        resume: bool,
    },
    /// Read the app-specific integration and typed settings
    AppConfig {
        project: String,
        service: String,
        #[arg(long)]
        edit: bool,
    },
    /// Preview typed settings from a JSON object of field ids and values
    AppPlan {
        project: String,
        service: String,
        file: PathBuf,
    },
    /// Apply a reviewed settings JSON file with the preview revision
    AppApply {
        project: String,
        service: String,
        file: Option<PathBuf>,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Run a declared app-management workflow; input JSON may contain secrets
    AppAction {
        project: String,
        service: String,
        action: String,
        #[arg(long)]
        inputs: Option<PathBuf>,
    },
    /// Copy a verified database archive for use with its engine's native tools
    DatabaseBackupExport {
        project: String,
        backup: String,
        output: PathBuf,
    },
    /// Create a native database archive in the private project store
    DatabaseBackup { project: String },
    /// List database archives for a project
    DatabaseBackups { project: String },
    /// Restore a project archive after stopping its app containers; saves a safety backup
    DatabaseRestore {
        project: String,
        backup: String,
        #[arg(long)]
        confirm_project: String,
    },
    /// List database sources (credentials are omitted)
    Databases,
    /// List database engines and their default connection ports
    DatabaseEngines,
    /// Add a database source from a private JSON file
    DatabaseAdd { file: Option<PathBuf> },
    /// Choose, connect and prepare a project's database
    DatabaseSetup { project: Option<String> },
    /// Edit a saved database connection
    DatabaseEdit { source: Option<String> },
    /// Remove an unused database source after review
    DatabaseRemove { source: Option<String> },
    /// Start a managed shared database server
    DatabaseStart { source: String },
    /// Attach a dedicated, external or shared database to a project
    DatabaseAttach {
        project: String,
        #[arg(value_parser=["dedicated","external","shared"])]
        mode: String,
        #[arg(long)]
        source: Option<String>,
        /// Engine supported by the app recipe; defaults to its source or recipe
        #[arg(long)]
        engine: Option<String>,
    },
    /// Create this project's isolated database and role on its shared source
    DatabaseProvision { project: String },
    /// Create a custom setup from ordinary Compose YAML or JSON (does not start it)
    CreateSetup {
        name: String,
        #[arg(long)]
        compose: PathBuf,
        /// JSON map of environment variable names to values
        #[arg(long)]
        environment: Option<PathBuf>,
        /// Directory containing config files referenced as ./files/NAME
        #[arg(long)]
        files: Option<PathBuf>,
        #[arg(long, default_value = "local")]
        server: String,
    },
    /// Read or replace a setup JSON document, including Compose, environment and files
    Setup {
        project: String,
        #[arg(long)]
        file: Option<PathBuf>,
        /// Explicitly include secret values in output
        #[arg(long)]
        reveal: bool,
    },
    /// Edit environment variables; values are read from stdin, never command arguments
    Env {
        project: String,
        #[arg(long)]
        set: Option<String>,
        #[arg(long, conflicts_with = "set")]
        unset: Option<String>,
        #[arg(long)]
        reveal: bool,
    },
    /// Edit a mounted configuration file
    Config {
        project: String,
        name: String,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        remove: bool,
    },
    /// Export standalone Compose, environment and config files (includes secrets, excludes volume data)
    Export { project: String, output: PathBuf },
    /// Open the dashboard server (loopback by default)
    Serve {
        /// Listener IP; remote access requires a configured HTTPS origin and identity provider
        #[arg(long, default_value = "127.0.0.1")]
        bind: std::net::IpAddr,
        #[arg(long, default_value_t = 8372)]
        port: u16,
        /// Temporarily allow one-use sign-in on a concrete private/VPN IP without an identity provider
        #[arg(long)]
        setup: bool,
    },
    /// Browse and control projects in the terminal
    Tui,
    /// Validate and list the service catalog
    Catalog,
    /// List recipe versions, optionally check its upstream release source
    Versions {
        app: String,
        #[arg(long)]
        check: bool,
    },
    /// Review a change to the desired app image without changing running services
    VersionPlan {
        project: String,
        service: String,
        image: String,
        #[arg(long)]
        allow_untested: bool,
    },
    /// Save a reviewed app version; recreate the service separately when ready
    VersionApply {
        project: String,
        service: String,
        image: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        allow_untested: bool,
    },
    /// Create a project without starting containers
    Create {
        #[arg(long, default_value = "local")]
        server: String,
        name: String,
        #[arg(long, value_delimiter = ',', required = true)]
        apps: Vec<String>,
        /// Select an image as APP=IMAGE (repeat for multiple apps)
        #[arg(long = "image")]
        images: Vec<String>,
        /// Acknowledge compatibility review for versions outside the recipe's tested set
        #[arg(long)]
        allow_untested: bool,
    },
    /// List projects
    List,
    /// Print a project's Compose plan (custom inline values may contain secrets)
    Plan { project: String },
    /// Start, stop, restart, refresh images, or snapshot a project
    Run {
        project: String,
        #[arg(value_parser = ["start", "stop", "restart", "refresh", "snapshot"])]
        action: String,
    },
    /// Schedule configuration snapshots or image refreshes while serve is running
    Schedule {
        project: String,
        action: String,
        #[arg(long, default_value_t = 24)]
        hours: u32,
        #[arg(long)]
        disabled: bool,
    },
    /// Restore desired configuration from a snapshot; use start to apply it
    Restore { snapshot: String },
    /// Add project apps to a dashboard using its manifest's API recipe
    Sync {
        project: String,
        target: String,
        #[arg(long, default_value = "SELFHOST_DASHBOARD_API_KEY")]
        key_env: String,
    },
    /// Check the Docker connection
    Doctor,
    /// Show a service's current CPU, memory, network and disk I/O
    Stats { project: String, service: String },
    /// Show the last 200 lines of service output
    Logs { project: String, service: String },
    /// Run start, stop, restart, update, or an action from the service recipe
    Service {
        project: String,
        service: String,
        action: String,
    },
}
#[derive(Subcommand)]
enum IdentityCommands {
    /// Guided setup from your provider directory; no JSON file required
    Setup {
        #[arg(default_value = ".")]
        directory: PathBuf,
    },
    /// Paste and review a private setup code on the dashboard host
    Connect,
    /// Read provider hints without executing scripts or printing credentials
    Inspect {
        #[arg(default_value = ".")]
        directory: PathBuf,
        #[arg(long, default_value = "http://localhost:8372")]
        selfhost_url: String,
    },
    /// Review a login manifest and its exact callback URLs
    Plan {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        directory: PathBuf,
    },
    /// Save reviewed settings after provider callbacks are configured
    Apply {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        directory: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long = "confirm-callbacks")]
        callbacks_confirmed: bool,
    },
    /// Review automatic client registration on a supported provider
    RegisterPlan {
        #[arg(long)]
        file: PathBuf,
    },
    /// Look up the credential's account for explicit administrator selection
    RegisterAccount {
        #[arg(long)]
        file: PathBuf,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        credential_env: String,
    },
    /// Register the reviewed client; provider credential is read from the environment
    Register {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long, default_value = "SELFHOST_IDP_TOKEN")]
        credential_env: String,
    },
}
#[derive(Subcommand)]
enum ExistingCommands {
    /// List independently managed connections
    List,
    /// List supported connection profiles and their declared actions
    Profiles,
    /// Discover and connect an existing application
    Link {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Review a replacement container for an existing connection
    Reconnect { id: Option<String> },
    /// Inspect the pinned container without making changes
    Inspect { id: String },
    /// Read resource usage for the pinned container
    Stats { id: String },
    /// Explicitly allow selected write actions using a reviewed JSON file
    Permissions {
        id: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Run a declared action; write actions require the exact saved app name
    Action {
        id: String,
        action: String,
        #[arg(long, default_value = "")]
        confirm: String,
    },
    /// Remove the connection record, preserving the actual app and all its data
    Unlink {
        id: String,
        #[arg(long)]
        confirm: String,
    },
}
#[derive(Subcommand)]
enum RemovalCommands {
    /// Review exact scope, resource ownership and backup coverage
    Plan {
        project: String,
        #[arg(long)]
        service: Option<String>,
        #[arg(long,default_value="archive",value_parser=["archive","remove_containers"])]
        mode: String,
        #[arg(long,default_value="configuration",value_parser=["configuration","configuration_and_database","none"])]
        backup: String,
    },
    /// Apply a fresh removal plan with explicit confirmations
    Apply {
        project: String,
        #[arg(long)]
        plan_id: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        confirm: String,
        #[arg(long)]
        acknowledge_preserved_data: bool,
        #[arg(long)]
        acknowledge_no_backup: bool,
    },
    /// List archived records and backup references
    Archives,
    /// Reconcile an interrupted removal without starting or deleting containers
    Reconcile {
        archive: String,
        #[arg(long)]
        confirm: String,
    },
    /// Restore an archived project record without starting containers
    Restore {
        archive: String,
        #[arg(long)]
        confirm: String,
    },
}
#[derive(Subcommand)]
enum UpdateCommands {
    /// Check the registry and show installation-specific instructions
    Check,
    /// Review the exact version, scope and update requirements
    Plan,
    /// Build and verify a previously reviewed update without replacing this executable
    Stage {
        #[arg(long)]
        plan_id: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        confirm: String,
    },
    /// Activate a verified update after this process exits
    Activate {
        #[arg(long)]
        job: String,
        #[arg(long)]
        confirm: String,
    },
    /// Recover an interrupted update using its verified previous executable
    Recover {
        #[arg(long)]
        job: String,
        #[arg(long)]
        confirm: String,
    },
    /// Show staged updates and recovery information
    Jobs,
}

fn confirm_update(
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
    question: &str,
) -> Result<bool> {
    write!(output, "{question} [y/N]: ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

async fn interactive_update(store: &Store) -> Result<()> {
    use std::io::{self, IsTerminal};
    let status = store.update_status().await?;
    if io::stdin().is_terminal() {
        guided::review(&status);
    } else {
        println!("{}", serde_json::to_string_pretty(&status)?);
    }
    if status["update_available"] != true || status["can_stage"] != true {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        println!(
            "Use selfhost update plan, stage and activate for a reviewed non-interactive update."
        );
        return Ok(());
    }
    let plan = store.plan_update().await?;
    guided::review(&plan);
    let expected = plan["confirmation"]
        .as_str()
        .context("Missing update confirmation")?;
    if !confirm_update(
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
        &format!(
            "Prepare Selfhost {} for installation?",
            plan["version"].as_str().unwrap_or("update")
        ),
    )? {
        println!("Update cancelled.");
        return Ok(());
    }
    let staged = store
        .stage_update(updates::UpdateApproval {
            plan_id: plan["id"].as_str().context("Missing plan ID")?.into(),
            revision: plan["revision"]
                .as_str()
                .context("Missing revision")?
                .into(),
            confirmation: expected.into(),
        })
        .await?;
    guided::review(&staged);
    let expected = staged["confirmation"]
        .as_str()
        .context("Missing activation confirmation")?;
    if !confirm_update(
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
        &format!(
            "Activate Selfhost {} after this process exits?",
            staged["version"].as_str().unwrap_or("update")
        ),
    )? {
        println!("Update remains staged. Use selfhost update jobs to review it later.");
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &store.activate_update(
                updates::UpdateActivation {
                    job_id: staged["job_id"]
                        .as_str()
                        .context("Missing update job")?
                        .into(),
                    confirmation: expected.into()
                },
                None
            )?
        )?
    );
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    serde_json::from_slice(&std::fs::read(path)?).context("Invalid JSON input file")
}
fn login_manifest(file: Option<PathBuf>, directory: PathBuf) -> Result<auth::LoginConfig> {
    match file {
        Some(file) => read_json(&file),
        None => identity_setup::read_manifest(&directory),
    }
}
fn main() {
    // Windows' small default stack cannot construct the nested orchestration futures.
    let result = std::thread::Builder::new()
        .name("selfhost-main".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(8 * 1024 * 1024)
                .build()?
                .block_on(Box::pin(run()))
        })
        .expect("Unable to start selfhost runtime")
        .join()
        .expect("Selfhost runtime panicked");
    if let Err(error) = result {
        eprintln!("selfhost: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Commands::App { directory, command }) = cli.command {
        anyhow::ensure!(
            cli.data_dir.is_none(),
            "Directory helpers use --directory, not --data-dir"
        );
        return standalone::execute(&directory, cli.catalog_dir.as_deref(), command).await;
    }
    #[cfg(windows)]
    if matches!(
        cli.command,
        Some(Commands::Dashboard {
            command: dashboard::DashboardCommand::Run { .. }
        })
    ) {
        // A scheduled service has no interactive console. Its backend writes private logs.
        unsafe {
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }
    if matches!(cli.command, Some(Commands::Catalog)) {
        for app in catalog::load(cli.catalog_dir.as_deref())? {
            println!("{:<20} {:<18} {}", app.id, app.name, app.image);
        }
        return Ok(());
    }
    let root = match cli.data_dir {
        Some(root) => root,
        None => directories::ProjectDirs::from("org", "Obiente", "selfhost")
            .context("Cannot locate an application data directory; use --data-dir")?
            .data_local_dir()
            .to_path_buf(),
    };
    if let Some(Commands::UpdateHelper { job }) = &cli.command {
        return updates::run_helper(root, job.clone());
    }
    let store = if cli.catalog_dir.is_none() {
        Store::open(root)?
    } else {
        Store::with_catalog(root, cli.catalog_dir.as_deref())?
    };
    if !matches!(
        cli.command,
        Some(Commands::Update { .. })
            | Some(Commands::Ssh { .. })
            | Some(Commands::Identity { .. })
            | Some(Commands::Tui)
            | Some(Commands::Dashboard { .. })
    ) {
        let cached = store
            .cached_update_status()
            .unwrap_or_else(|_| serde_json::json!({}));
        let checked = cached["checked_at"].as_u64().unwrap_or(0);
        let status = if core::now().saturating_sub(checked) > 86400 {
            tokio::time::timeout(
                std::time::Duration::from_secs(3),
                store.update_status_with_timeout(std::time::Duration::from_secs(2)),
            )
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(cached)
        } else {
            cached
        };
        if status["update_available"] == true {
            eprintln!(
                "Selfhost {} is available. Run selfhost update for your installation's update options.",
                status["latest_version"].as_str().unwrap_or("update")
            );
        }
    }
    match cli.command.unwrap_or(Commands::Serve {
        port: 8372,
        bind: std::net::Ipv4Addr::LOCALHOST.into(),
        setup: false,
    }) {
        Commands::Guide => workflow::guide(&store).await?,
        Commands::ServerAdd => infrastructure_assist::server_setup(&store).await?,
        Commands::Ssh { command } => ssh_keys::execute(&store, command).await?,
        Commands::Task { command } => tasks::run(&store, command).await?,
        Commands::App { .. } => unreachable!("directory commands do not open a managed workspace"),
        Commands::Dashboard { command } => {
            dashboard::execute(&store, cli.catalog_dir.as_deref(), command).await?;
        }
        Commands::Update { command } => {
            let result = match command {
                None => return Box::pin(interactive_update(&store)).await,
                Some(UpdateCommands::Check) => store.update_status().await?,
                Some(UpdateCommands::Plan) => store.plan_update().await?,
                Some(UpdateCommands::Stage {
                    plan_id,
                    revision,
                    confirm,
                }) => {
                    Box::pin(store.stage_update(updates::UpdateApproval {
                        plan_id,
                        revision,
                        confirmation: confirm,
                    }))
                    .await?
                }
                Some(UpdateCommands::Activate { job, confirm }) => store.activate_update(
                    updates::UpdateActivation {
                        job_id: job,
                        confirmation: confirm,
                    },
                    None,
                )?,
                Some(UpdateCommands::Recover { job, confirm }) => store.recover_update(
                    updates::UpdateActivation {
                        job_id: job,
                        confirmation: confirm,
                    },
                    None,
                )?,
                Some(UpdateCommands::Jobs) => serde_json::json!(store.update_jobs()?),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::UpdateHelper { .. } => unreachable!("helper exits before opening a workspace"),
        Commands::Deployments => {
            println!("{}", serde_json::to_string_pretty(&store.deployments()?)?)
        }
        Commands::Deploy { file } => {
            let Some(file) = file else {
                return workflow::deploy(&store).await;
            };
            let (project, instructions) = store.create_deployment(read_json(&file)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"project":project,"instructions":instructions})
                )?
            );
        }
        Commands::Identity { command } => {
            let result = match command {
                IdentityCommands::Setup { directory } => {
                    identity_assist::wizard(&store, &directory).await?;
                    return Ok(());
                }
                IdentityCommands::Connect => {
                    identity_assist::connect(&store).await?;
                    return Ok(());
                }
                IdentityCommands::Inspect {
                    directory,
                    selfhost_url,
                } => identity_setup::inspect(&directory, &selfhost_url)?,
                IdentityCommands::Plan { file, directory } => {
                    identity_setup::plan(&store, &login_manifest(file, directory)?)?
                }
                IdentityCommands::Apply {
                    file,
                    directory,
                    revision,
                    callbacks_confirmed,
                } => identity_setup::apply(
                    &store,
                    login_manifest(file, directory)?,
                    &revision,
                    callbacks_confirmed,
                )?,
                IdentityCommands::RegisterPlan { file } => {
                    identity_setup::registration_plan(&store, &read_json(&file)?)?
                }
                IdentityCommands::RegisterAccount {
                    file,
                    credential_env,
                } => {
                    let credential = std::env::var(&credential_env).with_context(|| {
                        format!("Set {credential_env} locally before looking up your account")
                    })?;
                    identity_setup::registration_account(&read_json(&file)?, &credential).await?
                }
                IdentityCommands::Register {
                    file,
                    revision,
                    credential_env,
                } => {
                    let credential = std::env::var(&credential_env)
                        .context("Set the requested provider credential environment variable")?;
                    Box::pin(identity_setup::register(
                        &store,
                        read_json(&file)?,
                        &revision,
                        &credential,
                    ))
                    .await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Existing { command } => {
            let result = match command {
                ExistingCommands::List => serde_json::json!(store.existing_apps()?),
                ExistingCommands::Profiles => serde_json::json!(store.existing_profiles()?),
                ExistingCommands::Link { file } => {
                    let Some(file) = file else {
                        return infrastructure_assist::existing_link(&store).await;
                    };
                    serde_json::json!(Box::pin(store.link_existing(read_json(&file)?)).await?)
                }
                ExistingCommands::Inspect { id } => Box::pin(store.inspect_existing(&id)).await?,
                ExistingCommands::Stats { id } => Box::pin(store.existing_stats(&id)).await?,
                ExistingCommands::Permissions { id, file } => {
                    let Some(file) = file else {
                        return infrastructure_assist::existing_permissions(&store, Some(&id))
                            .await;
                    };
                    serde_json::json!(store.consent_existing(&id, read_json(&file)?)?)
                }
                ExistingCommands::Action {
                    id,
                    action,
                    confirm,
                } => {
                    serde_json::json!({"output":Box::pin(store.existing_action(&id,&action,&confirm)).await?})
                }
                ExistingCommands::Unlink { id, confirm } => {
                    store.unlink_existing(&id, &confirm)?;
                    serde_json::json!({"unlinked":true})
                }
                ExistingCommands::Reconnect { id } => {
                    return infrastructure_assist::existing_reconnect(&store, id.as_deref()).await;
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::Removal { command } => {
            let result = match command {
                RemovalCommands::Plan {
                    project,
                    service,
                    mode,
                    backup,
                } => {
                    let input = serde_json::from_value(
                        serde_json::json!({"service":service,"mode":mode,"backup":backup}),
                    )?;
                    serde_json::json!(Box::pin(store.plan_removal(&project, input)).await?)
                }
                RemovalCommands::Apply {
                    project,
                    plan_id,
                    revision,
                    confirm,
                    acknowledge_preserved_data,
                    acknowledge_no_backup,
                } => {
                    Box::pin(store.apply_removal(
                        &project,
                        removal::RemovalApply {
                            plan_id,
                            revision,
                            confirmation: confirm,
                            acknowledge_preserved_data,
                            acknowledge_no_backup,
                        },
                    ))
                    .await?
                }
                RemovalCommands::Reconcile { archive, confirm } => {
                    Box::pin(store.reconcile_removal(&archive, &confirm)).await?
                }
                RemovalCommands::Archives => serde_json::json!(store.removal_archives()?),
                RemovalCommands::Restore { archive, confirm } => {
                    serde_json::json!(store.restore_removed_project(&archive, &confirm)?)
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }

        Commands::Stacks => println!("{}", serde_json::to_string_pretty(&store.stack_catalog()?)?),
        Commands::StackCreate { file } => {
            let Some(file) = file else {
                return workflow::stack(&store).await;
            };
            let (project, instructions) =
                store.install_blueprint(serde_json::from_slice(&std::fs::read(file)?)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"project":project,"instructions":instructions})
                )?
            );
        }
        Commands::Networking => println!("{}", serde_json::to_string_pretty(&store.networking()?)?),
        Commands::ProxyAdd { file } => {
            if let Some(file) = file {
                println!("{}", store.add_proxy(read_json(&file)?)?);
            } else {
                infrastructure_assist::proxy_setup(&store, None).await?;
            }
        }
        Commands::ProxyEdit { file } => {
            if let Some(file) = file {
                println!("{}", store.save_proxy(read_json(&file)?)?);
            } else {
                infrastructure_assist::proxy_setup(&store, Some("")).await?;
            }
        }
        Commands::NetworkAdd { file } => {
            if let Some(file) = file {
                println!("{}", store.add_network(read_json(&file)?)?);
            } else {
                infrastructure_assist::network_setup(&store).await?;
            }
        }
        Commands::RouteSetup => infrastructure_assist::route_setup(&store).await?,
        Commands::RoutePlan { file } => println!(
            "{}",
            serde_json::to_string_pretty(
                &Box::pin(store.route_plan(&serde_json::from_slice(&std::fs::read(file)?)?))
                    .await?
            )?
        ),
        Commands::RouteApply { file, revision } => println!(
            "{}",
            serde_json::to_string_pretty(
                &Box::pin(
                    store.route_apply(serde_json::from_slice(&std::fs::read(file)?)?, &revision)
                )
                .await?
            )?
        ),
        Commands::RouteProbe { file } => println!(
            "{}",
            serde_json::to_string_pretty(
                &Box::pin(store.route_probe(&serde_json::from_slice(&std::fs::read(file)?)?))
                    .await?
            )?
        ),
        Commands::LoginConfig { file } => {
            if let Some(file) = file {
                store.save_login_config(serde_json::from_slice(&std::fs::read(file)?)?)?;
                println!(
                    "Selfhost login configured. Keep the local recovery link available while testing sign-in."
                );
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&store.login_config()?.map(|c| c.public()))?
                );
            }
        }
        Commands::AppSetup {
            project,
            service,
            inspect,
        } => {
            if inspect {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&store.onboarding_info(&project, &service)?)?
                );
            } else {
                app_assist::setup(&store, &project, &service).await?;
            }
        }
        Commands::AppSync { project, service } => {
            app_assist::sync(&store, &project, &service).await?
        }
        Commands::AppUpdate { project } => app_assist::update(&store, &project).await?,
        Commands::AppSetupPlan {
            project,
            service,
            file,
        } => {
            let request = onboarding::read_request(&file)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &Box::pin(store.onboarding_plan(&project, &service, request)).await?
                )?
            );
        }
        Commands::AppSetupApply {
            project,
            service,
            file,
            revision,
        } => {
            let Some(file) = file else {
                return app_assist::setup(&store, &project, &service).await;
            };
            let revision = revision.context(
                "Provide --revision for automated input, or omit the file for guided setup",
            )?;
            let request = onboarding::read_request(&file)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &Box::pin(store.onboarding_apply(&project, &service, request, &revision))
                        .await?
                )?
            );
        }
        Commands::AppConnectAccount {
            project,
            service,
            file,
            token_env,
        } => {
            let mut request: connections::ConnectRequest =
                serde_json::from_slice(&std::fs::read(file)?)?;
            request.credential =
                std::env::var(token_env).context("Set the provider token environment variable")?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &Box::pin(store.connection_account(&project, &service, &request)).await?
                )?
            );
        }
        Commands::AppConnectPlan {
            project,
            service,
            file,
        } => {
            let request = serde_json::from_slice(&std::fs::read(file)?)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &store.connection_plan(&project, &service, &request)?
                )?
            );
        }
        Commands::AppConnect {
            project,
            service,
            file,
            revision,
            token_env,
        } => {
            let Some(file) = file else {
                return app_assist::connect(&store, &project, &service).await;
            };
            let revision = revision.context(
                "Provide --revision for automated input, or omit the file for guided setup",
            )?;
            let mut request: connections::ConnectRequest =
                serde_json::from_slice(&std::fs::read(file)?)?;
            request.revision = revision;
            request.credential = std::env::var(token_env)
                .context("Set the identity-provider token environment variable")?;
            let result = Box::pin(store.connection_create(&project, &service, request)).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            anyhow::ensure!(
                result["action"]["ok"] == true,
                "Client saved but app configuration needs attention"
            );
        }
        Commands::AppConnection {
            project,
            service,
            resume,
        } => {
            let result = if resume {
                Box::pin(store.connection_resume(&project, &service)).await?
            } else {
                store.connection_state(&project, &service)?
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
            if resume {
                anyhow::ensure!(
                    result["action"]["ok"] == true,
                    "App configuration needs attention"
                );
            }
        }
        Commands::AppBackups { project, service } => println!(
            "{}",
            serde_json::to_string_pretty(&store.integration_backups(&project, &service)?)?
        ),
        Commands::AppRestore {
            project,
            service,
            backup,
            revision,
        } => println!(
            "{}",
            serde_json::to_string_pretty(
                &Box::pin(store.integration_restore(
                    &project,
                    &service,
                    &backup,
                    revision.as_deref()
                ))
                .await?
            )?
        ),
        Commands::AppConfig {
            project,
            service,
            edit,
        } => {
            if edit {
                app_assist::settings(&store, &project, &service).await?;
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &Box::pin(store.integration_state(&project, &service)).await?
                    )?
                );
            }
        }
        Commands::AppPlan {
            project,
            service,
            file,
        } => println!(
            "{}",
            serde_json::to_string_pretty(
                &Box::pin(store.integration_plan(
                    &project,
                    &service,
                    &serde_json::from_slice(&std::fs::read(file)?)?
                ))
                .await?
            )?
        ),
        Commands::AppApply {
            project,
            service,
            file,
            revision,
        } => {
            let Some(file) = file else {
                return app_assist::settings(&store, &project, &service).await;
            };
            let revision = revision.context(
                "Provide --revision for automated input, or omit the file for guided setup",
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &Box::pin(store.integration_apply(
                        &project,
                        &service,
                        serde_json::from_slice(&std::fs::read(file)?)?,
                        &revision
                    ))
                    .await?
                )?
            );
        }
        Commands::AppAction {
            project,
            service,
            action,
            inputs,
        } => {
            if inputs.is_none() {
                return app_assist::action(&store, &project, &service, &action).await;
            }
            let values = match inputs {
                Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
                None => Default::default(),
            };
            let result =
                Box::pin(store.integration_action(&project, &service, &action, values)).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            anyhow::ensure!(result["ok"] == true, "App workflow did not complete");
        }
        Commands::DatabaseBackupExport {
            project,
            backup,
            output,
        } => store.export_database_backup(&project, &backup, &output)?,
        Commands::DatabaseBackup { project } => println!(
            "{}",
            serde_json::to_string_pretty(&Box::pin(store.backup_database(&project)).await?)?
        ),
        Commands::DatabaseBackups { project } => println!(
            "{}",
            serde_json::to_string_pretty(&store.database_backups(&project)?)?
        ),
        Commands::DatabaseRestore {
            project,
            backup,
            confirm_project,
        } => {
            let safety =
                Box::pin(store.restore_database(&project, &backup, &confirm_project)).await?;
            println!("Database restored. Safety backup: {}", safety.id);
        }
        Commands::DatabaseEngines => println!(
            "{}",
            serde_json::to_string_pretty(&catalog::database_drivers()?)?
        ),
        Commands::Databases => println!(
            "{}",
            serde_json::to_string_pretty(&store.database_sources()?)?
        ),
        Commands::DatabaseAdd { file } => {
            if let Some(file) = file {
                println!(
                    "Created database source {}",
                    store.add_database_source(read_json(&file)?)?
                );
            } else {
                infrastructure_assist::database_add(&store).await?;
            }
        }
        Commands::DatabaseSetup { project } => {
            infrastructure_assist::database_setup(&store, project.as_deref()).await?
        }
        Commands::DatabaseEdit { source } => {
            infrastructure_assist::database_edit(&store, source.as_deref()).await?
        }
        Commands::DatabaseRemove { source } => {
            infrastructure_assist::database_remove(&store, source.as_deref()).await?
        }
        Commands::DatabaseStart { source } => {
            store.start_database_source(&source).await?;
            println!("Database server started");
        }
        Commands::DatabaseAttach {
            project,
            mode,
            source,
            engine,
        } => {
            store.configure_database(
                &project,
                database::Selection {
                    mode,
                    source_id: source,
                    engine,
                },
            )?;
            println!(
                "Database configured. Map DATABASE_* variables into your application's environment before starting it."
            );
        }
        Commands::DatabaseProvision { project } => {
            store.provision_database(&project).await?;
            println!("Project database and role created");
        }
        Commands::CreateSetup {
            name,
            compose,
            environment,
            files,
            server,
        } => {
            let mut setup = setup::Setup {
                database: None,
                compose: setup::parse_compose(&std::fs::read_to_string(compose)?)?,
                environment: Default::default(),
                files: Default::default(),
            };
            if let Some(path) = environment {
                setup.environment = serde_json::from_slice(&std::fs::read(path)?)?;
            }
            if let Some(path) = files {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    anyhow::ensure!(
                        entry.file_type()?.is_file(),
                        "Config files must be regular files"
                    );
                    setup.files.insert(
                        entry.file_name().to_string_lossy().into(),
                        std::fs::read_to_string(entry.path())?,
                    );
                }
            }
            let project = store.create_setup(&name, &server, setup)?;
            println!("Created {} ({})", project.name, project.id);
        }
        Commands::Setup {
            project,
            file,
            reveal,
        } => {
            if let Some(path) = file {
                let setup = serde_json::from_slice(&std::fs::read(path)?)?;
                store.save_setup(&project, setup)?;
                println!("Saved. Start the project to apply changes.");
            } else if reveal {
                println!("{}", serde_json::to_string_pretty(&store.setup(&project)?)?);
            } else {
                let setup = store.setup(&project)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"environment_keys":setup.environment.keys().collect::<Vec<_>>(),"files":setup.files.keys().collect::<Vec<_>>(),"services":setup.compose["services"].as_object().map(|s|s.keys().collect::<Vec<_>>())})
                    )?
                );
            }
        }
        Commands::Env {
            project,
            set,
            unset,
            reveal,
        } => {
            let mut setup = store.setup(&project)?;
            if let Some(name) = set {
                use std::io::Read;
                let mut value = String::new();
                std::io::stdin().read_to_string(&mut value)?;
                setup
                    .environment
                    .insert(name, value.trim_end_matches(['\r', '\n']).into());
                store.save_setup(&project, setup)?;
                println!("Environment saved.");
            } else if let Some(name) = unset {
                setup.environment.remove(&name);
                store.save_setup(&project, setup)?;
                println!("Variable removed.");
            } else {
                for (name, value) in setup.environment {
                    println!("{name}={}", if reveal { value } else { "[hidden]".into() });
                }
            }
        }
        Commands::Config {
            project,
            name,
            file,
            remove,
        } => {
            let mut setup = store.setup(&project)?;
            if let Some(path) = file {
                setup.files.insert(name, std::fs::read_to_string(path)?);
                store.save_setup(&project, setup)?;
                println!("Configuration file saved.");
            } else if remove {
                setup.files.remove(&name);
                store.save_setup(&project, setup)?;
                println!("Configuration file removed.");
            } else {
                println!(
                    "{}",
                    setup
                        .files
                        .get(&name)
                        .context("Configuration file not found")?
                );
            }
        }
        Commands::Export { project, output } => {
            store.export_to(&project, &output)?;
            println!(
                "Exported standalone setup. Archive includes credentials; volume data is not included."
            );
        }
        Commands::Serve { port, bind, setup } => {
            Box::pin(server::serve(store, bind, port, setup)).await?
        }
        Commands::Tui => Box::pin(tui::run(store)).await?,
        Commands::Catalog => unreachable!(),
        Commands::Versions { app, check } => {
            let recipe = store
                .catalog
                .iter()
                .find(|item| item.id == app)
                .context("Unknown app")?;
            let result = if check {
                store.catalog_version_check(&app).await?
            } else {
                versions::options(recipe)
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::VersionPlan {
            project,
            service,
            image,
            allow_untested,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&store.version_plan(
                    &project,
                    &service,
                    &versions::Selection {
                        image,
                        allow_untested
                    }
                )?)?
            );
        }
        Commands::VersionApply {
            project,
            service,
            image,
            revision,
            allow_untested,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&store.version_apply(
                    &project,
                    &service,
                    &versions::Selection {
                        image,
                        allow_untested
                    },
                    &revision
                )?)?
            );
        }
        Commands::Create {
            name,
            apps,
            server,
            images,
            allow_untested,
        } => {
            let mut selections = std::collections::BTreeMap::new();
            for value in images {
                let (app, image) = value.split_once('=').context("Use --image APP=IMAGE")?;
                anyhow::ensure!(
                    selections
                        .insert(
                            app.to_owned(),
                            versions::Selection {
                                image: image.to_owned(),
                                allow_untested
                            }
                        )
                        .is_none(),
                    "Choose one version per app"
                );
            }
            let p = store.create_versioned(
                CreateProject {
                    name,
                    apps,
                    server_id: server,
                },
                selections,
            )?;
            println!("Created {} ({})", p.name, p.id);
        }
        Commands::List => {
            for p in store.read()?.projects {
                println!(
                    "{}  {:<25} {}",
                    p.id,
                    p.name,
                    p.services
                        .iter()
                        .map(|s| s.app.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        Commands::Plan { project } => println!(
            "{}",
            serde_json::to_string_pretty(&store.render(&project)?)?
        ),
        Commands::Run { project, action } => {
            Box::pin(store.action(&project, &action)).await?;
            println!("Action completed");
        }
        Commands::Schedule {
            project,
            action,
            hours,
            disabled,
        } => {
            store.schedule(Schedule {
                project_id: project,
                action,
                interval_hours: hours,
                enabled: !disabled,
                next_run: 0,
            })?;
            println!("Schedule saved. Run selfhost serve to process scheduled actions.");
        }
        Commands::Restore { snapshot } => {
            let p = store.restore_config(&snapshot)?;
            println!(
                "Restored configuration for {}. Use selfhost run {} start to apply it.",
                p.name, p.id
            );
        }
        Commands::Sync {
            project,
            target,
            key_env,
        } => {
            let count = store
                .dashboard_sync(
                    &project,
                    &target,
                    std::env::var(&key_env).unwrap_or_default(),
                )
                .await?;
            println!("Added {count} apps to the dashboard");
        }
        Commands::Doctor => {
            store.statuses().await?;
            println!("Docker is available");
        }
        Commands::Stats { project, service } => println!(
            "{}",
            serde_json::to_string_pretty(&store.service_stats(&project, &service).await?)?
        ),
        Commands::Logs { project, service } => {
            println!("{}", store.service_logs(&project, &service).await?)
        }
        Commands::Service {
            project,
            service,
            action,
        } => println!(
            "{}",
            Box::pin(store.service_action(&project, &service, &action)).await?
        ),
    }
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    #[test]
    fn command_tree_has_unique_arguments() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
    #[test]
    fn guided_workflows_accept_no_request_files_or_revision_arguments() {
        for arguments in [
            vec!["guide"],
            vec!["deploy"],
            vec!["stack-create"],
            vec!["server-add"],
            vec!["database-add"],
            vec!["database-setup"],
            vec!["database-edit"],
            vec!["database-remove"],
            vec!["proxy-add"],
            vec!["proxy-edit"],
            vec!["route-setup"],
            vec!["network-add"],
            vec!["existing", "link"],
            vec!["existing", "reconnect"],
            vec!["existing", "permissions", "synthetic-app"],
            vec!["task", "create"],
            vec!["dashboard", "setup"],
            vec!["dashboard", "domain"],
            vec!["app-setup", "synthetic-project", "homarr"],
            vec!["app-connect", "synthetic-project", "nextcloud"],
            vec!["app-apply", "synthetic-project", "nextcloud"],
            vec!["app-sync", "synthetic-project", "homarr"],
            vec!["app-update", "synthetic-project"],
            vec!["app", "setup", "homarr"],
            vec!["app", "connect", "nextcloud"],
            vec!["app", "apply", "nextcloud"],
            vec!["app", "sync", "homarr"],
        ] {
            let mut argv = vec!["selfhost"];
            argv.extend(arguments);
            assert!(
                Cli::try_parse_from(&argv).is_ok(),
                "Guided command requires extra arguments: {argv:?}"
            );
        }
    }
    #[test]
    fn update_confirmation_requires_an_explicit_yes() {
        for answer in ["y\n", "Y\n", "yes\n", " YES \n"] {
            let mut output = Vec::new();
            assert!(confirm_update(&mut answer.as_bytes(), &mut output, "Stage update?").unwrap());
            assert_eq!(String::from_utf8(output).unwrap(), "Stage update? [y/N]: ");
        }
        for answer in [
            "",
            "\n",
            "n\n",
            "no\n",
            "anything\n",
            "UPDATE SELFHOST TO 0.1.2\n",
        ] {
            assert!(
                !confirm_update(&mut answer.as_bytes(), &mut Vec::new(), "Stage update?").unwrap()
            );
        }
    }
    #[test]
    fn windows_invocation_uses_portable_help_name() {
        for args in [
            vec!["selfhost.exe", "--help"],
            vec!["selfhost.exe", "identity"],
            vec!["selfhost.exe", "dashboard", "--help"],
        ] {
            let help = Cli::try_parse_from(args)
                .err()
                .expect("Expected help output")
                .to_string();
            assert!(help.contains("Usage: selfhost"));
            assert!(!help.contains("selfhost.exe"));
        }
    }
}
