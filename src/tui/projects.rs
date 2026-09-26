use super::*;

pub(super) async fn create(store: &Store) -> Result<()> {
    let create_kind = take!(menu(
        "Create",
        &[
            "Choose an app and deployment method",
            "Create a multi-app project",
            "Custom Compose setup",
            "Host an identity provider",
            "Choose a stack template"
        ]
    ));
    match create_kind {
        0 => {
            let index = take!(select(
                "Choose an app",
                &labels(&store.catalog, |a| format!("{}: {}", a.name, a.description))
            ));
            let app = &store.catalog[index];
            let methods = store.deployments()?;
            let mut options: Vec<_> = methods
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["app"] == app.id)
                .collect();
            options.sort_by_key(|p| !p["default"].as_bool().unwrap_or(false));
            if options.is_empty() {
                let name = take!(required("Project name", &app.name));
                let server = take!(server_pick(store, false, true));
                show(
                    "Created stopped project",
                    &project_summary(&store.create(CreateProject {
                        name,
                        server_id: server.id,
                        apps: vec![app.id.clone()],
                    })?),
                )?;
            } else {
                let choice = take!(select(
                    "Deployment method",
                    &labels(&options, |p| format!(
                        "{}: {}",
                        p["name"].as_str().unwrap_or(""),
                        p["description"].as_str().unwrap_or("")
                    ))
                ));
                let method = options[choice];
                let name = take!(required("Project name", &app.name));
                let server = take!(server_pick(store, false, true));
                let mut inputs = BTreeMap::new();
                let mut acknowledgements = vec![];
                if let Some(fields) = method["configuration"]["inputs"].as_object() {
                    for (key, input) in fields {
                        inputs.insert(
                            key.clone(),
                            take!(dynamic_input(
                                input["label"].as_str().unwrap_or(key),
                                input["kind"].as_str().unwrap_or("text"),
                                &input["default"],
                                &[]
                            )),
                        );
                    }
                }
                if let Some(requirements) = method["configuration"]["requirements"].as_array() {
                    for requirement in requirements {
                        if !yes(requirement["label"]
                            .as_str()
                            .unwrap_or("Accept requirement"))?
                        {
                            return Ok(());
                        }
                        acknowledgements.push(requirement["id"].as_str().unwrap_or("").into());
                    }
                }
                let (project, instructions) =
                    store.create_deployment(crate::deployments::CreateDeployment {
                        version: None,
                        app: app.id.clone(),
                        method: method["id"].as_str().unwrap_or("").into(),
                        name,
                        server_id: server.id,
                        inputs,
                        acknowledgements,
                    })?;
                show(
                    "Created stopped deployment",
                    &json!({"project":project_summary(&project),"next_steps":instructions}),
                )?;
            }
        }
        1 => {
            let simple: Vec<_> = store
                .catalog
                .iter()
                .filter(|app| !app.deployment_only)
                .collect();
            let choices = take!(multi(
                "Apps for this project",
                &labels(&simple, |a| a.name.clone()),
                &[]
            ));
            ensure!(!choices.is_empty(), "Choose at least one app");
            let name = take!(required("Project name", "My services"));
            let server = take!(server_pick(store, false, true));
            show(
                "Created stopped project",
                &project_summary(&store.create(CreateProject {
                    name,
                    server_id: server.id,
                    apps: choices.iter().map(|i| simple[*i].id.clone()).collect(),
                })?),
            )?;
        }
        2 => {
            let name = take!(required("Project name", "Custom setup"));
            let server = take!(server_pick(store, false, true));
            let compose = take!(editor(
                "Compose YAML or JSON",
                "services:\n  app:\n    image: nginx:stable-alpine\n    ports:\n      - 127.0.0.1:8080:80\n"
            ));
            let setup = crate::setup::Setup {
                compose: crate::setup::parse_compose(&compose)?,
                environment: BTreeMap::new(),
                files: BTreeMap::new(),
                database: None,
            };
            let project = store.create_setup(&name, &server.id, setup)?;
            show("Created stopped setup", &project_summary(&project))?;
            setup_editor(store, &project.id)?;
        }
        _ => {
            let blueprints: Vec<_> = store
                .blueprints()?
                .into_values()
                .filter(|b| create_kind != 3 || b.category == "Identity")
                .collect();
            let choice = take!(select(
                "Choose a stack template",
                &labels(&blueprints, |b| format!("{}: {}", b.name, b.description))
            ));
            let blueprint = &blueprints[choice];
            let name = take!(required("Project name", &blueprint.name));
            let server = take!(server_pick(store, false, true));
            let mut inputs = BTreeMap::new();
            for (key, input) in &blueprint.inputs {
                inputs.insert(
                    key.clone(),
                    take!(dynamic_input(
                        &input.label,
                        &input.kind,
                        &input.default,
                        &[]
                    )),
                );
            }
            let mut acknowledgements = vec![];
            for r in &blueprint.requirements {
                if !yes(&r.label)? {
                    return Ok(());
                }
                acknowledgements.push(r.id.clone());
            }
            let (project, instructions) = store.install_blueprint(crate::stacks::Install {
                blueprint: blueprint.id.clone(),
                name,
                server_id: server.id,
                inputs,
                acknowledgements,
            })?;
            show(
                "Created stopped stack",
                &json!({"project":project_summary(&project),"next_steps":instructions}),
            )?;
        }
    }
    Ok(())
}
pub(super) async fn browse(store: &Store) -> Result<()> {
    let project = take!(project_pick(store));
    let id = &project.id;
    loop {
        let project = store.project(id)?;
        let choice = take!(menu(
            &project.name,
            &[
                "Services: status, logs, actions and native settings",
                "Start services",
                "Stop services",
                "Restart services",
                "Refresh images",
                "Save configuration snapshot",
                "Project settings",
                "Environment, files and Compose",
                "Database settings and backups",
                "Move or assign to another server",
                "Export standalone configuration",
                "Review generated Compose plan",
                "Remove or archive project"
            ]
        ));
        match choice {
            0 => Box::pin(services(store, &project)).await?,
            1..=4 => {
                let action = ["start", "stop", "restart", "refresh"][choice - 1];
                if yes(&format!("{action} {}?", project.name))? {
                    wait(action, store.action(id, action)).await?;
                    view(
                        "Completed",
                        &format!("{action} completed for {}", project.name),
                    )?;
                }
            }
            5 => {
                wait(
                    "Saving configuration snapshot",
                    store.action(id, "snapshot"),
                )
                .await?;
                view(
                    "Snapshot saved",
                    "Configuration was saved. Application volume data is excluded.",
                )?;
            }
            6 => settings(store, &project)?,
            7 => setup_editor(store, id)?,
            8 => Box::pin(super::administration::project_database(store, &project)).await?,
            9 => Box::pin(super::administration::project_move(store, &project)).await?,
            10 => {
                let path = take!(required("New export archive path", "selfhost-export.tar"));
                if yes("Export includes passwords and config files. Save privately?")? {
                    store.export_to(id, Path::new(&path))?;
                    view(
                        "Export complete",
                        "Compose, environment and files were exported. Application data is not included.",
                    )?;
                }
            }
            11 => {
                if yes("The Compose plan can include credentials. Open it privately?")? {
                    show("Generated Compose plan", &store.render(id)?)?;
                }
            }
            _ => {
                super::administration::remove(store, &project, None).await?;
                if store.project(id).is_err() {
                    return Ok(());
                }
            }
        }
    }
}
fn settings(store: &Store, project: &Project) -> Result<()> {
    if project.custom {
        return setup_editor(store, &project.id);
    }
    let name = take!(required("Project name", &project.name));
    let access = take!(menu(
        "Network access",
        &[
            "This host only (loopback)",
            "Local network (all interfaces)"
        ]
    ));
    let mut services = vec![];
    for service in &project.services {
        let image = take!(required(
            &format!("{} image", service.definition.name),
            &service.image
        ));
        let port = take!(number(
            &format!("{} host port", service.definition.name),
            service.port as u64
        ));
        services.push(EditService {
            app: service.app.clone(),
            image,
            port: port.try_into().context("Port must fit 16 bits")?,
        });
    }
    if yes("Save project settings? Restart may be needed to apply them.")? {
        show(
            "Saved settings",
            &project_summary(&store.edit(
                &project.id,
                EditProject {
                    name,
                    access: if access == 0 { "local" } else { "lan" }.into(),
                    services,
                },
            )?),
        )?;
    }
    Ok(())
}
pub(super) fn setup_editor(store: &Store, id: &str) -> Result<()> {
    loop {
        let mut setup = store.setup(id)?;
        match take!(menu(
            "Configuration",
            &[
                "Summary (secret values hidden)",
                "Environment variables",
                "Mounted configuration files",
                "Advanced Compose editor",
                "Advanced complete setup editor"
            ]
        )) {
            0 => show(
                "Setup summary",
                &json!({"environment":setup.environment.keys().collect::<Vec<_>>(),"files":setup.files.keys().collect::<Vec<_>>(),"database":setup.database,"services":setup.compose["services"].as_object().map(|s|s.keys().collect::<Vec<_>>())}),
            )?,
            1 => {
                let mut keys = vec!["Add an environment variable".into()];
                keys.extend(setup.environment.keys().cloned());
                let choice = take!(select("Environment variables", &keys));
                let key = if choice == 0 {
                    take!(required("Variable name", ""))
                } else {
                    keys[choice].clone()
                };
                let action = take!(menu(&key, &["Set value (hidden input)", "Remove variable"]));
                if action == 0 {
                    let value = take!(prompt(
                        &key,
                        "Stored privately; included in project exports",
                        setup
                            .environment
                            .get(&key)
                            .map(String::as_str)
                            .unwrap_or(""),
                        true
                    ));
                    setup.environment.insert(key, value);
                } else if yes(&format!("Remove {key}?"))? {
                    setup.environment.remove(&key);
                } else {
                    continue;
                }
                store.save_setup(id, setup)?;
                view(
                    "Saved",
                    "Environment updated. Start or recreate the app to apply it.",
                )?;
            }
            2 => {
                let mut names = vec!["Add a config file".into()];
                names.extend(setup.files.keys().cloned());
                let choice = take!(select("Mounted config files", &names));
                let name = if choice == 0 {
                    take!(required("Filename (no directory separators)", "app.conf"))
                } else {
                    names[choice].clone()
                };
                match take!(menu(&name, &["Edit content", "Remove file"])) {
                    0 => {
                        let text = take!(editor(
                            &name,
                            setup.files.get(&name).map(String::as_str).unwrap_or("")
                        ));
                        setup.files.insert(name, text);
                    }
                    _ => {
                        if !yes("Remove this file? Remove mounted references in Compose first.")? {
                            continue;
                        }
                        setup.files.remove(&name);
                    }
                }
                store.save_setup(id, setup)?;
                view(
                    "Saved",
                    "File updated. Review mounted references and restart requirements.",
                )?;
            }
            3 => {
                if !yes("Compose may include inline secrets. Open it?")? {
                    continue;
                }
                let compose = take!(editor(
                    "Compose YAML or JSON",
                    &serde_json::to_string_pretty(&setup.compose)?
                ));
                setup.compose = crate::setup::parse_compose(&compose)?;
                if yes(
                    "Save desired Compose configuration? Runtime changes when you start/update.",
                )? {
                    store.save_setup(id, setup)?;
                }
            }
            _ => {
                if !yes("Complete setup includes secrets. Open advanced editor?")? {
                    continue;
                }
                let edited = take!(json_edit("Complete setup JSON", &setup));
                if yes("Save complete setup?")? {
                    store.save_setup(id, edited)?;
                }
            }
        }
    }
}
async fn services(store: &Store, project: &Project) -> Result<()> {
    let index = take!(select(
        "Choose a service",
        &labels(&project.services, |s| s.definition.name.clone())
    ));
    let service = &project.services[index];
    let id = &project.id;
    let app = &service.app;
    loop {
        match take!(menu(
            &service.definition.name,
            &[
                "Resource usage",
                "Logs (last 200 lines)",
                "Lifecycle and recipe actions",
                "Native app settings",
                "Native app workflows",
                "Native configuration backups",
                "Connect an identity provider to this app",
                "Add apps to a dashboard",
                "Automatic app setup",
                "Choose app version",
                "Remove this service"
            ]
        )) {
            0 => {
                let out = wait("Reading service usage", store.service_stats(id, app)).await?;
                show("Service usage", &out)?;
            }
            1 => {
                let out = wait("Reading service logs", store.service_logs(id, app)).await?;
                view("Service logs (may contain private information)", &out)?;
            }
            2 => {
                let mut actions = vec![
                    "start".into(),
                    "stop".into(),
                    "restart".into(),
                    "update".into(),
                ];
                actions.extend(service.definition.actions.iter().map(|a| a.id.clone()));
                let choice = take!(select("Service action", &actions));
                if confirm_name(
                    &format!("Run {} on this service?", actions[choice]),
                    &service.definition.name,
                )? {
                    let out = wait(
                        "Running service action",
                        store.service_action(id, app, &actions[choice]),
                    )
                    .await?;
                    view("Action output", &out)?;
                }
            }
            3 => Box::pin(native_settings(store, id, app)).await?,
            4 => Box::pin(native_action(store, id, app)).await?,
            5 => Box::pin(native_backups(store, id, app)).await?,
            6 => Box::pin(app_identity(store, id, app)).await?,
            7 => {
                let dashboards: Vec<_> = project
                    .services
                    .iter()
                    .filter(|service| service.definition.dashboard.is_some())
                    .collect();
                let target = take!(select(
                    "Dashboard service",
                    &labels(&dashboards, |service| service.definition.name.clone())
                ));
                let key = take!(prompt("Dashboard API credential", "Hidden input", "", true));
                if yes("Create dashboard entries for this project's apps?")? {
                    let count = wait(
                        "Syncing dashboard",
                        store.dashboard_sync(id, &dashboards[target].app, key),
                    )
                    .await?;
                    show("Dashboard entries added", &count)?;
                }
            }
            8 => Box::pin(app_onboarding(store, id, app)).await?,
            9 => Box::pin(super::versions::manage(store, id, app)).await?,
            _ => {
                super::administration::remove(store, project, Some(app.clone())).await?;
                return Ok(());
            }
        }
    }
}
async fn app_onboarding(store: &Store, id: &str, app: &str) -> Result<()> {
    let info = store.onboarding_info(id, app)?;
    show("Automatic app setup", &info)?;
    if info["supported"] != true {
        return Ok(());
    }
    if take!(menu(
        "Automatic app setup",
        &[
            "View setup details only",
            "Review and apply a setup request"
        ]
    )) == 0
    {
        return Ok(());
    }
    let file = take!(required(
        "Setup request JSON path (use env:VARIABLE for secrets)",
        ""
    ));
    let request = crate::onboarding::read_request(std::path::Path::new(&file))?;
    let plan = wait(
        "Reviewing app setup",
        store.onboarding_plan(id, app, request.clone()),
    )
    .await?;
    if review(
        "Initialize app and create the reviewed dashboard links",
        &plan,
    )? {
        let result = wait(
            "Applying app setup",
            store.onboarding_apply(id, app, request, revision(&plan)?),
        )
        .await?;
        show("App setup result", &result)?;
    }
    Ok(())
}
async fn native_settings(store: &Store, id: &str, app: &str) -> Result<()> {
    let state = wait(
        "Reading native app configuration",
        store.integration_state(id, app),
    )
    .await?;
    let profile = store.integration(id, app)?;
    let mode = take!(menu(
        "Native app settings",
        &[
            "View current settings",
            "Edit common settings",
            "Edit all settings (advanced)"
        ]
    ));
    if mode == 0 {
        return show("Current native settings", &state["values"]);
    }
    let fields: Vec<_> = profile
        .fields
        .iter()
        .filter(|f| mode == 2 || !f.advanced)
        .collect();
    let selected = take!(multi(
        "Choose fields to change",
        &labels(&fields, |f| format!("{} ({})", f.label, f.kind)),
        &[]
    ));
    let mut changes = BTreeMap::new();
    for index in selected {
        let field = fields[index];
        let value = take!(dynamic_input(
            &field.label,
            &field.kind,
            state["values"].get(&field.id).unwrap_or(&Value::Null),
            &field.choices
        ));
        changes.insert(field.id.clone(), value);
    }
    if changes.is_empty() {
        return Ok(());
    }
    let plan = wait(
        "Previewing native settings",
        store.integration_plan(id, app, &changes),
    )
    .await?;
    if review("Native configuration change", &plan)? {
        let out = wait(
            "Applying native settings",
            store.integration_apply(id, app, changes, revision(&plan)?),
        )
        .await?;
        show("Applied native settings", &out)?;
    }
    Ok(())
}
async fn native_action(store: &Store, id: &str, app: &str) -> Result<()> {
    let profile = store.integration(id, app)?;
    let index = take!(select(
        "App workflows",
        &labels(&profile.actions, |a| format!(
            "{}: {}",
            a.label, a.description
        ))
    ));
    let action = &profile.actions[index];
    let mut inputs = BTreeMap::new();
    for field in &action.inputs {
        inputs.insert(
            field.id.clone(),
            take!(dynamic_input(
                &field.label,
                &field.kind,
                field.default.as_ref().unwrap_or(&Value::Null),
                &field.choices
            )),
        );
    }
    if confirm_name(
        &format!("Run {}? Review backup requirements first.", action.label),
        &profile.name,
    )? {
        let out = wait(
            &action.label,
            store.integration_action(id, app, &action.id, inputs),
        )
        .await?;
        show("Workflow output", &out)?;
    }
    Ok(())
}
async fn native_backups(store: &Store, id: &str, app: &str) -> Result<()> {
    let backups = store.integration_backups(id, app)?;
    let index = take!(select("Native configuration backups", &backups));
    let plan = wait(
        "Previewing configuration restore",
        store.integration_restore(id, app, &backups[index], None),
    )
    .await?;
    if review("Restore native settings", &plan)? {
        let out = wait(
            "Restoring native settings",
            store.integration_restore(id, app, &backups[index], Some(revision(&plan)?)),
        )
        .await?;
        show("Restored settings", &out)?;
    }
    Ok(())
}
async fn app_identity(store: &Store, id: &str, app: &str) -> Result<()> {
    match take!(menu(
        "App identity connection",
        &[
            "View connection progress",
            "Create a provider client and configure app",
            "Resume saved connection"
        ]
    )) {
        0 => show("Connection progress", &store.connection_state(id, app)?)?,
        2 => {
            if yes("Resume configuring the saved app client?")? {
                let out = wait("Resuming connection", store.connection_resume(id, app)).await?;
                show("Connection progress", &out)?;
            }
        }
        _ => {
            let providers: Vec<_> = store.identity_providers()?.into_iter().collect();
            let index = take!(select(
                "Identity provider",
                &labels(&providers, |(id, p)| format!("{} ({id})", p.name))
            ));
            let issuer = take!(required(
                "Issuer origin (HTTPS or localhost)",
                "https://identity.example.com"
            ));
            let supports_automatic =
                crate::identity_setup::registration_profile(&providers[index].0)
                    .is_ok_and(|profile| profile.project_creation.is_some());
            let create_project = supports_automatic
                && take!(menu(
                    "Provider project",
                    &[
                        "Create a dedicated Selfhost project",
                        "Use an existing project"
                    ]
                )) == 0;
            let provider_project = if create_project {
                String::new()
            } else {
                take!(required("Existing provider project ID", ""))
            };
            let app_url = take!(required(
                "App origin (HTTPS or localhost)",
                "https://app.example.com"
            ));
            let name = take!(required("Client name", app));
            let mut request = crate::connections::ConnectRequest {
                provider: providers[index].0.clone(),
                issuer,
                provider_project,
                create_project,
                project_name: "Selfhost".into(),
                organization_id: String::new(),
                administrator_subject: String::new(),
                replace_existing: false,
                ca_certificate: String::new(),
                app_url,
                name,
                credential: String::new(),
                revision: String::new(),
            };
            request.credential = take!(prompt(
                "Temporary provider API credential",
                "Hidden input",
                "",
                true
            ));
            if supports_automatic {
                let account = wait(
                    "Looking up provider account",
                    store.connection_account(id, app, &request),
                )
                .await?;
                if account["human"].as_bool() == Some(true)
                    && !account["administrator_role"]
                        .as_str()
                        .unwrap_or("")
                        .is_empty()
                {
                    show("Provider account", &account)?;
                    if yes("Use this verified account as the app administrator?")? {
                        request.administrator_subject =
                            account["suggested_subject"].as_str().unwrap_or("").into();
                    }
                }
            }
            let plan = store.connection_plan(id, app, &request)?;
            if review("Create app identity connection", &plan)? {
                request.revision = revision(&plan)?.into();
                let out = wait(
                    "Creating app identity connection",
                    store.connection_create(id, app, request),
                )
                .await?;
                show("Connection result", &out)?;
            }
        }
    }
    Ok(())
}
