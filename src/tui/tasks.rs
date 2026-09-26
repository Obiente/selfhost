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
            let managed =
                yes("Include managed projects? An empty ID list includes future projects too.")?;
            let project_ids = if managed {
                strings(&take!(field(
                    "Project IDs, comma separated; blank includes all current and future projects",
                    ""
                )))
            } else {
                vec![]
            };
            let existing =
                yes("Include linked existing apps? An empty ID list includes future links too.")?;
            let existing_ids = if existing {
                strings(&take!(field(
                    "Existing app IDs, comma separated; blank includes all current and future links",
                    ""
                )))
            } else {
                vec![]
            };
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
