//! Interactive infrastructure operations. Plans and revisions remain internal.
use crate::{
    adoption::LinkExisting,
    core::Store,
    database::{NewSource, Selection},
    guided as prompt,
    infrastructure::Provider,
    networking::{PrivateNetwork, Proxy, Route},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

fn input(label: &str, default: Option<&str>) -> Result<String> {
    prompt::input(label, default.unwrap_or_default())
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

fn select(label: &str, rows: &[Value]) -> Result<Value> {
    ensure!(!rows.is_empty(), "No choices available for {label}");
    let labels = rows
        .iter()
        .map(|r| {
            format!(
                "{} ({})",
                r["name"].as_str().unwrap_or("Unnamed"),
                r["id"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    Ok(rows[prompt::choose(label, &labels)?].clone())
}

fn server(store: &Store, write: bool) -> Result<String> {
    let servers = store
        .servers()?
        .into_iter()
        .filter(|s| s.provider != Provider::ProxmoxSsh && (!write || !s.read_only))
        .filter(|s| {
            s.id != "local"
                || store
                    .local_docker_mode()
                    .is_ok_and(|m| !matches!(m, crate::dashboard::LocalDockerMode::Disabled))
        })
        .map(|s| json!({"id":s.id,"name":s.name}))
        .collect::<Vec<_>>();
    Ok(text(&select("Docker host", &servers)?, "id"))
}

fn project(store: &Store, id: Option<&str>) -> Result<String> {
    if let Some(id) = id {
        store.project(id)?;
        return Ok(id.into());
    }
    let rows = store
        .read()?
        .projects
        .into_iter()
        .map(|p| json!({"id":p.id,"name":p.name}))
        .collect::<Vec<_>>();
    Ok(text(&select("Project", &rows)?, "id"))
}

fn source(store: &Store, id: Option<&str>) -> Result<String> {
    if let Some(id) = id {
        store.database_source(id)?;
        return Ok(id.into());
    }
    Ok(text(
        &select("Database source", &store.database_sources()?)?,
        "id",
    ))
}

fn engine(allowed: Option<&[String]>) -> Result<String> {
    let drivers = crate::catalog::database_drivers()?
        .into_iter()
        .filter(|d| allowed.is_none_or(|a| a.contains(&text(d, "engine"))))
        .collect::<Vec<_>>();
    ensure!(
        !drivers.is_empty(),
        "These apps do not share a supported database engine"
    );
    let labels = drivers.iter().map(|d| text(d, "name")).collect::<Vec<_>>();
    Ok(text(
        &drivers[prompt::choose("Database engine", &labels)?],
        "engine",
    ))
}

fn tls(default: &str) -> Result<String> {
    let modes = ["verify-full", "require", "disable"];
    let value = input("TLS mode: verify-full, require, or disable", Some(default))?;
    ensure!(
        modes.contains(&value.as_str()),
        "Choose verify-full, require, or disable"
    );
    Ok(value)
}

pub async fn database_add(store: &Store) -> Result<String> {
    prompt::interactive()?;
    database_add_for(store, None, None).await
}

async fn database_add_for(
    store: &Store,
    placement: Option<&str>,
    target: Option<(&str, &str)>,
) -> Result<String> {
    let kind = match placement {
        Some(kind) => kind.to_owned(),
        None => ["shared", "external"][prompt::choose(
            "Source purpose",
            &[
                "Shared server: create a separate database per project".into(),
                "Existing database and account".into(),
            ],
        )?]
        .into(),
    };
    let engine = match target {
        Some((engine, _)) => engine.to_owned(),
        None => engine(None)?,
    };
    let managed = kind == "shared"
        && prompt::confirm("Host a new shared database server with Selfhost?", true)?;
    let name = input("Source name", Some(&format!("{engine} {kind}")))?;
    let server_id = match target {
        Some((_, host)) => host.to_owned(),
        None => server(store, true)?,
    };
    let driver = crate::catalog::database_driver(&engine)?;
    let mut request = NewSource {
        engine,
        auth_database: String::new(),
        name,
        kind,
        managed,
        server_id,
        host: String::new(),
        port: 0,
        username: String::new(),
        password: String::new(),
        database: String::new(),
        ssl_mode: "verify-full".into(),
    };
    if !managed {
        request.host = input("Database hostname reachable from this Docker host", None)?;
        request.port = input("Port", Some(&driver["port"].to_string()))?
            .parse()
            .context("Port must be a number")?;
        request.database = input("Database name", None)?;
        request.username = input(
            if request.kind == "shared" {
                "Provisioning administrator"
            } else {
                "Database user"
            },
            None,
        )?;
        request.password = prompt::secret("Database password")?;
        request.ssl_mode = tls("verify-full")?;
        if request.engine == "mongodb" {
            request.auth_database = input("Authentication database", Some("admin"))?;
        }
    }
    prompt::review(
        &json!({"name":request.name,"engine":request.engine,"placement":request.kind,"host_new_server":managed,"docker_host":request.server_id,"database_host":request.host,"database":request.database,"user":request.username,"tls":request.ssl_mode}),
    );
    if !prompt::confirm("Save this source and test its connection?", false)? {
        anyhow::bail!("Cancelled; no source saved");
    }
    let id = store.add_database_source(request)?;
    println!("Saved source {id}.");
    if managed {
        store.start_database_source(&id).await?;
    }
    store.test_database_source(&id).await.context(format!("Source {id} is saved. Fix its connection with selfhost database-edit {id}, then retry setup"))?;
    println!("Database connection verified.");
    Ok(id)
}

pub async fn database_setup(store: &Store, id: Option<&str>) -> Result<()> {
    prompt::interactive()?;
    let id = project(store, id)?;
    let current = store.setup(&id)?;
    if let Some(binding) = &current.database {
        if binding.mode == "shared" && !binding.provisioned {
            ensure!(
                !binding.provisioning_uncertain,
                "A previous provisioning attempt needs recovery; inspect its database before retrying"
            );
            if prompt::confirm(
                "Finish creating this project's database and account?",
                false,
            )? {
                if let Some(source_id) = &binding.source_id {
                    if store.database_source(source_id)?.managed_project.is_some() {
                        store.start_database_source(source_id).await?;
                    }
                    store.test_database_source(source_id).await?;
                }
                store.provision_database(&id).await?;
            }
            return Ok(());
        }
        anyhow::bail!(
            "This project already has a database. Database placement changes require a data migration"
        );
    }
    let project = store.project(&id)?;
    let recipes = project
        .services
        .iter()
        .filter_map(|s| s.definition.database.as_ref())
        .collect::<Vec<_>>();
    let supported = crate::catalog::database_drivers()?
        .iter()
        .map(|d| text(d, "engine"))
        .filter(|e| {
            recipes
                .iter()
                .all(|r| r.engine == *e || r.engines.contains(e))
        })
        .collect::<Vec<_>>();
    let engine = engine(Some(&supported))?;
    let mode = ["dedicated", "shared", "external"][prompt::choose(
        "Where should this project's database live?",
        &[
            "Dedicated container for this project".into(),
            "Shared database server, isolated project database".into(),
            "Existing database and account".into(),
        ],
    )?];
    let mut source_id = None;
    if mode != "dedicated" {
        let rows = store
            .database_sources()?
            .into_iter()
            .filter(|s| {
                s["kind"] == mode && s["engine"] == engine && s["server_id"] == project.server_id
            })
            .collect::<Vec<_>>();
        let mut options = rows.iter().map(|s| text(s, "name")).collect::<Vec<_>>();
        options.push("Add a database source".into());
        let selected = prompt::choose("Database source", &options)?;
        let sid = if selected == rows.len() {
            database_add_for(store, Some(mode), Some((&engine, &project.server_id))).await?
        } else {
            text(&rows[selected], "id")
        };
        source_id = Some(sid);
    }
    prompt::review(
        &json!({"project":project.name,"engine":engine,"placement":mode,"source":source_id,"create_project_database_and_account":mode=="shared"}),
    );
    if project.custom {
        println!(
            "This is a custom Compose project. Its services must reference the generated DATABASE_* variables; Selfhost cannot infer an app's database configuration."
        );
    }
    if !prompt::confirm("Configure this database?", false)? {
        return Ok(());
    }
    if let Some(sid) = &source_id {
        if store.database_source(sid)?.managed_project.is_some() {
            store.start_database_source(sid).await?;
        }
        store.test_database_source(sid).await?;
    }
    store.configure_database(
        &id,
        Selection {
            engine: Some(engine),
            mode: mode.into(),
            source_id,
        },
    )?;
    if mode == "shared" {
        store.provision_database(&id).await?;
    }
    if project.custom {
        println!(
            "Database connection variables saved. Map DATABASE_* values in your custom Compose services before starting the app."
        );
    } else {
        println!("Database configured. Your app can now be started.");
    }
    Ok(())
}

pub async fn database_edit(store: &Store, id: Option<&str>) -> Result<()> {
    prompt::interactive()?;
    let id = source(store, id)?;
    let old = store.database_source(&id)?;
    let name = input("Source name", Some(&old.name))?;
    if old.managed_project.is_some() {
        if prompt::confirm("Save the source name?", false)? {
            store.rename_database_source(&id, &name)?;
        }
        println!(
            "Managed database credentials stay with their database. Editing a source does not rotate a server password."
        );
        return Ok(());
    }
    let mut replacement = old.clone();
    replacement.name = name;
    replacement.host = input("Database hostname", Some(&old.host))?;
    replacement.port = input("Port", Some(&old.port.to_string()))?.parse()?;
    replacement.database = input("Database name", Some(&old.database))?;
    replacement.username = input("Database user", Some(&old.username))?;
    replacement.ssl_mode = tls(&old.ssl_mode)?;
    if old.engine == "mongodb" {
        replacement.auth_database = input("Authentication database", Some(&old.auth_database))?;
    }
    if prompt::confirm(
        "Replace the saved password with a password already set on the database?",
        false,
    )? {
        replacement.password = prompt::secret("Current database password")?;
    }
    prompt::review(
        &json!({"source":replacement.name,"host":replacement.host,"port":replacement.port,"database":replacement.database,"user":replacement.username,"tls":replacement.ssl_mode,"references":store.database_source_references(&id)?}),
    );
    if prompt::confirm("Verify and save these connection settings?", false)? {
        store.edit_database_source(&id, replacement).await?;
        println!("Connection verified and saved. Previous settings are backed up privately.");
    }
    Ok(())
}

pub async fn database_remove(store: &Store, id: Option<&str>) -> Result<()> {
    prompt::interactive()?;
    let id = source(store, id)?;
    let source = store.database_source(&id)?;
    let refs = store.database_source_references(&id)?;
    ensure!(
        refs.is_empty(),
        "This source is still used by {}",
        refs.join(", ")
    );
    prompt::review(
        &json!({"source":source.name,"remove":"Saved connection only","database_and_data":"Preserved","managed_project":source.managed_project,"private_recovery_copy":true}),
    );
    if prompt::confirm("Remove this saved database source?", false)? {
        store.remove_database_source(&id)?;
    }
    Ok(())
}

pub async fn proxy_setup(store: &Store, id: Option<&str>) -> Result<String> {
    prompt::interactive()?;
    let networking = store.networking()?;
    let profiles = store.proxy_profiles()?;
    let selected_id;
    let id = if id == Some("") {
        selected_id = text(
            &select(
                "Proxy connection",
                networking["data"]["proxies"]
                    .as_array()
                    .context("Missing proxies")?,
            )?,
            "id",
        );
        Some(selected_id.as_str())
    } else {
        id
    };
    let mut proxy: Proxy = if let Some(id) = id {
        serde_json::from_value(
            networking["data"]["proxies"]
                .as_array()
                .context("Missing proxies")?
                .iter()
                .find(|p| p["id"] == id)
                .context("Proxy not found")?
                .clone(),
        )?
    } else {
        let choices = profiles.values().collect::<Vec<_>>();
        let profile = choices[prompt::choose(
            "Reverse proxy",
            &choices.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        )?];
        Proxy {
            id: String::new(),
            name: profile.name.clone(),
            provider: profile.id.clone(),
            ssh_alias: String::new(),
            admin_url: String::new(),
            api_token: String::new(),
            ca_certificate: String::new(),
            config_directory: String::new(),
            settings: profile.defaults.clone(),
            read_only: true,
        }
    };
    let profile = &profiles[&proxy.provider];
    proxy.name = input("Connection name", Some(&proxy.name))?;
    println!("{}", profile.description);
    let mut transports = vec![
        "Set up a dedicated SSH key".into(),
        "Use an existing SSH alias".into(),
    ];
    if profile.driver != "file_watch" {
        transports.push("Connect directly to the HTTPS administration API".into());
    }
    proxy.ssh_alias = match prompt::choose("How should Selfhost reach this proxy?", &transports)? {
        0 => crate::ssh_keys::guided_setup(store, crate::ssh_keys::Purpose::Proxy).await?,
        1 => input("SSH alias for proxy host or guest", Some(&proxy.ssh_alias))?,
        _ => String::new(),
    };
    if profile.driver == "file_watch" {
        proxy.config_directory = input(
            "Watched configuration directory on proxy host",
            Some(&proxy.config_directory),
        )?;
    } else {
        proxy.admin_url = input("Proxy administration URL", Some(&proxy.admin_url))?;
        if id.is_none() || prompt::confirm("Replace saved API token?", false)? {
            proxy.api_token = prompt::secret("API token (blank if not needed)")?;
        }
    }
    if proxy.ssh_alias.is_empty() {
        if prompt::confirm(
            "Use a private certificate authority for this administration API?",
            !proxy.ca_certificate.is_empty(),
        )? {
            let path = input("CA certificate PEM file", None)?;
            proxy.ca_certificate = std::fs::read_to_string(path)?;
        } else {
            proxy.ca_certificate.clear();
        }
    } else {
        proxy.ca_certificate.clear();
    }
    // Ask unconditional settings before dependent fields such as ACME email/acceptance.
    for conditional in [false, true] {
        for (key, rule) in &profile.settings_schema {
            if rule.required_when.is_some() != conditional {
                continue;
            }
            if let Some((other, value)) = &rule.required_when
                && proxy.settings.get(other) != Some(value)
            {
                continue;
            }
            let old = proxy.settings.get(key).cloned().unwrap_or(Value::Null);
            let value = match rule.kind.as_str() {
                "accept" => json!(prompt::confirm(&rule.label, old == true)?),
                "paths" | "ca_pool" => {
                    let paths = if rule.kind == "ca_pool" {
                        old["pem_files"].clone()
                    } else {
                        old.clone()
                    };
                    let mut values = Vec::new();
                    for v in paths.as_array().into_iter().flatten() {
                        if prompt::confirm(
                            &format!("Keep {}?", v.as_str().unwrap_or_default()),
                            true,
                        )? {
                            values.push(v.clone());
                        }
                    }
                    while prompt::confirm(&format!("Add a path for {}?", rule.label), false)? {
                        values.push(json!(input("Absolute path on proxy host", None)?));
                    }
                    if rule.kind == "paths" {
                        json!(values)
                    } else if values.is_empty() {
                        Value::Null
                    } else {
                        json!({"provider":"file","pem_files":values})
                    }
                }
                "certificate" => {
                    let default = old
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| old.as_u64().map(|n| n.to_string()))
                        .unwrap_or_else(|| "new".into());
                    let value = input(
                        &format!("{}: new, or existing certificate number", rule.label),
                        Some(&default),
                    )?;
                    if value == "new" {
                        json!(value)
                    } else {
                        json!(
                            value
                                .parse::<u64>()
                                .context("Use new or a certificate number")?
                        )
                    }
                }
                _ => json!(input(&rule.label, old.as_str())?),
            };
            proxy.settings.insert(key.clone(), value);
        }
    }
    proxy.read_only = !prompt::confirm(
        "Allow Selfhost to create reviewed routes on this proxy?",
        !proxy.read_only,
    )?;
    prompt::review(
        &json!({"name":proxy.name,"provider":proxy.provider,"ssh_host":proxy.ssh_alias,"admin_url":proxy.admin_url,"directory":proxy.config_directory,"settings":proxy.settings,"read_only":proxy.read_only}),
    );
    ensure!(
        prompt::confirm("Save this proxy connection?", false)?,
        "Cancelled"
    );
    store.save_proxy(proxy)
}

pub async fn route_setup(store: &Store) -> Result<()> {
    prompt::interactive()?;
    let networking = store.networking()?;
    let proxies = networking["data"]["proxies"]
        .as_array()
        .context("Missing proxies")?;
    let proxy_id = if proxies.is_empty() {
        proxy_setup(store, None).await?
    } else {
        text(&select("Proxy", proxies)?, "id")
    };
    let id = project(store, None)?;
    let project = store.project(&id)?;
    let services = project
        .services
        .iter()
        .map(|s| s.app.clone())
        .collect::<Vec<_>>();
    ensure!(
        !services.is_empty(),
        "This project has no catalog services to route"
    );
    let service = services[prompt::choose("Service", &services)?].clone();
    let domain = input("Public hostname", None)?;
    let upstream = input(
        "Service URL as reached from the proxy (HTTPS for another host)",
        None,
    )?;
    let networks = networking["data"]["networks"]
        .as_array()
        .context("Missing networks")?;
    let network_id =
        if !networks.is_empty() && prompt::confirm("Use a registered private network?", false)? {
            Some(text(&select("Private network", networks)?, "id"))
        } else {
            None
        };
    let route = Route {
        id: format!("route-{}", crate::core::token(6)?),
        proxy_id,
        project_id: id,
        service,
        domain,
        upstream,
        network_id,
    };
    let plan = store.route_plan(&route).await?;
    prompt::review(&plan);
    ensure!(
        plan["can_apply"] == true,
        "This proxy is read-only. Edit it to allow reviewed route creation"
    );
    if prompt::confirm("Create this route?", false)? {
        store
            .route_apply(
                route,
                plan["revision"]
                    .as_str()
                    .context("Missing route revision")?,
            )
            .await?;
        println!("Route created. Verify DNS and HTTPS reachability before sharing it.");
    }
    Ok(())
}

pub async fn network_setup(store: &Store) -> Result<String> {
    prompt::interactive()?;
    let networking = store.networking()?;
    let profiles = networking["network_profiles"]
        .as_array()
        .context("Missing network profiles")?;
    let profile = select("Existing private network", profiles)?;
    println!("{}", profile["guidance"].as_str().unwrap_or_default());
    println!(
        "This connects an existing network record. Peer enrollment and firewall configuration remain with your network provider."
    );
    let name = input("Network name", profile["name"].as_str())?;
    let mut endpoints = vec![];
    loop {
        endpoints.push(input("Peer identity or endpoint", None)?);
        if endpoints.len() >= 2 && !prompt::confirm("Add another peer?", false)? {
            break;
        }
        ensure!(endpoints.len() < 64, "At most 64 peers are supported");
    }
    let policy_reference = input("Access policy or firewall rule reference", None)?;
    let description = input("Description", Some(""))?;
    let network = PrivateNetwork {
        id: String::new(),
        name,
        provider: text(&profile, "id"),
        description,
        endpoints,
        policy_reference,
    };
    prompt::review(&serde_json::to_value(&network)?);
    ensure!(
        prompt::confirm("Register this existing network?", false)?,
        "Cancelled"
    );
    store.add_network(network)
}

pub async fn existing_link(store: &Store) -> Result<()> {
    prompt::interactive()?;
    let profiles = store.existing_profiles()?;
    let rows = profiles
        .values()
        .map(|p| json!({"id":p.id,"name":p.name}))
        .collect::<Vec<_>>();
    let selected = select("App type", &rows)?;
    let profile = text(&selected, "id");
    let name = input("App name", selected["name"].as_str())?;
    let url = input("App URL", None)?;
    let (server_id, container) = if !profiles[&profile].image_repositories.is_empty()
        && prompt::confirm("Discover its container on a Docker host?", true)?
    {
        let host = server(store, false)?;
        let candidates = store.discover_existing(&host, &profile).await?;
        let candidate = select("Running or stopped app container", &candidates)?;
        (host, text(&candidate, "id"))
    } else {
        (String::new(), String::new())
    };
    prompt::review(
        &json!({"name":name,"url":url,"profile":profile,"server":server_id,"container":container,"management_permissions":"None"}),
    );
    if prompt::confirm("Link this existing app?", false)? {
        let app = store
            .link_existing(LinkExisting {
                profile,
                name,
                url,
                server_id,
                container,
            })
            .await?;
        println!(
            "Linked {}. Its containers and configuration are unchanged.",
            app.name
        );
    }
    Ok(())
}

pub async fn existing_reconnect(store: &Store, id: Option<&str>) -> Result<()> {
    prompt::interactive()?;
    let apps = store.existing_apps()?;
    let id = match id {
        Some(id) => id.to_owned(),
        None => text(
            &select(
                "Linked app",
                &apps
                    .iter()
                    .map(|a| json!({"id":a.id,"name":a.name}))
                    .collect::<Vec<_>>(),
            )?,
            "id",
        ),
    };
    let app = apps
        .iter()
        .find(|a| a.id == id)
        .context("Linked app not found")?;
    let host = server(store, false)?;
    let candidate = select(
        "Replacement container",
        &store.discover_existing(&host, &app.profile.id).await?,
    )?;
    let container = text(&candidate, "id");
    let plan = store
        .reconnect_existing_plan(&id, &host, &container)
        .await?;
    prompt::review(&plan);
    if prompt::confirm(
        "Reconnect this app and reset its management permissions?",
        false,
    )? {
        store
            .reconnect_existing(
                &id,
                &host,
                &container,
                plan["revision"].as_str().context("Missing revision")?,
            )
            .await?;
        println!("App reconnected. Review management permissions before enabling actions.");
    }
    Ok(())
}

pub async fn existing_permissions(store: &Store, id: Option<&str>) -> Result<()> {
    prompt::interactive()?;
    let apps = store.existing_apps()?;
    let id = match id {
        Some(id) => id.to_owned(),
        None => text(
            &select(
                "Linked app",
                &apps
                    .iter()
                    .map(|a| json!({"id":a.id,"name":a.name}))
                    .collect::<Vec<_>>(),
            )?,
            "id",
        ),
    };
    let app = apps
        .iter()
        .find(|a| a.id == id)
        .context("Linked app not found")?;
    let mut allowed_actions = Vec::new();
    for action in app.profile.actions.iter().filter(|a| a.write) {
        println!("{}: {}", action.label, action.description);
        if prompt::confirm(
            &format!("Allow {}?", action.label),
            app.allowed_actions.contains(&action.id),
        )? {
            allowed_actions.push(action.id.clone());
        }
    }
    prompt::review(&json!({"app":app.name,"allow_changes":allowed_actions}));
    if prompt::confirm("Save these management permissions?", false)? {
        store.consent_existing(
            &id,
            crate::adoption::ExistingConsent {
                confirmation: app.name.clone(),
                allowed_actions,
            },
        )?;
    }
    Ok(())
}

pub async fn server_setup(store: &Store) -> Result<()> {
    prompt::interactive()?;
    let provider = [
        Provider::DockerSsh,
        Provider::DockerContext,
        Provider::ProxmoxSsh,
    ][prompt::choose(
        "Server connection",
        &[
            "Docker host through SSH".into(),
            "Existing Docker context".into(),
            "Proxmox host through SSH".into(),
        ],
    )?]
    .clone();
    let name = input("Server name", None)?;
    let endpoint = if provider == Provider::DockerContext {
        input("Docker context name", None)?
    } else {
        match prompt::choose(
            "SSH access",
            &[
                "Set up a dedicated SSH key".into(),
                "Use an existing SSH alias".into(),
            ],
        )? {
            0 => {
                crate::ssh_keys::guided_setup(
                    store,
                    if provider == Provider::ProxmoxSsh {
                        crate::ssh_keys::Purpose::Proxmox
                    } else {
                        crate::ssh_keys::Purpose::Docker
                    },
                )
                .await?
            }
            _ => input("SSH alias for the host or guest", None)?,
        }
    };
    let read_only = !prompt::confirm("Allow reviewed changes to services on this server?", false)?;
    let app_host = input("Browser-reachable app hostname (optional)", Some(""))?;
    let group = input("Server group (optional)", Some(""))?;
    prompt::review(
        &json!({"name":name,"provider":provider,"connection":endpoint,"read_only":read_only,"app_host":app_host,"group":group}),
    );
    if prompt::confirm("Save and test this server connection?", false)? {
        let server = store.add_server(crate::infrastructure::Server {
            id: String::new(),
            name,
            provider,
            endpoint,
            group,
            read_only,
            app_host,
        })?;
        store.infrastructure_inventory(&server.id).await.context(format!("Connection saved as {}. Check SSH host trust and access, then inspect this host again", server.id))?;
        println!("Server connected: {}", server.name);
    }
    Ok(())
}
