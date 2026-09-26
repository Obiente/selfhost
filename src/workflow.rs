//! Human-oriented entry points over the same reviewed operations used by the API.
use crate::{
    core::{Project, Store},
    guided as g,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub fn project(store: &Store) -> Result<Project> {
    let projects = store.read()?.projects;
    ensure!(
        !projects.is_empty(),
        "Create a project first with selfhost deploy"
    );
    let labels = projects
        .iter()
        .map(|p| format!("{} ({})", p.name, p.id))
        .collect::<Vec<_>>();
    Ok(projects[g::choose("Choose a project", &labels)?].clone())
}
pub fn service(project: &Project) -> Result<String> {
    let labels = project
        .services
        .iter()
        .map(|s| s.definition.name.clone())
        .collect::<Vec<_>>();
    Ok(project.services[g::choose("Choose an app", &labels)?]
        .app
        .clone())
}
pub async fn guide(store: &Store) -> Result<()> {
    g::interactive()?;
    let choices = [
        "Install an app",
        "Set up an app",
        "Connect an app to an identity provider",
        "Change app settings",
        "Update an app",
        "Connect an existing app",
        "Connect a server",
        "Set up a database",
        "Connect a reverse proxy",
        "Publish a service through a proxy",
        "Connect a private network",
        "Create a recurring task",
    ];
    let choice = g::choose("What would you like to do?", &choices.map(String::from))?;
    match choice {
        0 => deploy(store).await?,
        1..=4 => {
            let p = project(store)?;
            if choice == 4 {
                crate::app_assist::update(store, &p.id).await?;
            } else {
                let s = service(&p)?;
                match choice {
                    1 => crate::app_assist::setup(store, &p.id, &s).await?,
                    2 => crate::app_assist::connect(store, &p.id, &s).await?,
                    _ => crate::app_assist::settings(store, &p.id, &s).await?,
                }
            }
        }
        5 => crate::infrastructure_assist::existing_link(store).await?,
        6 => crate::infrastructure_assist::server_setup(store).await?,
        7 => crate::infrastructure_assist::database_setup(store, None).await?,
        8 => {
            crate::infrastructure_assist::proxy_setup(store, None).await?;
        }
        9 => crate::infrastructure_assist::route_setup(store).await?,
        10 => {
            crate::infrastructure_assist::network_setup(store).await?;
        }
        11 => task_create(store).await?,
        _ => unreachable!(),
    }
    Ok(())
}

pub async fn task_create(store: &Store) -> Result<()> {
    use crate::tasks::{Destination, TaskKind, TaskRequest};
    g::interactive()?;
    let kind = g::choose(
        "Recurring task",
        &[
            "Keep dashboard service links current".into(),
            "Run an app action".into(),
        ],
    )?;
    let destinations = store.task_destinations()?;
    let options = destinations
        .as_array()
        .context("Missing destinations")?
        .iter()
        .filter(|d| {
            if kind == 0 {
                d["supports_sync"] == true && d["configured"] == true
            } else {
                d["actions"].as_array().is_some_and(|v| !v.is_empty())
            }
        })
        .collect::<Vec<_>>();
    ensure!(
        !options.is_empty(),
        "No ready destination supports this task. Complete app setup or add an app with recurring actions first."
    );
    let labels = options
        .iter()
        .map(|d| d["name"].as_str().unwrap_or("App").to_owned())
        .collect::<Vec<_>>();
    let destination = options[g::choose("Choose the destination", &labels)?];
    let mut request = TaskRequest {
        name: g::input(
            "Task name",
            if kind == 0 {
                "Keep service links current"
            } else {
                "App maintenance"
            },
        )?,
        kind: if kind == 0 {
            TaskKind::DashboardLinks
        } else {
            TaskKind::ServiceAction
        },
        destination: Destination {
            project_id: destination["project_id"]
                .as_str()
                .context("Missing project")?
                .into(),
            service: destination["service"]
                .as_str()
                .context("Missing service")?
                .into(),
        },
        managed: false,
        existing: false,
        project_ids: vec![],
        existing_ids: vec![],
        interval_seconds: 300,
        action: String::new(),
    };
    if kind == 0 {
        let scope = g::choose(
            "Which services should be included?",
            &[
                "All current and future projects and linked apps".into(),
                "Choose specific projects and linked apps".into(),
            ],
        )?;
        if scope == 0 {
            request.managed = true;
            request.existing = true;
        } else {
            for p in store.read()?.projects {
                if g::confirm(&format!("Include project {}?", p.name), false)? {
                    request.project_ids.push(p.id);
                }
            }
            for a in store.existing_apps()? {
                if g::confirm(&format!("Include linked app {}?", a.name), false)? {
                    request.existing_ids.push(a.id);
                }
            }
            request.managed = !request.project_ids.is_empty();
            request.existing = !request.existing_ids.is_empty();
            ensure!(
                request.managed || request.existing,
                "Choose at least one source"
            );
        }
    } else {
        let actions = destination["actions"]
            .as_array()
            .context("Missing actions")?;
        let labels = actions
            .iter()
            .map(|a| a["label"].as_str().unwrap_or("Action").to_owned())
            .collect::<Vec<_>>();
        request.action = actions[g::choose("Choose an action", &labels)?]["id"]
            .as_str()
            .context("Missing action")?
            .into();
    }
    loop {
        let value = g::input("Run every how many minutes?", "5")?;
        if let Ok(minutes) = value.parse::<u64>()
            && (1..=43200).contains(&minutes)
        {
            request.interval_seconds = minutes * 60;
            break;
        }
        println!("Enter a whole number between 1 and 43200.");
    }
    let plan = store.task_plan(request.clone()).await?;
    g::review(&plan);
    if !g::confirm("Enable this recurring task?", false)? {
        println!("Cancelled. No task created.");
        return Ok(());
    }
    let value = store
        .task_create(
            request,
            plan["revision"].as_str().context("Missing task review")?,
        )
        .await?;
    println!(
        "Task created: {}. It runs while the dashboard is running, or through selfhost task tick.",
        value["id"].as_str().unwrap_or("")
    );
    Ok(())
}

pub async fn deploy(store: &Store) -> Result<()> {
    g::interactive()?;
    let deployments = store.deployments()?;
    let mut options = deployments
        .as_array()
        .context("Invalid deployment catalogue")?
        .clone();
    for recipe in &store.catalog {
        if !options.iter().any(|d| d["app"] == recipe.id) {
            options.push(json!({"app":recipe.id,"id":"docker","name":"Docker Compose","description":recipe.description,"recipe":recipe.id,"default_recipe":true}));
        }
    }
    let mut names = std::collections::BTreeMap::new();
    for d in &options {
        names
            .entry(d["app"].as_str().context("Missing app")?.to_owned())
            .or_insert_with(|| d["app"].as_str().unwrap().to_owned());
    }
    let apps = names.keys().cloned().collect::<Vec<_>>();
    let app = &apps[g::choose("Choose an app", &apps)?];
    let methods = options
        .iter()
        .filter(|d| d["app"] == *app)
        .collect::<Vec<_>>();
    let labels = methods
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                m["name"].as_str().unwrap_or("Method"),
                m["description"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    let selected = methods[g::choose("How should this app run?", &labels)?];
    let name = g::input("Project name", app)?;
    let server = server(store)?;
    let mut inputs = std::collections::BTreeMap::new();
    let mut acknowledgements = vec![];
    if let Some(blueprint) = selected["blueprint"].as_str() {
        let blueprints = store.blueprints()?;
        let profile = blueprints
            .get(blueprint)
            .context("Missing deployment inputs")?;
        inputs = stack_inputs(profile)?;
        acknowledgements = requirements(profile)?;
    }
    g::review(
        &json!({"app":app,"method":selected["name"],"project":name,"server":server.name,"inputs":inputs}),
    );
    if !g::confirm("Create this project?", false)? {
        println!("Cancelled. No project created.");
        return Ok(());
    }
    let (project, instructions) = if selected["default_recipe"] == true {
        (
            store.create(crate::core::CreateProject {
                server_id: server.id,
                name,
                apps: vec![app.clone()],
            })?,
            vec![],
        )
    } else {
        store.create_deployment(crate::deployments::CreateDeployment {
            version: None,
            app: app.clone(),
            method: selected["id"].as_str().context("Missing method")?.into(),
            name,
            server_id: server.id,
            inputs,
            acknowledgements,
        })?
    };
    finish_install(store, &project, instructions).await
}
pub fn server(store: &Store) -> Result<crate::infrastructure::Server> {
    let remote_only = matches!(
        store.local_docker_mode()?,
        crate::dashboard::LocalDockerMode::Disabled
    );
    let servers = store
        .servers()?
        .into_iter()
        .filter(|s| {
            !s.read_only
                && s.provider != crate::infrastructure::Provider::ProxmoxSsh
                && !(remote_only && s.id == "local")
        })
        .collect::<Vec<_>>();
    let labels = servers.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    Ok(servers[g::choose("Where should it run?", &labels)?].clone())
}
pub fn stack_inputs(
    profile: &crate::stacks::Blueprint,
) -> Result<std::collections::BTreeMap<String, Value>> {
    let mut values = std::collections::BTreeMap::new();
    for (id, field) in &profile.inputs {
        loop {
            let default = field
                .default
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| field.default.to_string());
            let value = g::input(&field.label, &default)?;
            let parsed = match field.kind.as_str() {
                "port" => value
                    .parse::<u16>()
                    .ok()
                    .filter(|v| *v >= 1024)
                    .map(|v| json!(v)),
                "hostname" => (value == "localhost" || crate::networking::hostname(&value))
                    .then(|| json!(value)),
                "identifier" => crate::setup::slug(&value).then(|| json!(value)),
                "email" => (value.contains('@')
                    && value.len() < 254
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b)))
                .then(|| json!(value)),
                _ => anyhow::bail!("Unsupported input type for {}", field.label),
            };
            if let Some(value) = parsed {
                values.insert(id.clone(), value);
                break;
            }
            println!("Enter a valid {}.", field.kind);
        }
    }
    Ok(values)
}
pub fn requirements(profile: &crate::stacks::Blueprint) -> Result<Vec<String>> {
    let mut accepted = vec![];
    for requirement in &profile.requirements {
        ensure!(
            g::confirm(&format!("{} Continue?", requirement.label), false)?,
            "Cancelled before creating resources"
        );
        accepted.push(requirement.id.clone());
    }
    Ok(accepted)
}
pub async fn stack(store: &Store) -> Result<()> {
    g::interactive()?;
    let blueprints = store.blueprints()?;
    let profiles = blueprints.values().collect::<Vec<_>>();
    let labels = profiles
        .iter()
        .map(|p| format!("{}: {}", p.name, p.description))
        .collect::<Vec<_>>();
    let profile = profiles[g::choose("Choose a stack", &labels)?];
    let name = g::input("Project name", &profile.name)?;
    let server = server(store)?;
    let inputs = stack_inputs(profile)?;
    let acknowledgements = requirements(profile)?;
    g::review(&json!({"stack":profile.name,"project":name,"server":server.name,"inputs":inputs}));
    if !g::confirm("Create this stack?", false)? {
        println!("Cancelled. No project created.");
        return Ok(());
    }
    let (project, instructions) = store.install_blueprint(crate::stacks::Install {
        blueprint: profile.id.clone(),
        name,
        server_id: server.id,
        inputs,
        acknowledgements,
    })?;
    finish_install(store, &project, instructions).await
}
async fn finish_install(store: &Store, project: &Project, instructions: Vec<String>) -> Result<()> {
    println!("Created {}.", project.name);
    for instruction in instructions {
        println!("{instruction}");
    }
    if project
        .services
        .iter()
        .any(|s| s.definition.database.is_some())
        && g::confirm(
            "Choose a dedicated, shared or existing database before the first start?",
            false,
        )?
    {
        crate::infrastructure_assist::database_setup(store, Some(&project.id)).await?;
    }
    if g::confirm("Start the app now?", false)? {
        store.action(&project.id, "start").await?;
        println!("Started {}.", project.name);
        for service in &project.services {
            if service.definition.onboarding.is_some()
                && g::confirm(
                    &format!("Complete {} setup now?", service.definition.name),
                    true,
                )?
            {
                crate::app_assist::setup(store, &project.id, &service.app).await?;
            }
            let info = store.integration(&project.id, &service.app);
            if info.as_ref().is_ok_and(|value| value.oidc_client.is_some())
                && g::confirm(
                    &format!(
                        "Connect {} to an identity provider?",
                        service.definition.name
                    ),
                    false,
                )?
            {
                crate::app_assist::connect(store, &project.id, &service.app).await?;
            }
        }
    }
    Ok(())
}
