use super::*;

pub(super) async fn servers(store: &Store) -> Result<()> {
    loop {
        let list = store.servers()?;
        let mut options = vec![
            "Add a server connection".into(),
            "Docker move jobs and recovery".into(),
            "Proxmox migration tasks".into(),
        ];
        options.extend(labels(&list, |s| {
            format!(
                "{} [{}]",
                s.name,
                if s.read_only {
                    "read-only"
                } else {
                    "management enabled"
                }
            )
        }));
        let choice = take!(select("Servers and moves", &options));
        match choice {
            0 => {
                let kind = take!(menu(
                    "Connection type",
                    &[
                        "Docker over SSH (host, VM or LXC)",
                        "Existing Docker context",
                        "Proxmox over SSH"
                    ]
                ));
                let name = take!(required("Server name", ""));
                let endpoint = take!(required("Configured SSH alias or Docker context", ""));
                let group = take!(field("Group (optional)", ""));
                let app_host = take!(field("Browser hostname for apps (optional)", ""));
                let server = Server {
                    id: String::new(),
                    name,
                    provider: [
                        Provider::DockerSsh,
                        Provider::DockerContext,
                        Provider::ProxmoxSsh,
                    ][kind]
                        .clone(),
                    endpoint,
                    group,
                    app_host,
                    read_only: true,
                };
                show("Saved read-only connection", &store.add_server(server)?)?;
            }
            1 => {
                let jobs = store.move_jobs()?;
                let selected = take!(select(
                    "Docker moves",
                    &labels(&jobs, |j| format!(
                        "{}: {} ({})",
                        j.project_id, j.stage, j.status
                    ))
                ));
                show("Move details", &jobs[selected])?;
                if yes("Recover this interrupted move? Review its source and destination first.")? {
                    let out =
                        wait("Recovering move", store.recover_move(&jobs[selected].id)).await?;
                    show("Move recovery", &out)?;
                }
            }
            2 => {
                let jobs = store.guest_moves()?;
                show("Proxmox migration records", &jobs)?;
                if yes("Check a Proxmox task now?")? {
                    let server = take!(server_pick(store, false, false));
                    let node = take!(required("Node name", ""));
                    let task = take!(required("Task UPID", "UPID:"));
                    let out = wait(
                        "Checking Proxmox task",
                        store.guest_task(&server.id, &node, &task),
                    )
                    .await?;
                    show("Task status", &out)?;
                }
            }
            _ => {
                let server = &list[choice - 3];
                match take!(menu(
                    &server.name,
                    &[
                        "Inspect infrastructure",
                        "Check Docker engine",
                        "Edit connection and permissions",
                        "Move a stopped VM or LXC"
                    ]
                )) {
                    0 => {
                        let out = wait(
                            "Inspecting infrastructure",
                            store.infrastructure_inventory(&server.id),
                        )
                        .await?;
                        show("Infrastructure", &out)?;
                    }
                    1 => {
                        let out = wait("Checking Docker engine", store.server_statuses(&server.id))
                            .await?;
                        show("Docker engine", &out)?;
                    }
                    2 => {
                        ensure!(
                            server.id != "local",
                            "The local connection cannot be replaced"
                        );
                        let mut edited = server.clone();
                        edited.name = take!(required("Server name", &server.name));
                        edited.endpoint =
                            take!(required("SSH alias or Docker context", &server.endpoint));
                        edited.group = take!(field("Group", &server.group));
                        edited.app_host = take!(field("App hostname", &server.app_host));
                        edited.read_only = take!(menu(
                            "Management permission",
                            &["Read-only", "Allow changes"]
                        )) == 0;
                        if !edited.read_only
                            && !confirm_name("Allow changes on this server", &server.name)?
                        {
                            continue;
                        }
                        if yes("Save server connection?")? {
                            show("Saved connection", &store.edit_server(&server.id, edited)?)?;
                        }
                    }
                    _ => {
                        ensure!(
                            server.provider == Provider::ProxmoxSsh,
                            "Choose a Proxmox connection"
                        );
                        let source_node = take!(required("Source node", ""));
                        let target_node = take!(required("Destination node", ""));
                        let kind = take!(menu("Guest type", &["Virtual machine", "LXC container"]));
                        let vmid = take!(number("Guest ID", 100));
                        let input = crate::migration::GuestMove {
                            source_node,
                            target_node,
                            kind: if kind == 0 { "qemu" } else { "lxc" }.into(),
                            vmid: vmid.try_into()?,
                        };
                        let plan = wait(
                            "Planning guest move",
                            store.guest_move_plan(&server.id, &input),
                        )
                        .await?;
                        if review("Offline guest migration", &plan)? {
                            let out = wait(
                                "Submitting guest migration",
                                store.migrate_guest(&server.id, &input),
                            )
                            .await?;
                            show("Guest migration", &out)?;
                        }
                    }
                }
            }
        }
    }
}
pub(super) async fn project_move(store: &Store, project: &Project) -> Result<()> {
    let destination = take!(server_pick(store, false, true));
    let plan = wait(
        "Planning workload move",
        store.relocation_plan(&project.id, &destination.id),
    )
    .await?;
    show("Move plan", &plan)?;
    match take!(menu(
        "Choose the move operation",
        &[
            "Cancel",
            "Copy data and move workload",
            "Assign an empty, undeployed project"
        ]
    )) {
        1 => {
            if confirm_name("Confirm workload move and downtime", &project.name)? {
                let out = wait(
                    "Moving workload",
                    store.begin_move(&project.id, &destination.id),
                )
                .await?;
                show("Move result", &out)?;
            }
        }
        2 if confirm_name("Assign project without moving existing data", &project.name)? => {
            let out = wait(
                "Assigning project",
                store.assign_project(&project.id, &destination.id),
            )
            .await?;
            show("Assigned project", &project_summary(&out))?;
        }
        _ => {}
    }
    Ok(())
}
pub(super) async fn databases(store: &Store) -> Result<()> {
    loop {
        let sources = store.database_sources()?;
        let mut options = vec![
            "Add a database source".into(),
            "Configure a project's database".into(),
        ];
        options.extend(labels(&sources, |s| {
            format!(
                "{} ({})",
                s["name"].as_str().unwrap_or(""),
                s["kind"].as_str().unwrap_or("")
            )
        }));
        let choice = take!(select("Databases", &options));
        match choice {
            0 => {
                let mode = take!(menu(
                    "Database source",
                    &[
                        "Host a shared database server",
                        "Connect an existing shared database server",
                        "Connect a project-specific external database"
                    ]
                ));
                let drivers = crate::catalog::database_drivers()?;
                let selected = take!(select(
                    "Database engine",
                    &labels(&drivers, |driver| driver["name"]
                        .as_str()
                        .unwrap_or("")
                        .into())
                ));
                let driver = &drivers[selected];
                let default_port = driver["port"].as_u64().context("Missing database port")?;
                let name = take!(required(
                    "Source name",
                    driver["name"].as_str().unwrap_or("Database")
                ));
                let server = take!(server_pick(store, false, true));
                let mut input = crate::database::NewSource {
                    name,
                    engine: driver["engine"].as_str().context("Missing engine")?.into(),
                    auth_database: driver["auth_database"].as_str().unwrap_or("").into(),
                    kind: if mode == 2 { "external" } else { "shared" }.into(),
                    managed: mode == 0,
                    server_id: server.id,
                    host: String::new(),
                    port: default_port.try_into()?,
                    username: String::new(),
                    password: String::new(),
                    database: String::new(),
                    ssl_mode: "require".into(),
                };
                if mode != 0 {
                    input.host = take!(required("Database host", ""));
                    input.port = take!(number("Database port", default_port)).try_into()?;
                    input.username = take!(required("Database username", ""));
                    input.password = take!(prompt("Database password", "Hidden input", "", true));
                    input.database = take!(required("Database name", ""));
                    if driver["auth_database"].is_string() {
                        input.auth_database =
                            take!(required("Authentication database", &input.auth_database));
                    }
                    let tls = take!(menu(
                        "Database TLS",
                        &[
                            "Require encryption",
                            "Verify hostname and certificate",
                            "Disable TLS (private local network only)"
                        ]
                    ));
                    input.ssl_mode = ["require", "verify-full", "disable"][tls].into();
                }
                let id = store.add_database_source(input)?;
                show("Database source saved", &json!({"id":id,"started":false}))?;
            }
            1 => {
                let project = take!(project_pick(store));
                Box::pin(project_database(store, &project)).await?;
            }
            _ => {
                let source = &sources[choice - 2];
                show("Database source (credentials hidden)", source)?;
                if source["managed_project"].is_string()
                    && yes("Start this managed shared database server?")?
                {
                    wait(
                        "Starting database server",
                        store.start_database_source(
                            source["id"].as_str().context("Source ID missing")?,
                        ),
                    )
                    .await?;
                    view("Database started", "Managed PostgreSQL server started.")?;
                }
            }
        }
    }
}
pub(super) async fn project_database(store: &Store, project: &Project) -> Result<()> {
    loop {
        match take!(menu(
            "Project database",
            &[
                "Current database binding",
                "Configure dedicated, shared or external database",
                "Provision project database and role",
                "Create database backup",
                "List, export or restore database backups"
            ]
        )) {
            0 => show("Database binding", &store.setup(&project.id)?.database)?,
            1 => {
                let mode = take!(menu(
                    "Database arrangement",
                    &[
                        "Dedicated container for this project",
                        "Shared server with isolated project database",
                        "External database"
                    ]
                ));
                let source_id = if mode == 0 {
                    None
                } else {
                    let sources = store.database_sources()?;
                    let index = take!(select(
                        "Database source",
                        &labels(&sources, |s| s["name"].as_str().unwrap_or("").into())
                    ));
                    Some(
                        sources[index]["id"]
                            .as_str()
                            .context("Source ID missing")?
                            .into(),
                    )
                };
                let engine = if mode == 0 {
                    let drivers: Vec<_> = crate::catalog::database_drivers()?
                        .into_iter()
                        .filter(|driver| {
                            project
                                .services
                                .iter()
                                .filter_map(|service| service.definition.database.as_ref())
                                .all(|recipe| {
                                    driver["engine"] == recipe.engine
                                        || recipe
                                            .engines
                                            .iter()
                                            .any(|engine| driver["engine"] == *engine)
                                })
                        })
                        .collect();
                    let selected = take!(select(
                        "Database engine",
                        &labels(&drivers, |driver| driver["name"]
                            .as_str()
                            .unwrap_or("")
                            .into())
                    ));
                    Some(
                        drivers[selected]["engine"]
                            .as_str()
                            .context("Missing engine")?
                            .into(),
                    )
                } else {
                    None
                };
                if confirm_name(
                    "Change desired database configuration; existing data is not migrated automatically",
                    &project.name,
                )? {
                    store.configure_database(
                        &project.id,
                        crate::database::Selection {
                            mode: ["dedicated", "shared", "external"][mode].into(),
                            source_id,
                            engine,
                        },
                    )?;
                    view(
                        "Database configured",
                        "Review connection settings and migration requirements before starting the app.",
                    )?;
                }
            }
            2 => {
                if confirm_name(
                    "Create this project's isolated role and database",
                    &project.name,
                )? {
                    wait(
                        "Provisioning database",
                        store.provision_database(&project.id),
                    )
                    .await?;
                    view(
                        "Database provisioned",
                        "The project's isolated database and role are ready.",
                    )?;
                }
            }
            3 => {
                let out = wait(
                    "Creating database backup",
                    store.backup_database(&project.id),
                )
                .await?;
                show("Database backup", &out)?;
            }
            _ => {
                let backups = store.database_backups(&project.id)?;
                let index = take!(select(
                    "Database backups",
                    &labels(&backups, |b| b.id.clone())
                ));
                let backup = &backups[index];
                show("Backup details", backup)?;
                match take!(menu(
                    "Database backup",
                    &[
                        "Back",
                        "Export verified archive",
                        "Restore archive (stops apps, takes safety backup)"
                    ]
                )) {
                    1 => {
                        let path = take!(required("New destination filename", "database.dump"));
                        store.export_database_backup(&project.id, &backup.id, Path::new(&path))?;
                        view("Exported", "The verified database archive was exported.")?;
                    }
                    2 if confirm_name(
                        "Restore database and stop app containers",
                        &project.name,
                    )? =>
                    {
                        let out = wait(
                            "Restoring database",
                            store.restore_database(&project.id, &backup.id, &project.id),
                        )
                        .await?;
                        show("Database restore", &out)?;
                    }
                    _ => {}
                }
            }
        }
    }
}
pub(super) async fn remove(
    store: &Store,
    project: &Project,
    service: Option<String>,
) -> Result<()> {
    let mode = take!(menu(
        "Removal scope",
        &[
            "Archive from Selfhost, leave runtime and data",
            "Remove owned containers, preserve volumes and data"
        ]
    ));
    let backup = take!(menu(
        "Backup before removal",
        &[
            "Configuration snapshot (does not include app data)",
            "Configuration and supported PostgreSQL database",
            "No backup (explicit acknowledgement required)"
        ]
    ));
    let request = crate::removal::RemovalRequest {
        service,
        mode: if mode == 0 {
            crate::removal::RemovalMode::Archive
        } else {
            crate::removal::RemovalMode::RemoveContainers
        },
        backup: match backup {
            0 => crate::removal::BackupChoice::Configuration,
            1 => crate::removal::BackupChoice::ConfigurationAndDatabase,
            _ => crate::removal::BackupChoice::None,
        },
    };
    let plan = wait(
        "Inspecting ownership and backup scope",
        store.plan_removal(&project.id, request),
    )
    .await?;
    show("Review exact removal plan", &plan)?;
    if !yes(
        "I understand that volumes and data are preserved, but container writable layers are not backed up.",
    )? {
        return Ok(());
    }
    if backup == 2 && !yes("Continue without creating a backup?")? {
        return Ok(());
    }
    if !confirm_name("Confirm the reviewed removal", &plan.confirmation)? {
        return Ok(());
    }
    let result = wait(
        "Applying reviewed removal",
        store.apply_removal(
            &project.id,
            crate::removal::RemovalApply {
                plan_id: plan.id,
                revision: plan.revision,
                confirmation: plan.confirmation,
                acknowledge_preserved_data: true,
                acknowledge_no_backup: backup == 2,
            },
        ),
    )
    .await?;
    show("Removal result", &result)
}
pub(super) async fn recovery(store: &Store) -> Result<()> {
    match take!(menu(
        "Backups and recovery",
        &[
            "Configuration snapshots",
            "Database backups",
            "Removed projects and interrupted removals",
            "Docker move recovery"
        ]
    )) {
        0 => {
            let snapshots = store.read()?.snapshots;
            let index = take!(select(
                "Configuration snapshots",
                &labels(&snapshots, |s| format!("{} - {}", s.project_name, s.id))
            ));
            if confirm_name(
                "Restore desired configuration only; runtime is not started",
                &snapshots[index].project_name,
            )? {
                show(
                    "Configuration restored",
                    &project_summary(&store.restore_config(&snapshots[index].id)?),
                )?;
            }
        }
        1 => {
            let project = take!(project_pick(store));
            Box::pin(project_database(store, &project)).await?;
        }
        2 => {
            let archives = store.removal_archives()?;
            let index = take!(select(
                "Removal archives",
                &labels(&archives, |a| format!(
                    "{} [{}] {}",
                    a["project_name"].as_str().unwrap_or(""),
                    a["status"].as_str().unwrap_or(""),
                    a["id"].as_str().unwrap_or("")
                ))
            ));
            let archive = &archives[index];
            show("Archive details", archive)?;
            let id = archive["id"].as_str().context("Archive ID missing")?;
            let name = archive["project_name"]
                .as_str()
                .context("Project name missing")?;
            let mode = take!(menu(
                "Recovery operation",
                &[
                    "Back",
                    "Restore archived project record (leave stopped)",
                    "Reconcile interrupted removal (no runtime writes)"
                ]
            ));
            if mode > 0 && confirm_name("Confirm project record recovery", name)? {
                if mode == 1 {
                    show(
                        "Project record restored",
                        &project_summary(&store.restore_removed_project(id, name)?),
                    )?;
                } else {
                    let out =
                        wait("Reconciling removal", store.reconcile_removal(id, name)).await?;
                    show("Reconciliation result", &out)?;
                }
            }
        }
        _ => {
            let jobs = store.move_jobs()?;
            let index = take!(select(
                "Move recovery",
                &labels(&jobs, |j| format!("{} {}", j.project_id, j.status))
            ));
            show("Move details", &jobs[index])?;
            if yes("Run recovery for this move?")? {
                let out = wait("Recovering move", store.recover_move(&jobs[index].id)).await?;
                show("Move recovery", &out)?;
            }
        }
    }
    Ok(())
}
pub(super) fn activity(store: &Store) -> Result<()> {
    loop {
        match take!(menu(
            "Activity and schedules",
            &[
                "Activity history",
                "Mark activity as read",
                "Scheduled operations",
                "Create or change a schedule"
            ]
        )) {
            0 => show("Activity history", &store.read()?.activities)?,
            1 => {
                store.mark_read()?;
                view("Activity", "All current activity marked as read.")?;
            }
            2 => show(
                "Schedules (run while selfhost serve is active)",
                &store.read()?.schedules,
            )?,
            _ => {
                let project = take!(project_pick(store));
                let mut actions = vec!["snapshot".to_string(), "refresh".into()];
                for service in &project.services {
                    if let Ok(profile) = store.integration(&project.id, &service.app) {
                        for action in profile.actions.iter().filter(|a| a.schedulable) {
                            actions.push(format!("app:{}:{}", service.app, action.id));
                        }
                    }
                }
                let index = take!(select("Scheduled operation", &actions));
                let hours = take!(number("Interval in hours", 24));
                let enabled = take!(menu("Schedule state", &["Disabled", "Enabled"])) == 1;
                if yes(
                    "Save this schedule? Enabled actions run while the dashboard server is running.",
                )? {
                    show(
                        "Schedule saved",
                        &store.schedule(Schedule {
                            project_id: project.id,
                            action: actions[index].clone(),
                            interval_hours: hours.try_into()?,
                            enabled,
                            next_run: 0,
                        })?,
                    )?;
                }
            }
        }
    }
}

pub(super) async fn networking(store: &Store) -> Result<()> {
    loop {
        let network = store.networking()?;
        match take!(menu(
            "Networking",
            &[
                "View proxies, routes and private networks",
                "Add a reverse proxy",
                "Edit a reverse proxy",
                "Register an existing private network",
                "Plan and apply a service route",
                "Probe upstream TLS from the proxy host"
            ]
        )) {
            0 => show("Networking (credentials hidden)", &network)?,
            choice @ (1 | 2) => {
                let edit = choice == 2;
                let profiles = store.proxy_profiles()?;
                let mut proxy = if edit {
                    let proxies: Vec<crate::networking::Proxy> =
                        serde_json::from_value(network["data"]["proxies"].clone())?;
                    let index = take!(select(
                        "Saved proxies",
                        &labels(&proxies, |p| p.name.clone())
                    ));
                    proxies[index].clone()
                } else {
                    let profiles: Vec<_> = profiles.values().collect();
                    let index = take!(select(
                        "Proxy software",
                        &labels(&profiles, |p| format!("{}: {}", p.name, p.description))
                    ));
                    crate::networking::Proxy {
                        id: String::new(),
                        name: profiles[index].name.clone(),
                        provider: profiles[index].id.clone(),
                        ssh_alias: String::new(),
                        admin_url: String::new(),
                        api_token: String::new(),
                        ca_certificate: String::new(),
                        config_directory: String::new(),
                        settings: profiles[index].defaults.clone(),
                        read_only: true,
                    }
                };
                proxy.name = take!(required("Proxy connection name", &proxy.name));
                proxy.ssh_alias = take!(field(
                    "SSH alias of proxy host or guest (optional)",
                    &proxy.ssh_alias
                ));
                proxy.admin_url = take!(field("Proxy administration URL", &proxy.admin_url));
                proxy.config_directory = take!(field(
                    "Proxy dynamic config directory (file providers)",
                    &proxy.config_directory
                ));
                proxy.api_token = take!(prompt(
                    "Proxy API credential",
                    "Leave empty to preserve the saved credential on the same target",
                    "",
                    true
                ));
                if yes("Configure a private certificate authority for the proxy API?")? {
                    proxy.ca_certificate = take!(editor(
                        "CA certificate PEM (public certificate only)",
                        &proxy.ca_certificate
                    ));
                }
                if let Some(profile) = profiles.get(&proxy.provider) {
                    for (key, rule) in &profile.settings_schema {
                        if let Some((other, expected)) = &rule.required_when
                            && proxy.settings.get(other) != Some(expected)
                        {
                            continue;
                        }
                        let value = take!(dynamic_input(
                            &rule.label,
                            &rule.kind,
                            proxy.settings.get(key).unwrap_or(&Value::Null),
                            &[]
                        ));
                        proxy.settings.insert(key.clone(), value);
                    }
                }
                proxy.read_only = take!(menu(
                    "Proxy management permission",
                    &["Read-only", "Allow reviewed route changes"]
                )) == 0;
                if !proxy.read_only
                    && !confirm_name("Allow changes through this proxy connection", &proxy.name)?
                {
                    continue;
                }
                if yes("Save the proxy connection?")? {
                    let id = if edit {
                        store.save_proxy(proxy)?
                    } else {
                        store.add_proxy(proxy)?
                    };
                    show("Proxy saved", &json!({"id":id}))?;
                }
            }
            3 => {
                let profiles = network["network_profiles"]
                    .as_array()
                    .context("Network profiles missing")?;
                let choice = take!(select(
                    "Private network",
                    &labels(profiles, |p| p["name"].as_str().unwrap_or("").into())
                ));
                view(
                    "Network setup requirements",
                    profiles[choice]["guidance"].as_str().unwrap_or(""),
                )?;
                let name = take!(required("Connection name", "Private network"));
                let description = take!(field("Description", ""));
                let endpoints = strings(&take!(field(
                    "Peer identities or endpoints (comma separated)",
                    ""
                )));
                let policy_reference =
                    take!(required("Scoped firewall or access-policy reference", ""));
                let id = store.add_network(crate::networking::PrivateNetwork {
                    id: String::new(),
                    name,
                    provider: profiles[choice]["id"].as_str().unwrap_or("").into(),
                    description,
                    endpoints,
                    policy_reference,
                })?;
                show(
                    "Private network recorded",
                    &json!({"id":id,"note":"Peers and ACLs are configured through your network provider."}),
                )?;
            }
            choice => {
                let proxies = network["data"]["proxies"]
                    .as_array()
                    .context("Proxy list missing")?;
                let proxy = take!(select(
                    "Proxy for this route",
                    &labels(proxies, |p| p["name"].as_str().unwrap_or("").into())
                ));
                let project = take!(project_pick(store));
                let service = take!(select(
                    "Service",
                    &labels(&project.services, |s| s.definition.name.clone())
                ));
                let domain = take!(required("Route hostname", "app.example.com"));
                let upstream = take!(required(
                    "Upstream HTTPS URL (localhost HTTP only for a verified local hop)",
                    "https://"
                ));
                let network_id = if yes("Associate this route with a registered private network?")?
                {
                    let networks = network["data"]["networks"]
                        .as_array()
                        .context("Network list missing")?;
                    let selected = take!(select(
                        "Private network",
                        &labels(networks, |p| p["name"].as_str().unwrap_or("").into())
                    ));
                    Some(
                        networks[selected]["id"]
                            .as_str()
                            .context("Network ID missing")?
                            .into(),
                    )
                } else {
                    None
                };
                let route = crate::networking::Route {
                    id: format!("route-{}", crate::core::token(5)?),
                    proxy_id: proxies[proxy]["id"]
                        .as_str()
                        .context("Proxy ID missing")?
                        .into(),
                    project_id: project.id.clone(),
                    service: project.services[service].app.clone(),
                    domain,
                    upstream,
                    network_id,
                };
                if choice == 5 {
                    let out = wait("Probing upstream TLS", store.route_probe(&route)).await?;
                    show("TLS probe", &out)?;
                } else {
                    let plan =
                        wait("Planning native proxy route", store.route_plan(&route)).await?;
                    if review("Review exact proxy route", &plan)? {
                        let out = wait(
                            "Applying proxy route",
                            store.route_apply(route, revision(&plan)?),
                        )
                        .await?;
                        show("Route result", &out)?;
                    }
                }
            }
        }
    }
}

pub(super) async fn identity(store: &Store) -> Result<()> {
    loop {
        match take!(menu(
            "Identity and Selfhost login",
            &[
                "View login settings (credentials hidden)",
                "Connect an existing identity provider",
                "Change localhost/domain address",
                "Inspect an identity-provider directory",
                "Apply a directory login manifest",
                "Register an OIDC client automatically",
                "Edit or remove a login provider",
                "Advanced complete login configuration"
            ]
        )) {
            0 => show(
                "Login configuration",
                &store.login_config()?.map(|c| c.public()),
            )?,
            1 => {
                let mut config = store.login_config()?.unwrap_or_default();
                if config.public_url.is_empty() {
                    config.public_url = take!(required(
                        "Selfhost URL (localhost HTTP or domain HTTPS)",
                        "http://localhost:8372"
                    ));
                }
                let id = take!(required(
                    "Connection ID (lowercase letters and hyphens)",
                    "my-identity"
                ));
                let name = take!(required("Provider display name", "My identity provider"));
                let issuer = take!(required("OIDC issuer URL", "https://identity.example.com"));
                let client_id = take!(required("Registered OIDC client ID", ""));
                let client_secret = take!(prompt(
                    "Client secret (optional for a public PKCE client)",
                    "Hidden input",
                    "",
                    true
                ));
                let subjects = strings(&take!(required(
                    "Administrator subject IDs (comma separated, not email addresses)",
                    ""
                )));
                let ca_certificate =
                    if yes("Does the provider use a private certificate authority?")? {
                        take!(editor("CA certificate PEM", ""))
                    } else {
                        String::new()
                    };
                config.providers.push(crate::auth::LoginProvider {
                    id,
                    name,
                    issuer,
                    client_id,
                    client_secret,
                    ca_certificate,
                    admin_subjects: subjects,
                });
                login_apply(store, config)?;
            }
            2 => {
                let mut config = store
                    .login_config()?
                    .context("Connect an identity provider first")?;
                config.public_url = take!(required("New Selfhost URL", &config.public_url));
                login_apply(store, config)?;
            }
            3 => {
                let path = take!(required("Identity provider configuration directory", "."));
                let url = take!(required("Selfhost URL", "http://localhost:8372"));
                show(
                    "Read-only configuration discovery",
                    &crate::identity_setup::inspect(Path::new(&path), &url)?,
                )?;
            }
            4 => {
                let path = take!(required("Directory containing selfhost-login.json", "."));
                let config = crate::identity_setup::read_manifest(Path::new(&path))?;
                login_apply(store, config)?;
            }
            5 => {
                let profiles = crate::identity_setup::registration_profiles()?;
                let profiles = profiles
                    .as_array()
                    .context("Registration profiles missing")?;
                let selected = take!(select(
                    "Provider supporting automatic registration",
                    &labels(profiles, |p| p["name"].as_str().unwrap_or("").into())
                ));
                let create_project = take!(menu(
                    "Provider project",
                    &[
                        "Create a new Selfhost project",
                        "Use an existing project (advanced)"
                    ]
                )) == 0;
                let mut request = crate::identity_setup::RegistrationRequest {
                    provider: profiles[selected]["id"].as_str().unwrap_or("").into(),
                    issuer: take!(required("Issuer origin", "https://identity.example.com")),
                    provider_project: if create_project {
                        String::new()
                    } else {
                        take!(required("Existing provider project ID", ""))
                    },
                    create_project,
                    project_name: if create_project {
                        take!(required("New provider project name", "Selfhost"))
                    } else {
                        "Selfhost".into()
                    },
                    organization_id: String::new(),
                    selfhost_url: take!(required("Selfhost URL", "http://localhost:8372")),
                    id: take!(required("Connection ID", "my-identity")),
                    name: take!(required("Provider display name", "My identity provider")),
                    admin_subjects: vec![],
                    ca_certificate: if yes("Use a private CA certificate?")? {
                        take!(editor("CA certificate PEM", ""))
                    } else {
                        String::new()
                    },
                };
                if yes("Set an explicit organization ID (advanced)?")? {
                    request.organization_id = take!(required("Provider organization ID", ""));
                }
                let credential = take!(prompt(
                    "Temporary provider API credential",
                    "Hidden input; used only for this reviewed registration",
                    "",
                    true
                ));
                if take!(menu(
                    "Administrator account",
                    &[
                        "Look up the human account for this credential",
                        "Enter administrator subject IDs manually"
                    ]
                )) == 0
                {
                    match wait(
                        "Looking up account (read-only)",
                        crate::identity_setup::registration_account(&request, &credential),
                    )
                    .await
                    {
                        Ok(account) => {
                            show("Review provider account", &account)?;
                            if let Some(subject) = account["suggested_subject"].as_str() {
                                if yes(
                                    "Grant this reviewed human account administrator access to Selfhost?",
                                )? {
                                    request.admin_subjects.push(subject.into());
                                }
                            } else {
                                view(
                                    "Choose a human administrator",
                                    "This credential does not identify a human account. Enter a human administrator's exact subject ID.",
                                )?;
                            }
                        }
                        Err(error) => view(
                            "Account lookup unavailable",
                            &format!(
                                "{error:#}\nEnter administrator subject IDs manually to continue."
                            ),
                        )?,
                    }
                }
                if request.admin_subjects.is_empty() {
                    request.admin_subjects = strings(&take!(required(
                        "Administrator subject IDs (comma separated, not email addresses)",
                        ""
                    )));
                }
                let plan = crate::identity_setup::registration_plan(store, &request)?;
                if review("Review client registration", &plan)? {
                    let out = wait(
                        "Registering OIDC client",
                        crate::identity_setup::register(
                            store,
                            request,
                            revision(&plan)?,
                            &credential,
                        ),
                    )
                    .await?;
                    show("Registration result", &out)?;
                }
            }
            6 => {
                let mut config = store
                    .login_config()?
                    .context("No identity provider configured")?;
                let index = take!(select(
                    "Choose login provider",
                    &labels(&config.providers, |p| p.name.clone())
                ));
                match take!(menu(
                    "Provider changes",
                    &[
                        "Edit common fields",
                        "Edit advanced settings",
                        "Remove provider"
                    ]
                )) {
                    0 => {
                        let provider = &mut config.providers[index];
                        provider.name = take!(required("Provider name", &provider.name));
                        provider.issuer = take!(required("Issuer URL", &provider.issuer));
                        provider.client_id = take!(required("Client ID", &provider.client_id));
                        if yes("Replace the client secret?")? {
                            provider.client_secret =
                                take!(prompt("Client secret", "Hidden input", "", true));
                        }
                        provider.admin_subjects = strings(&take!(required(
                            "Administrator subject IDs (comma separated)",
                            &provider.admin_subjects.join(",")
                        )));
                    }
                    1 => {
                        if !yes("Advanced provider settings include a client secret. Open editor?")?
                        {
                            continue;
                        }
                        config.providers[index] = take!(json_edit(
                            "Provider configuration",
                            &config.providers[index]
                        ));
                    }
                    _ => {
                        if !confirm_name(
                            "Remove this provider from Selfhost login",
                            &config.providers[index].name,
                        )? {
                            continue;
                        }
                        config.providers.remove(index);
                    }
                }
                login_apply(store, config)?;
            }
            _ => {
                let current = store
                    .login_config()?
                    .unwrap_or_else(|| crate::auth::LoginConfig {
                        public_url: "http://localhost:8372".into(),
                        providers: vec![],
                    });
                let mode = take!(menu(
                    "Advanced login configuration",
                    &[
                        "Edit JSON in this terminal (contains secrets)",
                        "Read a private JSON file"
                    ]
                ));
                let config = if mode == 0 {
                    if !yes("Open configuration containing client secrets?")? {
                        continue;
                    }
                    take!(json_edit("Complete login configuration", &current))
                } else {
                    take!(load_json("Private login JSON file"))
                };
                login_apply(store, config)?;
            }
        }
    }
}
fn login_apply(store: &Store, config: crate::auth::LoginConfig) -> Result<()> {
    let plan = crate::identity_setup::plan(store, &config)?;
    show("Review login change and callback URLs", &plan)?;
    if !yes(
        "Have you registered every exact callback URL shown in the plan? Keep the previous callback until sign-in is tested.",
    )? {
        return Ok(());
    }
    if !yes("Apply these reviewed login settings? Current identity sessions will expire.")? {
        return Ok(());
    }
    show(
        "Saved login configuration",
        &crate::identity_setup::apply(store, config, revision(&plan)?, true)?,
    )?;
    Ok(())
}
