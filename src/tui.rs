//! Keyboard-first workspace using the same validated Store APIs as the dashboard.
mod administration;
mod projects;
mod tasks;
mod updates;
mod versions;
mod widgets;
use crate::{
    core::{CreateProject, EditProject, EditService, Project, Schedule, Store},
    infrastructure::{Provider, Server},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use widgets::*;
macro_rules! take {
    ($value:expr) => {
        match $value? {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}
pub(crate) use take;
fn labels<T>(items: &[T], label: impl Fn(&T) -> String) -> Vec<String> {
    items.iter().map(label).collect()
}
fn project_pick(store: &Store) -> Result<Option<Project>> {
    let projects = store.read()?.projects;
    Ok(select(
        "Projects",
        &labels(&projects, |p| {
            format!("{}  [{}]  {} apps", p.name, p.id, p.services.len())
        }),
    )?
    .map(|i| projects[i].clone()))
}
fn server_pick(store: &Store, writable: bool, docker: bool) -> Result<Option<Server>> {
    let servers: Vec<_> = store
        .servers()?
        .into_iter()
        .filter(|s| (!writable || !s.read_only) && (!docker || s.provider != Provider::ProxmoxSsh))
        .collect();
    Ok(select(
        "Choose a server",
        &labels(&servers, |s| {
            format!(
                "{}{}",
                s.name,
                if s.read_only { " [read-only]" } else { "" }
            )
        }),
    )?
    .map(|i| servers[i].clone()))
}
fn field(title: &str, default: &str) -> Result<Option<String>> {
    prompt(title, "", default, false)
}
fn revision(value: &Value) -> Result<&str> {
    value["revision"]
        .as_str()
        .context("The backend did not return a plan revision")
}
fn project_summary(project: &Project) -> Value {
    json!({"id":project.id,"name":project.name,"server":project.server_id,"access":project.access,"apps":project.services.iter().map(|s|json!({"id":s.app,"name":s.definition.name,"image":s.image,"host_port":s.port})).collect::<Vec<_>>()})
}
fn dynamic_input(
    label: &str,
    kind: &str,
    current: &Value,
    choices: &[String],
) -> Result<Option<Value>> {
    if !choices.is_empty() {
        return Ok(select(label, choices)?.map(|i| json!(choices[i])));
    }
    match kind {
        "bool" | "boolean" | "accept" => {
            if let Some(value) = current.as_bool() {
                let choices = vec![
                    format!("Keep current value ({value})"),
                    "False".into(),
                    "True".into(),
                ];
                Ok(select(label, &choices)?
                    .map(|i| if i == 0 { json!(value) } else { json!(i == 2) }))
            } else {
                Ok(menu(label, &["False", "True"])?.map(|i| json!(i == 1)))
            }
        }
        "integer" | "number" => {
            let mut text = current.as_i64().unwrap_or(0).to_string();
            loop {
                let Some(value) = prompt(label, "Whole number", &text, false)? else {
                    return Ok(None);
                };
                match value.parse::<i64>() {
                    Ok(value) => return Ok(Some(json!(value))),
                    Err(_) => {
                        text = value;
                        view("Invalid number", "Enter a signed whole number.")?;
                    }
                }
            }
        }
        "port" => Ok(number(label, current.as_u64().unwrap_or(8080))?.map(|v| json!(v))),
        "string_list" | "string_array" | "paths" | "arguments" => {
            let values = current
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(string_list_edit(label, &values, kind == "arguments")?.map(|values| json!(values)))
        }
        "ca_pool" => {
            match menu(
                label,
                &[
                    "System certificate authorities",
                    "Private CA files on the proxy",
                ],
            )? {
                Some(0) => Ok(Some(Value::Null)),
                Some(_) => {
                    let paths = current["pem_files"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    Ok(
                        string_list_edit("CA certificate file paths", &paths, false)?
                            .map(|paths| json!({"provider":"file","pem_files":paths})),
                    )
                }
                None => Ok(None),
            }
        }
        "certificate" => {
            match menu(
                label,
                &[
                    "Request a new certificate automatically",
                    "Use an existing certificate",
                ],
            )? {
                Some(0) => Ok(Some(json!("new"))),
                Some(_) => loop {
                    let Some(value) = number(
                        "Certificate ID in your proxy",
                        current.as_u64().filter(|v| *v > 0).unwrap_or(1),
                    )?
                    else {
                        return Ok(None);
                    };
                    if value > 0 {
                        return Ok(Some(json!(value)));
                    }
                    view(
                        "Invalid certificate ID",
                        "Choose a certificate ID greater than zero.",
                    )?;
                },
                None => Ok(None),
            }
        }
        "array" | "json" | "object" => structured_edit(label, current, true),
        _ => Ok(prompt(
            label,
            if kind == "secret" {
                "Value is hidden"
            } else {
                ""
            },
            current.as_str().unwrap_or(""),
            kind == "secret",
        )?
        .map(|v| json!(v))),
    }
}
pub async fn run(store: Store) -> Result<()> {
    let _terminal = Terminal::enter()?;
    loop {
        let cached = store.cached_update_status().ok();
        let title = cached
            .as_ref()
            .filter(|s| s["update_available"] == true)
            .and_then(|s| s["latest_version"].as_str())
            .map(|version| format!("Workspace | Selfhost {version} available in Updates"))
            .unwrap_or_else(|| "Workspace".into());
        let Some(choice) = menu(
            &title,
            &[
                "Projects and apps",
                "Create a project or deployment",
                "Existing apps",
                "Servers and moves",
                "Databases",
                "Networking and reverse proxies",
                "Identity and dashboard login",
                "Backups and removal recovery",
                "Activity and schedules",
                "Selfhost updates",
                "Tasks and triggers",
                "Help and keyboard controls",
                "Quit",
            ],
        )?
        else {
            break;
        };
        let outcome = match choice {
            0 => Box::pin(projects::browse(&store)).await,
            1 => Box::pin(projects::create(&store)).await,
            2 => Box::pin(existing(&store)).await,
            3 => Box::pin(administration::servers(&store)).await,
            4 => Box::pin(administration::databases(&store)).await,
            5 => Box::pin(administration::networking(&store)).await,
            6 => Box::pin(administration::identity(&store)).await,
            7 => Box::pin(administration::recovery(&store)).await,
            8 => administration::activity(&store),
            9 => match Box::pin(updates::browse(&store)).await {
                Ok(true) => break,
                Ok(false) => Ok(()),
                Err(error) => Err(error),
            },
            10 => Box::pin(tasks::tasks(&store)).await,
            11 => view(
                "Help",
                "Up/Down and Enter navigate. Type to filter menus. Esc returns without changes.\nSpace toggles multi-select lists. Ctrl+U clears text prompts.\nEditor: Ctrl+S accepts; Esc discards. Secret prompts are masked.\nPageUp/PageDown scroll output; Left/Right pans long lines.\nAn operation in progress finishes before navigation resumes.\nSnapshots exclude volume data. Review backup coverage before removal or restoration.\nLocal terminal access grants administrator access to this workspace.\nSee docs/tui.md and selfhost --help for command-line equivalents.",
            ),
            _ => break,
        };
        if let Err(error) = outcome {
            view(
                "Could not complete the operation",
                &format!(
                    "{error:#}\n\nNo automatic retry was performed. Review the error before trying again."
                ),
            )?;
        }
    }
    Ok(())
}
async fn container_pick(store: &Store, server: &str, profile: &str) -> Result<Option<String>> {
    let candidates = wait(
        "Finding matching containers",
        store.discover_existing(server, profile),
    )
    .await?;
    let mut choices = candidates
        .iter()
        .map(|row| {
            format!(
                "{} | {} | {}",
                row["name"].as_str().unwrap_or(""),
                row["image"].as_str().unwrap_or(""),
                row["status"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    choices.push("Enter an exact container name manually".into());
    let Some(index) = select("Choose a container to link read-only", &choices)? else {
        return Ok(None);
    };
    if index == candidates.len() {
        required("Exact container name or ID", "")
    } else {
        Ok(Some(
            candidates[index]["id"]
                .as_str()
                .context("Container identity missing")?
                .into(),
        ))
    }
}
async fn existing(store: &Store) -> Result<()> {
    loop {
        let apps = store.existing_apps()?;
        let mut items = vec!["Link an existing app".into()];
        items.extend(labels(&apps, |a| {
            format!(
                "{} [{}]",
                a.name,
                if a.allowed_actions.is_empty() {
                    "read-only"
                } else {
                    "selected actions enabled"
                }
            )
        }));
        let choice = take!(select("Existing apps", &items));
        if choice == 0 {
            let profiles: Vec<_> = store.existing_profiles()?.into_values().collect();
            let selected = take!(select(
                "App profile",
                &labels(&profiles, |p| p.name.clone())
            ));
            let profile = &profiles[selected];
            let name = take!(required("App name", &profile.name));
            let url = take!(required("App URL (HTTPS or HTTP localhost)", "https://"));
            let (mut server_id, mut container) = (String::new(), String::new());
            if !profile.image_repositories.is_empty()
                && yes("Connect a Docker container for status and declared actions?")?
            {
                let server = take!(server_pick(store, false, true));
                server_id = server.id;
                container = take!(container_pick(store, &server_id, &profile.id).await);
            }
            let out = wait(
                "Linking existing app",
                store.link_existing(crate::adoption::LinkExisting {
                    profile: profile.id.clone(),
                    name,
                    url,
                    server_id,
                    container,
                }),
            )
            .await?;
            show("Linked read-only app", &out)?;
            continue;
        }
        let app = &apps[choice - 1];
        match take!(menu(
            &app.name,
            &[
                "Connection details",
                "Container status",
                "Resource usage",
                "Declared app actions",
                "Management permissions",
                "Reconnect to its current container",
                "Unlink (preserve the running app)"
            ]
        )) {
            0 => show("Connection", app)?,
            1 => {
                let out = wait(
                    "Inspecting pinned container",
                    store.inspect_existing(&app.id),
                )
                .await?;
                show("Container status", &out)?;
            }
            2 => {
                let out = wait("Reading resource usage", store.existing_stats(&app.id)).await?;
                show("Resource usage", &out)?;
            }
            3 => {
                let index = take!(select(
                    "Declared actions",
                    &labels(&app.profile.actions, |a| format!(
                        "{}{}: {}",
                        a.label,
                        if a.write { " [writes]" } else { "" },
                        a.description
                    ))
                ));
                let action = &app.profile.actions[index];
                let confirmation = if action.write {
                    if !yes("Have you made a recent app backup and verified how to restore it?")? {
                        continue;
                    }
                    if !confirm_name("Confirm app action", &app.name)? {
                        continue;
                    }
                    app.name.as_str()
                } else {
                    ""
                };
                let out = wait(
                    &action.label,
                    store.existing_action(&app.id, &action.id, confirmation),
                )
                .await?;
                view("App action output", &out)?;
            }
            4 => {
                let actions: Vec<_> = app.profile.actions.iter().filter(|a| a.write).collect();
                let defaults: Vec<_> = actions
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| app.allowed_actions.contains(&a.id))
                    .map(|(i, _)| i)
                    .collect();
                let choices = take!(multi(
                    "Allow these specific write actions",
                    &labels(&actions, |a| format!("{}: {}", a.label, a.description)),
                    &defaults
                ));
                if confirm_name("Change management permissions", &app.name)? {
                    let out = store.consent_existing(
                        &app.id,
                        crate::adoption::ExistingConsent {
                            confirmation: app.name.clone(),
                            allowed_actions: choices
                                .iter()
                                .map(|i| actions[*i].id.clone())
                                .collect(),
                        },
                    )?;
                    show("Saved permissions", &out)?;
                }
            }
            5 => {
                let server = take!(server_pick(store, false, true));
                let container = take!(container_pick(store, &server.id, &app.profile.id).await);
                let plan = wait(
                    "Checking replacement container",
                    store.reconnect_existing_plan(&app.id, &server.id, &container),
                )
                .await?;
                let replacement = &plan["replacement"];
                view(
                    "Review replacement",
                    &format!(
                        "App: {}\nServer: {}\nContainer: {}\nImage: {}\n\nThe URL stays the same. Management permissions will be reset to read-only. Containers and data are not changed.",
                        app.name,
                        server.name,
                        replacement["name"].as_str().unwrap_or(""),
                        replacement["image"].as_str().unwrap_or("")
                    ),
                )?;
                if yes("Reconnect this app read-only?")? {
                    wait(
                        "Reconnecting app",
                        store.reconnect_existing(&app.id, &server.id, &container, revision(&plan)?),
                    )
                    .await?;
                    view(
                        "App reconnected",
                        "Review management permissions separately before enabling changes.",
                    )?;
                }
            }
            _ => {
                if confirm_name(
                    "Unlink reference only; app, volumes and backups remain",
                    &app.name,
                )? {
                    store.unlink_existing(&app.id, &app.name)?;
                    view("Unlinked", "The running app and its data were not changed.")?;
                }
            }
        }
    }
}
