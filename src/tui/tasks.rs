use super::*;
use crate::tasks::{Destination, TaskKind, TaskRequest};

pub(super) async fn tasks(store: &Store) -> Result<()> {
    loop {
        let value = store.tasks()?;
        let rules = value.as_array().context("Invalid task list")?;
        let mut options = vec![
            "Create a dashboard link task".into(),
            "Run due tasks once".into(),
        ];
        options.extend(rules.iter().map(|r| {
            format!(
                "{} [{}]",
                r["request"]["name"].as_str().unwrap_or("Task"),
                r["status"].as_str().unwrap_or("unknown")
            )
        }));
        let choice = take!(select("Tasks and triggers", &options));
        if choice == 0 {
            let value = store.task_destinations()?;
            let choices: Vec<_> = value
                .as_array()
                .context("Invalid destinations")?
                .iter()
                .filter(|d| d["supports_sync"] == true && d["configured"] == true)
                .collect();
            if choices.is_empty() {
                show(
                    "No configured destination",
                    &json!({"help":"Complete app onboarding on a dashboard that supports link sync first."}),
                )?;
                continue;
            }
            let selected = take!(select(
                "Destination dashboard",
                &choices
                    .iter()
                    .map(|d| d["name"].as_str().unwrap_or("Dashboard").to_owned())
                    .collect::<Vec<_>>()
            ));
            let d = choices[selected];
            let name = take!(required("Task name", "Keep dashboard links current"));
            let projects = store.read()?.projects;
            let (managed, project_ids) = take!(source_scope(
                "Managed project links",
                &projects
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone()))
                    .collect::<Vec<_>>()
            ));
            let apps = store.existing_apps()?;
            let (existing, existing_ids) = take!(source_scope(
                "Existing app links",
                &apps
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone()))
                    .collect::<Vec<_>>()
            ));
            let interval_seconds =
                take!(required("Interval in seconds, at least 60", "300")).parse::<u64>()?;
            let request = TaskRequest {
                name,
                kind: TaskKind::DashboardLinks,
                destination: Destination {
                    project_id: d["project_id"].as_str().context("Missing project")?.into(),
                    service: d["service"].as_str().context("Missing service")?.into(),
                },
                managed,
                existing,
                project_ids,
                existing_ids,
                interval_seconds,
                action: String::new(),
            };
            let plan = wait("Reviewing task", store.task_plan(request.clone())).await?;
            show("Review recurring writes and source scope", &plan)?;
            if yes("Enable this recurring task with the reviewed scope?")? {
                let saved =
                    wait("Saving task", store.task_create(request, revision(&plan)?)).await?;
                show("Task saved", &saved)?;
            }
        } else if choice == 1 {
            show(
                "Task results",
                &wait("Running due tasks", store.tasks_tick()).await?,
            )?;
        } else {
            let rule = &rules[choice - 2];
            let id = rule["id"].as_str().context("Missing task ID")?;
            show("Task status and history", rule)?;
            match take!(menu(
                "Task action",
                &[
                    "Run once",
                    "Enable or retry a blocked task",
                    "Disable",
                    "Remove task"
                ]
            )) {
                0 => show(
                    "Task result",
                    &wait("Running task", store.task_run(id)).await?,
                )?,
                1 => {
                    show("Task enabled", &store.task_set_enabled(id, true)?)?;
                }
                2 => {
                    show("Task disabled", &store.task_set_enabled(id, false)?)?;
                }
                _ => {
                    let name = take!(required(
                        "Type the task name to remove scheduling; existing app links remain",
                        ""
                    ));
                    store.task_remove(id, &name)?;
                }
            }
        }
    }
}

fn source_scope(title: &str, sources: &[(String, String)]) -> Result<Option<(bool, Vec<String>)>> {
    loop {
        let Some(scope) = menu(
            title,
            &[
                "Do not include",
                "Include all current and future sources",
                "Choose specific sources",
            ],
        )?
        else {
            return Ok(None);
        };
        match scope {
            0 => return Ok(Some((false, vec![]))),
            1 => return Ok(Some((true, vec![]))),
            _ => {
                if sources.is_empty() {
                    view(
                        "No sources yet",
                        "Choose all current and future sources, or return without including this category.",
                    )?;
                    continue;
                }
                let Some(selected) = multi(
                    title,
                    &sources
                        .iter()
                        .map(|(_, name)| name.clone())
                        .collect::<Vec<_>>(),
                    &[],
                )?
                else {
                    continue;
                };
                if selected.is_empty() {
                    view(
                        "No sources selected",
                        "Select at least one source, or choose Do not include.",
                    )?;
                    continue;
                }
                return Ok(Some((
                    true,
                    selected.into_iter().map(|i| sources[i].0.clone()).collect(),
                )));
            }
        }
    }
}
