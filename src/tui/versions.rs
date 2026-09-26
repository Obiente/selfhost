use super::*;

pub(super) async fn manage(store: &Store, id: &str, service: &str) -> Result<()> {
    let project = store.project(id)?;
    let app = project
        .services
        .iter()
        .find(|s| s.app == service)
        .context("Service not found")?;
    let mode = take!(menu(
        "App version",
        &[
            "View recipe versions",
            "Check upstream releases",
            "Review an image change"
        ]
    ));
    let options = crate::versions::options(&app.definition);
    if mode == 0 {
        return show("Recipe versions", &options);
    }
    if mode == 1 {
        return show(
            "Upstream release candidates",
            &wait(
                "Checking upstream releases",
                store.catalog_version_check(&app.definition.id),
            )
            .await?,
        );
    }
    let choices = options["choices"]
        .as_array()
        .context("Version choices missing")?;
    let mut labels: Vec<_> = choices
        .iter()
        .map(|v| {
            format!(
                "{} ({})",
                v["label"].as_str().unwrap_or("Version"),
                v["image"].as_str().unwrap_or("")
            )
        })
        .collect();
    labels.push("Enter a custom image tag or digest".into());
    let selected = take!(select("Choose image", &labels));
    let (image, allow_untested) = if selected == choices.len() {
        let image = take!(required("Full image tag or digest", &app.image));
        if !review(
            "Untested image",
            &json!({"image":image,"warning":"Recipe settings, actions and migrations may be incompatible. Review the upstream upgrade guide and back up data. No compatibility guarantee is implied."}),
        )? {
            return Ok(());
        }
        (image, true)
    } else {
        (choices[selected]["image"].as_str().unwrap().into(), false)
    };
    let selection = crate::versions::Selection {
        image,
        allow_untested,
    };
    let plan = store.version_plan(id, service, &selection)?;
    if review(
        "Save image choice without changing running containers",
        &plan,
    )? {
        show(
            "Saved image choice",
            &store.version_apply(id, service, &selection, revision(&plan)?)?,
        )?;
    }
    Ok(())
}
