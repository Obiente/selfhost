use super::*;
use crate::updates::{UpdateActivation, UpdateApproval};

/// True means the replacement helper is waiting for this process to exit.
pub(super) async fn browse(store: &Store) -> Result<bool> {
    loop {
        let Some(choice) = menu(
            "Selfhost updates",
            &[
                "Installed version and cached update information",
                "Check for updates now",
                "Review and stage an update",
                "Staged updates and recovery",
            ],
        )?
        else {
            return Ok(false);
        };
        match choice {
            0 => show("Update information", &store.cached_update_status()?)?,
            1 => show(
                "Update check",
                &wait("Checking package registry", store.update_status()).await?,
            )?,
            2 => {
                let status = wait("Checking update eligibility", store.update_status()).await?;
                if status["can_stage"] != true {
                    show("Update information and installation instructions", &status)?;
                    continue;
                }
                let plan = wait("Preparing update plan", store.plan_update()).await?;
                show("Review release, scope and warnings", &plan)?;
                let confirmation = plan["confirmation"]
                    .as_str()
                    .context("Update confirmation missing")?;
                if !confirm_name(
                    "Build this reviewed release in private staging",
                    confirmation,
                )? {
                    continue;
                }
                let job = wait(
                    "Building the approved update",
                    store.stage_update(UpdateApproval {
                        plan_id: plan["id"]
                            .as_str()
                            .context("Update plan ID missing")?
                            .into(),
                        revision: revision(&plan)?.into(),
                        confirmation: confirmation.into(),
                    }),
                )
                .await?;
                show("Staged update; activate from the updates list", &job)?;
            }
            _ => {
                let jobs = store.update_jobs()?;
                let Some(index) = select(
                    "Update history",
                    &labels(&jobs, |j| {
                        format!(
                            "{} [{}] {}",
                            j["version"].as_str().unwrap_or(""),
                            j["status"].as_str().unwrap_or(""),
                            j["job_id"].as_str().unwrap_or("")
                        )
                    }),
                )?
                else {
                    continue;
                };
                let job = &jobs[index];
                show("Update details and recovery instructions", job)?;
                let recovery = job["can_recover"] == true;
                if job["status"] != "staged" && !recovery {
                    continue;
                }
                let confirmation = job[if recovery {
                    "recovery_confirmation"
                } else {
                    "confirmation"
                }]
                .as_str()
                .context("Update confirmation missing")?;
                let title = if recovery {
                    "Recover the previous executable and exit Selfhost; data migrations are not reversed"
                } else {
                    "Activate this update and exit Selfhost; reopen after the helper completes"
                };
                if !confirm_name(title, confirmation)? {
                    continue;
                }
                let approval = UpdateActivation {
                    job_id: job["job_id"]
                        .as_str()
                        .context("Update job ID missing")?
                        .into(),
                    confirmation: confirmation.into(),
                };
                let result = if recovery {
                    store.recover_update(approval, None)?
                } else {
                    store.activate_update(approval, None)?
                };
                if result["shutdown_required"] == true {
                    // No further terminal I/O: the helper must see this process exit,
                    // even if its terminal disconnected immediately after approval.
                    return Ok(true);
                }
                show("Update result", &result)?;
            }
        }
    }
}
