//! Interactive app operations reuse the same validated plans as the dashboard.
use crate::{
    core::Store,
    guided,
    integrations::Field,
    onboarding::{AppLink, OnboardingRequest},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn revision(plan: &Value) -> Result<&str> {
    plan["revision"]
        .as_str()
        .context("The reviewed operation has no revision")
}

/// Structured values are entered as individual fields, never pasted JSON.
fn structured(depth: usize) -> Result<Value> {
    ensure!(depth < 8, "Configuration nesting is too deep");
    let choices = [
        "Object",
        "List",
        "Text",
        "Number",
        "Boolean",
        "Empty",
        "Hidden text",
    ]
    .map(String::from);
    Ok(match guided::choose("Value type", &choices)? {
        0 => {
            let mut values = serde_json::Map::new();
            while guided::confirm("Add a field?", false)? {
                ensure!(values.len() < 64, "Too many fields");
                let name = guided::input("Field name", "")?;
                ensure!(
                    !name.is_empty() && !values.contains_key(&name),
                    "Use a unique nonempty field name"
                );
                values.insert(name, structured(depth + 1)?);
            }
            Value::Object(values)
        }
        1 => {
            let mut values = Vec::new();
            while guided::confirm("Add an item?", false)? {
                ensure!(values.len() < 64, "Too many items");
                values.push(structured(depth + 1)?);
            }
            Value::Array(values)
        }
        2 => json!(guided::input("Text", "")?),
        3 => loop {
            let value = guided::input("Number", "0")?;
            if let Ok(number) = value.parse::<serde_json::Number>() {
                break Value::Number(number);
            }
            println!("Enter a number.");
        },
        4 => json!(guided::confirm("Enabled?", false)?),
        6 => json!(guided::secret("Value")?),
        _ => Value::Null,
    })
}

/// Retain unedited native fields, including secret values that are never displayed.
fn edit_structured(current: Option<Value>, depth: usize) -> Result<Value> {
    ensure!(depth < 8, "Configuration nesting is too deep");
    match current {
        Some(Value::Object(mut values)) => loop {
            let keys = values.keys().cloned().collect::<Vec<_>>();
            let mut options = vec!["Finish editing".into(), "Add a field".into()];
            options.extend(keys.iter().cloned());
            let choice = guided::choose("Configuration fields (values are private)", &options)?;
            if choice == 0 {
                return Ok(Value::Object(values));
            }
            if choice == 1 {
                ensure!(values.len() < 64, "Too many fields");
                let key = guided::input("Field name", "")?;
                ensure!(
                    !key.is_empty() && !values.contains_key(&key),
                    "Use a unique nonempty field name"
                );
                values.insert(key, structured(depth + 1)?);
            } else {
                let key = &keys[choice - 2];
                let action = guided::choose(key, &["Edit".into(), "Remove".into(), "Back".into()])?;
                if action == 0 {
                    let value = edit_structured(values.get(key).cloned(), depth + 1)?;
                    values.insert(key.clone(), value);
                } else if action == 1 && guided::confirm("Remove this field?", false)? {
                    values.remove(key);
                }
            }
        },
        Some(Value::Array(mut values)) => loop {
            let mut options = vec!["Finish editing".into(), "Add an item".into()];
            options.extend((1..=values.len()).map(|n| format!("Item {n}")));
            let choice = guided::choose("List items (values are private)", &options)?;
            if choice == 0 {
                return Ok(Value::Array(values));
            }
            if choice == 1 {
                ensure!(values.len() < 64, "Too many items");
                values.push(structured(depth + 1)?);
            } else {
                let index = choice - 2;
                let action =
                    guided::choose("Item", &["Edit".into(), "Remove".into(), "Back".into()])?;
                if action == 0 {
                    values[index] = edit_structured(Some(values[index].clone()), depth + 1)?;
                } else if action == 1 && guided::confirm("Remove this item?", false)? {
                    values.remove(index);
                }
            }
        },
        _ => structured(depth),
    }
}

pub fn field(field: &Field, current: Option<&Value>) -> Result<Value> {
    if !field.description.is_empty() {
        println!("{}", field.description);
    }
    let default = field_default(field, current);
    if !field.choices.is_empty() {
        return Ok(json!(
            field.choices[guided::choose(&field.label, &field.choices)?]
        ));
    }
    let value = match field.kind.as_str() {
        "secret" => loop {
            let value = guided::secret(&field.label)?;
            if valid_text(field, &value) {
                break json!(value);
            }
            println!("The value does not meet this setting's length requirements.");
        },
        "boolean" => json!(guided::confirm(
            &field.label,
            default.and_then(Value::as_bool).unwrap_or(false)
        )?),
        "integer" => loop {
            let text = guided::input(
                &field.label,
                &default.map(Value::to_string).unwrap_or_default(),
            )?;
            if let Ok(value) = text.parse::<i64>()
                && field.minimum.is_none_or(|min| value >= min)
                && field.maximum.is_none_or(|max| value <= max)
            {
                break json!(value);
            }
            println!(
                "Enter a whole number within {:?} to {:?}.",
                field.minimum, field.maximum
            );
        },
        "arguments" | "string_list" => {
            println!(
                "{}: enter one item at a time; leave an item blank to finish.",
                field.label
            );
            let mut values = Vec::new();
            loop {
                let value = guided::input("Item", "")?;
                if value.is_empty() {
                    break;
                }
                ensure!(values.len() < 64, "Too many items");
                values.push(value);
            }
            json!(values)
        }
        "json" => loop {
            let value = edit_structured(default.cloned(), 0)?;
            if (value.is_object() || value.is_array())
                && serde_json::to_vec(&value)?.len() <= 128 * 1024
            {
                break value;
            }
            println!("This setting requires an object or list.");
        },
        _ => loop {
            let value = guided::input(
                &field.label,
                default.and_then(Value::as_str).unwrap_or_default(),
            )?;
            if valid_text(field, &value) {
                break json!(value);
            }
            println!("The value does not meet this setting's length requirements.");
        },
    };
    Ok(value)
}

fn field_default<'a>(field: &'a Field, current: Option<&'a Value>) -> Option<&'a Value> {
    if field.kind == "secret" {
        None
    } else {
        current.filter(|v| !v.is_null()).or(field.default.as_ref())
    }
}

pub(crate) fn settings_review(plan: &Value, profile: &crate::integrations::Profile) -> Value {
    let mut review = plan.clone();
    if let Some(changes) = review["changes"].as_array_mut() {
        for change in changes {
            if profile.fields.iter().any(|field| {
                change["id"] == field.id && matches!(field.kind.as_str(), "secret" | "json")
            }) {
                change["before"] = json!("Hidden");
                change["after"] = json!("Hidden");
            }
        }
    }
    review
}

fn valid_text(field: &Field, value: &str) -> bool {
    value.len() <= 4096
        && !value.contains('\0')
        && value.chars().count() >= field.minimum_length
        && field
            .maximum_length
            .is_none_or(|max| value.chars().count() <= max)
}

fn links(info: &Value) -> Result<Vec<AppLink>> {
    let mut result = Vec::new();
    if let Some(suggestions) = info["suggestions"].as_array() {
        for suggestion in suggestions {
            let link: AppLink = serde_json::from_value(suggestion.clone())?;
            if info["state"]["linked_urls"]
                .as_array()
                .is_some_and(|urls| urls.iter().any(|url| url.as_str() == Some(&link.url)))
            {
                continue;
            }
            if guided::confirm(&format!("Add {} ({})?", link.name, link.url), true)? {
                result.push(link);
            }
        }
    }
    while guided::confirm("Add another application link?", false)? {
        result.push(AppLink {
            name: guided::input("Application name", "")?,
            url: guided::input("Application URL", "")?,
            icon: String::new(),
            description: String::new(),
        });
    }
    Ok(result)
}

pub async fn setup(store: &Store, id: &str, service: &str) -> Result<()> {
    guided::interactive()?;
    let info = store.onboarding_info(id, service)?;
    ensure!(
        info["supported"] == true,
        "This app has no automatic setup profile"
    );
    if info["state"]["completed"] == true {
        println!("Application setup is already complete.");
        if info["profile"]["supports_sync"] == true
            && guided::confirm("Add application links?", true)?
        {
            return Box::pin(sync(store, id, service)).await;
        }
        return Ok(());
    }
    let modes: Vec<String> = serde_json::from_value(info["profile"]["modes"].clone())?;
    let modes: Vec<String> = modes.into_iter().filter(|mode| mode != "sync").collect();
    ensure!(!modes.is_empty(), "This app has no initial setup mode");
    let mode = modes[guided::choose("How should this app be connected?", &modes)?].clone();
    let fields: Vec<crate::onboarding::Field> =
        serde_json::from_value(info["profile"]["fields"].clone())?;
    let mut inputs = BTreeMap::new();
    for field in fields.iter().filter(|field| field.modes.contains(&mode)) {
        loop {
            let value = if field.kind == "secret" {
                guided::secret(&field.label)?
            } else {
                guided::input(&field.label, field.default.as_deref().unwrap_or_default())?
            };
            if value.is_empty() && !field.required {
                break;
            }
            if value.encode_utf16().count() >= field.minimum_length
                && value.encode_utf16().count() <= field.maximum_length
                && (field.characters.is_empty()
                    || value.chars().all(|c| field.characters.contains(c)))
                && (!field.required || !value.is_empty())
            {
                inputs.insert(field.id.clone(), value);
                break;
            }
            println!(
                "Enter {} to {} characters.",
                field.minimum_length, field.maximum_length
            );
        }
    }
    let apps = if info["profile"]["accepts_apps"] == true {
        links(&info)?
    } else {
        vec![]
    };
    onboarding(store, id, service, OnboardingRequest { mode, inputs, apps }).await
}

async fn onboarding(
    store: &Store,
    id: &str,
    service: &str,
    request: OnboardingRequest,
) -> Result<()> {
    let plan = store.onboarding_plan(id, service, request.clone()).await?;
    guided::review(&plan);
    if guided::confirm("Apply this application setup?", false)? {
        let result = store
            .onboarding_apply(id, service, request, revision(&plan)?)
            .await?;
        guided::review(&result);
    }
    Ok(())
}

pub async fn sync(store: &Store, id: &str, service: &str) -> Result<()> {
    guided::interactive()?;
    let info = store.onboarding_info(id, service)?;
    ensure!(
        info["profile"]["supports_sync"] == true,
        "This app has no link synchronization profile"
    );
    onboarding(
        store,
        id,
        service,
        OnboardingRequest {
            mode: "sync".into(),
            inputs: BTreeMap::new(),
            apps: links(&info)?,
        },
    )
    .await
}

pub async fn settings(store: &Store, id: &str, service: &str) -> Result<()> {
    guided::interactive()?;
    let state = store.integration_state(id, service).await?;
    let profile = store.integration(id, service)?;
    let mut fields = profile.fields.clone();
    let mut changes = BTreeMap::new();
    while !fields.is_empty() {
        let mut choices: Vec<String> = fields.iter().map(|f| f.label.clone()).collect();
        choices.push("Review changes".into());
        let selected = guided::choose("Setting to change", &choices)?;
        if selected == fields.len() {
            break;
        }
        let selected = fields.remove(selected);
        changes.insert(
            selected.id.clone(),
            field(&selected, state["values"].get(&selected.id))?,
        );
    }
    if changes.is_empty() {
        println!("No settings changed.");
        return Ok(());
    }
    let plan = store.integration_plan(id, service, &changes).await?;
    guided::review(&settings_review(&plan, &profile));
    if guided::confirm("Apply these settings?", false)? {
        guided::review(
            &store
                .integration_apply(id, service, changes, revision(&plan)?)
                .await?,
        );
    }
    Ok(())
}

pub async fn action(store: &Store, id: &str, service: &str, action_id: &str) -> Result<()> {
    guided::interactive()?;
    let project = store.project(id)?;
    let definition = &project
        .services
        .iter()
        .find(|s| s.app == service)
        .context("Service not found")?
        .definition;
    if let Some(action) = definition.actions.iter().find(|a| a.id == action_id) {
        println!("{}: {}", action.label, action.description);
        if guided::confirm("Run this action?", false)? {
            println!("{}", store.service_action(id, service, action_id).await?);
        }
        return Ok(());
    }
    let profile = store.integration(id, service)?;
    let action = profile
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .context("Unknown action")?;
    println!("{}: {}", action.label, action.description);
    let mut values = BTreeMap::new();
    for input in &action.inputs {
        values.insert(input.id.clone(), field(input, None)?);
    }
    guided::review(
        &json!({"action":action.label,"steps":action.steps.iter().map(|step| &step.label).collect::<Vec<_>>()}),
    );
    if guided::confirm("Run this action?", false)? {
        let result = store
            .integration_action(id, service, action_id, values)
            .await?;
        guided::review(&result);
        ensure!(result["ok"] == true, "App action needs attention");
    }
    Ok(())
}

pub async fn connect(store: &Store, id: &str, service: &str) -> Result<()> {
    guided::interactive()?;
    let existing = store.connection_state(id, service)?;
    if !existing.is_null() {
        guided::review(&existing);
        if guided::confirm("Resume this saved connection?", false)? {
            guided::review(&store.connection_resume(id, service).await?);
        }
        return Ok(());
    }
    let profiles = store.identity_providers()?;
    let ids: Vec<String> = profiles.keys().cloned().collect();
    ensure!(
        !ids.is_empty(),
        "No automatic provider registration profiles are installed"
    );
    let names = ids
        .iter()
        .map(|id| profiles[id].name.clone())
        .collect::<Vec<_>>();
    let provider = ids[guided::choose("Identity provider", &names)?].clone();
    let creates = crate::identity_setup::registration_profile(&provider)
        .ok()
        .is_some_and(|p| p.project_creation.is_some());
    let mut request = crate::connections::ConnectRequest {
        provider,
        issuer: guided::input("Identity provider URL", "")?,
        provider_project: if creates {
            String::new()
        } else {
            guided::input("Existing provider project", "")?
        },
        create_project: creates,
        project_name: format!("Selfhost {service}"),
        organization_id: String::new(),
        administrator_subject: String::new(),
        replace_existing: false,
        ca_certificate: String::new(),
        app_url: guided::input("Application browser URL", "")?,
        name: guided::input("Login button label", "Sign in")?,
        credential: String::new(),
        revision: String::new(),
    };
    if guided::confirm("Change advanced provider options?", false)? {
        request.organization_id = guided::input("Organization ID (optional)", "")?;
        request.provider_project = guided::input(
            "Existing project ID (blank creates a dedicated project)",
            &request.provider_project,
        )?;
        request.create_project = request.provider_project.is_empty();
        let ca_path = guided::input("Private CA certificate path (optional)", "")?;
        if !ca_path.is_empty() {
            request.ca_certificate = std::fs::read_to_string(ca_path)
                .context("Cannot read the private CA certificate")?;
        }
        request.replace_existing =
            guided::confirm("Replace this application's existing login settings?", false)?;
    }
    request.credential = guided::secret("Temporary provider API token")?;
    let client = store
        .integration(id, service)?
        .oidc_client
        .context("This app has no OIDC connection profile")?;
    if !client.administrator_environment.is_empty()
        && guided::confirm("Choose an application administrator?", true)?
    {
        match store.connection_account(id, service, &request).await {
            Ok(account) => {
                guided::review(&account);
                if let Some(subject) = account["suggested_subject"].as_str()
                    && guided::confirm("Authorize this account as administrator?", false)?
                {
                    request.administrator_subject = subject.into();
                }
            }
            Err(_) => println!(
                "The provider could not identify a human account. You can enter its subject ID."
            ),
        }
        if request.administrator_subject.is_empty() {
            request.administrator_subject =
                guided::input("Administrator subject ID (optional)", "")?;
        }
    }
    let plan = store.connection_plan(id, service, &request)?;
    guided::review(&plan);
    if guided::confirm("Create the login connection and configure this app?", false)? {
        request.revision = revision(&plan)?.into();
        let result = store.connection_create(id, service, request).await?;
        guided::review(&result);
        ensure!(
            result["action"]["ok"] == true,
            "Connection needs attention; run the same command to resume"
        );
    }
    Ok(())
}

pub async fn update(store: &Store, id: &str) -> Result<()> {
    guided::interactive()?;
    let project = store.project(id)?;
    let setup = store.setup(id)?;
    let mut plans = Vec::new();
    for service in &project.services {
        let current = setup.compose["services"][&service.app]["image"]
            .as_str()
            .context("Missing image")?;
        let options = crate::versions::options(&service.definition);
        let choices = options["choices"]
            .as_array()
            .context("Invalid version choices")?;
        let mut labels = vec![format!("Keep {current}")];
        labels.extend(choices.iter().map(|c| {
            format!(
                "{} ({})",
                c["label"].as_str().unwrap_or_default(),
                c["image"].as_str().unwrap_or_default()
            )
        }));
        labels.push("Choose another tag or digest".into());
        let index = guided::choose(&format!("Version for {}", service.definition.name), &labels)?;
        if index == 0 {
            let selection = crate::versions::Selection {
                image: current.into(),
                allow_untested: true,
            };
            let plan = store.version_plan(id, &service.app, &selection)?;
            plans.push((service.app.clone(), selection, plan));
            continue;
        }
        let custom = index == labels.len() - 1;
        let image = if custom {
            guided::input("Full image tag or digest", current)?
        } else {
            choices[index - 1]["image"]
                .as_str()
                .context("Missing image choice")?
                .to_owned()
        };
        let allow_untested = custom
            && guided::confirm(
                "Use this untested version after reviewing its migration requirements?",
                false,
            )?;
        let selection = crate::versions::Selection {
            image,
            allow_untested,
        };
        let plan = store.version_plan(id, &service.app, &selection)?;
        guided::review(&plan);
        plans.push((service.app.clone(), selection, plan));
    }
    println!(
        "Updating snapshots configuration, pulls selected images and recreates containers. This does not back up application or database data."
    );
    if !guided::confirm(
        "Have you backed up the app data, and want to update now?",
        false,
    )? {
        return Ok(());
    }
    apply_versions(store, id, &plans)?;
    store.action(id, "refresh").await.context("The update did not complete. Reviewed version choices remain saved; inspect app status and the configuration snapshot before retrying")?;
    println!("Application update completed.");
    Ok(())
}

fn apply_versions(
    store: &Store,
    id: &str,
    plans: &[(String, crate::versions::Selection, Value)],
) -> Result<()> {
    if plans.is_empty() {
        return Ok(());
    }
    let _lock = store.project_lock(id)?;
    let mut setup = store.setup(id)?;
    let before = setup.compose.clone();
    for (service, selection, reviewed) in plans {
        let current = store.version_plan(id, service, selection)?;
        ensure!(
            revision(&current)? == revision(reviewed)?,
            "Application configuration changed during review; run update again"
        );
        setup.compose["services"][service]["image"] = json!(selection.image);
    }
    if setup.compose != before {
        store.save_setup_locked(id, setup)?;
    }
    Ok(())
}

pub fn init_inputs(
    store: &Store,
    app: &str,
    stack: bool,
    method: &mut Option<String>,
    acknowledgements: &mut Vec<String>,
) -> Result<BTreeMap<String, Value>> {
    guided::interactive()?;
    let blueprint = if stack {
        Some(app.to_owned())
    } else {
        let deployments = store.deployments()?;
        let methods: Vec<_> = deployments
            .as_array()
            .context("Invalid deployments")?
            .iter()
            .filter(|m| m["app"] == app)
            .collect();
        if method.is_none() && !methods.is_empty() {
            let labels = methods
                .iter()
                .map(|m| {
                    format!(
                        "{}: {}",
                        m["name"].as_str().unwrap_or_default(),
                        m["description"].as_str().unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>();
            *method = Some(
                methods[guided::choose("Deployment method", &labels)?]["id"]
                    .as_str()
                    .context("Missing deployment method")?
                    .into(),
            );
        }
        methods
            .iter()
            .find(|m| m["id"].as_str() == method.as_deref())
            .and_then(|m| m["blueprint"].as_str())
            .map(str::to_owned)
    };
    let mut inputs = BTreeMap::new();
    if let Some(blueprint) = blueprint {
        let blueprints = store.blueprints()?;
        let profile = blueprints.get(&blueprint).context("Unknown stack")?;
        for (key, input) in &profile.inputs {
            let default = input
                .default
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| input.default.to_string());
            let text = guided::input(&input.label, &default)?;
            let value = if input.kind == "port" {
                json!(text.parse::<u16>().context("Enter a valid port")?)
            } else {
                json!(text)
            };
            inputs.insert(key.clone(), value);
        }
        for requirement in &profile.requirements {
            if !acknowledgements.contains(&requirement.id) {
                ensure!(
                    guided::confirm(&requirement.label, false)?,
                    "Deployment cancelled; requirement not accepted"
                );
                acknowledgements.push(requirement.id.clone());
            }
        }
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_preserve_false_zero_and_empty_text_but_never_expose_secrets() {
        let mut field: Field = serde_json::from_value(
            json!({"id":"setting","label":"Setting","kind":"string","default":"fallback"}),
        )
        .unwrap();
        for current in [json!(false), json!(0), json!("")] {
            assert_eq!(field_default(&field, Some(&current)), Some(&current));
        }
        assert_eq!(
            field_default(&field, Some(&Value::Null)),
            field.default.as_ref()
        );
        field.kind = "secret".into();
        assert!(field_default(&field, Some(&json!("saved-password"))).is_none());
        assert!(field_default(&field, None).is_none());
    }

    #[test]
    fn structured_and_secret_settings_are_hidden_in_reviews_without_mutating_the_plan() {
        let profile: crate::integrations::Profile = serde_json::from_value(json!({
            "schema":1,"name":"Example","documentation":"https://example.org","driver":{"kind":"compose_environment"},
            "fields":[{"id":"structured","label":"Structured","kind":"json"},{"id":"key","label":"Key","kind":"secret"},{"id":"enabled","label":"Enabled","kind":"boolean"}]
        })).unwrap();
        let plan = json!({"revision":"bound","changes":[
            {"id":"structured","before":{"customCredential":"old-secret"},"after":{"customCredential":"new-secret"}},
            {"id":"key","before":"old-key","after":"new-key"},
            {"id":"enabled","before":false,"after":true}
        ]});
        let review = settings_review(&plan, &profile);
        for secret in ["old-secret", "new-secret", "old-key", "new-key"] {
            assert!(!review.to_string().contains(secret));
        }
        assert_eq!(review["changes"][2]["after"], true);
        assert_eq!(review["revision"], plan["revision"]);
        assert_eq!(
            plan["changes"][0]["after"]["customCredential"],
            "new-secret"
        );
    }
    #[test]
    fn application_plans_require_a_revision() {
        assert!(revision(&json!({"revision":null})).is_err());
        assert_eq!(revision(&json!({"revision":"checked"})).unwrap(), "checked");
    }

    #[test]
    fn grouped_version_changes_reject_stale_reviews_without_partial_writes() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Guided versions".into(),
                apps: vec!["homarr".into(), "grafana".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let mut plans = Vec::new();
        for service in &project.services {
            let image = format!(
                "{}:guided-test",
                crate::versions::image_repository(&service.definition.image).unwrap()
            );
            let selection = crate::versions::Selection {
                image,
                allow_untested: true,
            };
            let plan = store
                .version_plan(&project.id, &service.app, &selection)
                .unwrap();
            plans.push((service.app.clone(), selection, plan));
        }
        let before = store.setup(&project.id).unwrap();
        plans[1].2["revision"] = json!("stale");
        assert!(apply_versions(&store, &project.id, &plans).is_err());
        assert_eq!(store.setup(&project.id).unwrap().compose, before.compose);
        plans[1].2 = store
            .version_plan(&project.id, &plans[1].0, &plans[1].1)
            .unwrap();
        apply_versions(&store, &project.id, &plans).unwrap();
        let after = store.setup(&project.id).unwrap();
        assert_eq!(after.environment, before.environment);
        assert_eq!(after.files, before.files);
        for (service, selection, _) in plans {
            assert_eq!(after.compose["services"][service]["image"], selection.image);
        }
    }
}
