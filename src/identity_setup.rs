//! Reviewable local login configuration. Directory discovery reads data, never scripts.
use crate::{
    auth::{LoginConfig, identity_url, login_revision},
    core::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationRequest {
    pub provider: String,
    pub issuer: String,
    #[serde(default)]
    pub provider_project: String,
    #[serde(default = "default_true")]
    pub create_project: bool,
    #[serde(default = "default_project_name")]
    pub project_name: String,
    #[serde(default)]
    pub organization_id: String,
    pub selfhost_url: String,
    pub id: String,
    pub name: String,
    pub admin_subjects: Vec<String>,
    #[serde(default)]
    pub ca_certificate: String,
}
fn default_true() -> bool {
    true
}
fn default_project_name() -> String {
    "Selfhost".into()
}
impl RegistrationRequest {
    fn creates_project(&self) -> bool {
        self.create_project && self.provider_project.is_empty()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectCreation {
    pub(crate) create_path: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Value,
    pub(crate) project_id_pointer: String,
    pub(crate) organization_lookup_path: String,
    pub(crate) organization_id_pointer: String,
    pub(crate) human_pointer: String,
    pub(crate) subject_pointer: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegistrationProfile {
    pub(crate) name: String,
    pub(crate) create_path: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Value,
    pub(crate) client_id_pointer: String,
    pub(crate) client_secret_pointer: String,
    #[serde(default)]
    pub(crate) project_creation: Option<ProjectCreation>,
}

pub(crate) fn registration_profile(id: &str) -> Result<RegistrationProfile> {
    ensure!(crate::setup::slug(id), "Invalid registration provider");
    let file = crate::catalog::BuiltinCatalog::get(&format!("identity-registration/{id}.json"))
        .context("This provider requires manual client registration")?;
    let profile: RegistrationProfile = serde_json::from_slice(&file.data)?;
    ensure!(
        profile.create_path.starts_with('/') && !profile.create_path.starts_with("//"),
        "Invalid registration endpoint"
    );
    Ok(profile)
}
pub fn registration_profiles() -> Result<Value> {
    let mut items = Vec::new();
    for path in crate::catalog::BuiltinCatalog::iter()
        .filter(|p| p.starts_with("identity-registration/") && p.ends_with(".json"))
    {
        let id = path
            .trim_start_matches("identity-registration/")
            .trim_end_matches(".json");
        let profile = registration_profile(id)?;
        items.push(json!({"id":id,"name":profile.name}));
    }
    Ok(json!(items))
}
/// Read-only discovery for an explicitly reviewed administrator selection.
/// A machine account never becomes a suggested human administrator.
pub async fn registration_account(
    request: &RegistrationRequest,
    credential: &str,
) -> Result<Value> {
    ensure!(
        !credential.is_empty() && credential.len() <= 8192,
        "Provide a temporary provider API credential"
    );
    let profile = registration_profile(&request.provider)?;
    let lookup = profile
        .project_creation
        .context("This provider requires manual administrator IDs")?;
    let issuer = identity_url(&request.issuer)?;
    ensure!(
        issuer.path() == "/",
        "Use the provider origin without a path"
    );
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30));
    if !request.ca_certificate.is_empty() {
        for cert in reqwest::Certificate::from_pem_bundle(request.ca_certificate.as_bytes())? {
            builder = builder.add_root_certificate(cert);
        }
    }
    let http = builder.build()?;
    let discovery = registration_response(
        http.get(issuer.join("/.well-known/openid-configuration")?)
            .send()
            .await
            .context("Identity discovery unavailable")?,
    )
    .await?;
    ensure!(
        discovery["issuer"]
            .as_str()
            .map(|s| s.trim_end_matches('/'))
            == Some(request.issuer.trim_end_matches('/')),
        "Discovery issuer differs from selected identity provider"
    );
    let endpoint = issuer.join(&lookup.organization_lookup_path)?;
    ensure!(
        endpoint.origin() == issuer.origin(),
        "Account lookup changed origin"
    );
    let account = registration_response(
        http.get(endpoint)
            .bearer_auth(credential)
            .send()
            .await
            .context("Provider account lookup unavailable")?,
    )
    .await?;
    let human = account
        .pointer(&lookup.human_pointer)
        .is_some_and(Value::is_object);
    let subject = human
        .then(|| {
            account
                .pointer(&lookup.subject_pointer)
                .and_then(Value::as_str)
        })
        .flatten()
        .filter(|s| !s.is_empty());
    Ok(
        json!({"human":human,"suggested_subject":subject,"organization_id":account.pointer(&lookup.organization_id_pointer).and_then(Value::as_str)}),
    )
}
fn registration_config(request: &RegistrationRequest) -> LoginConfig {
    LoginConfig {
        public_url: request.selfhost_url.clone(),
        providers: vec![crate::auth::LoginProvider {
            id: request.id.clone(),
            name: request.name.clone(),
            issuer: request.issuer.clone(),
            client_id: "registration-pending".into(),
            client_secret: String::new(),
            ca_certificate: request.ca_certificate.clone(),
            admin_subjects: request.admin_subjects.clone(),
        }],
    }
}
pub fn registration_plan(store: &Store, request: &RegistrationRequest) -> Result<Value> {
    registration_config(request).validate()?;
    let issuer = identity_url(&request.issuer)?;
    ensure!(
        issuer.path() == "/",
        "Automatic registration requires the provider origin without a path"
    );
    let profile = registration_profile(&request.provider)?;
    ensure!(
        request.provider_project.len() <= 200 && request.organization_id.len() <= 200,
        "Provider identifier is too long"
    );
    if request.creates_project() {
        ensure!(
            profile.project_creation.is_some(),
            "This provider requires an existing project ID"
        );
        ensure!(
            !request.project_name.trim().is_empty() && request.project_name.len() <= 200,
            "Provide a project name of up to 200 characters"
        );
    } else {
        ensure!(
            !request.provider_project.is_empty(),
            "Choose an existing identity provider project ID"
        );
    }
    let previous = store.login_config()?;
    ensure!(
        previous
            .as_ref()
            .is_none_or(|c| c.origin() == registration_config(request).origin()),
        "Change Selfhost's address using the reviewed login settings flow before adding a client, so every existing provider callback is confirmed."
    );
    ensure!(
        previous
            .as_ref()
            .is_none_or(|c| !c.providers.iter().any(|p| p.id == request.id)),
        "This connection ID already exists. Edit its existing settings instead."
    );
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            request,
            &profile,
            login_revision(previous.as_ref())?
        ))?)
    );
    let record_path = store
        .root
        .join("identity-registrations")
        .join(format!("{}.json", request.id));
    let resume = if record_path.exists() {
        let record: Value = serde_json::from_slice(&std::fs::read(record_path)?)?;
        (record["revision"].as_str() == Some(&revision)
            && matches!(
                record["stage"].as_str(),
                Some("project_created" | "client_rejected")
            ))
        .then(|| record["project_id"].as_str().map(str::to_owned))
        .flatten()
    } else {
        None
    };
    Ok(
        json!({"revision":revision,"provider":profile.name,"issuer":issuer.as_str(),"project":resume.as_deref().unwrap_or(if request.creates_project(){&request.project_name}else{&request.provider_project}),"create_project":request.creates_project()&&resume.is_none(),"resume":resume.is_some(),"organization":if request.organization_id.is_empty(){"The API credential's organization"}else{&request.organization_id},"callback":format!("{}/auth/callback",registration_config(request).origin()),"name":request.name,"administrator_count":request.admin_subjects.len(),"creates":if resume.is_some(){"One OIDC Web client in the recorded project, and one Selfhost sign-in connection. The existing project is reused"}else if request.creates_project(){"A dedicated project, one OIDC Web client, and one Selfhost sign-in connection"}else{"One OIDC Web client in your existing provider project, and one Selfhost sign-in connection"},"development_mode":identity_url(&request.selfhost_url)?.scheme()=="http","address_changed":false}),
    )
}
pub(crate) fn render_registration(
    value: &Value,
    variables: &BTreeMap<String, Value>,
) -> Result<Value> {
    Ok(match value {
        Value::String(s) if s.starts_with("{{") && s.ends_with("}}") => variables
            .get(&s[2..s.len() - 2])
            .context("Unknown registration variable")?
            .clone(),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| render_registration(v, variables))
                .collect::<Result<_>>()?,
        ),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), render_registration(v, variables)?)))
                .collect::<Result<_>>()?,
        ),
        v => v.clone(),
    })
}
async fn registration_response(mut response: reqwest::Response) -> Result<Value> {
    ensure!(
        response.status().is_success(),
        "Identity provider rejected registration (HTTP {})",
        response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Identity response interrupted")?
    {
        ensure!(
            bytes.len() + chunk.len() <= 1024 * 1024,
            "Identity response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("Invalid identity response")
}
/// The credential is never included in the revision, response, or journal.
pub async fn register(
    store: &Store,
    request: RegistrationRequest,
    revision: &str,
    credential: &str,
) -> Result<Value> {
    use crate::core::{atomic_write, private_dir, token};
    use fs2::FileExt;
    ensure!(
        !credential.is_empty() && credential.len() <= 8192,
        "Provide a temporary provider API credential"
    );
    let directory = store.root.join("identity-registrations");
    private_dir(&directory)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("registration.lock"))?;
    lock.try_lock_exclusive()
        .context("Another identity registration is running")?;
    let preview = registration_plan(store, &request)?;
    ensure!(
        preview["revision"].as_str() == Some(revision),
        "Review a fresh registration plan"
    );
    let journal = directory.join(format!("{}.json", request.id));
    let mut record: Value = if journal.exists() {
        let saved: Value = serde_json::from_slice(&std::fs::read(&journal)?)?;
        ensure!(
            saved["revision"].as_str() == Some(revision),
            "Registration record belongs to another plan; inspect it before proceeding"
        );
        ensure!(
            matches!(
                saved["stage"].as_str(),
                Some("project_created" | "client_rejected")
            ),
            "A registration record already exists with an uncertain or completed outcome. Inspect its project and application IDs before retrying"
        );
        saved
    } else {
        json!({"stage":"prepared","application_id":format!("selfhost-{}",token(12)?),"project_id":if request.creates_project(){format!("selfhost-{}",token(12)?)}else{request.provider_project.clone()},"request":request,"revision":revision})
    };
    let previous = store.login_config()?;
    let expected = login_revision(previous.as_ref())?;
    let profile = registration_profile(&request.provider)?;
    let issuer = identity_url(&request.issuer)?;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30));
    if !request.ca_certificate.is_empty() {
        for cert in reqwest::Certificate::from_pem_bundle(request.ca_certificate.as_bytes())? {
            builder = builder.add_root_certificate(cert);
        }
    }
    let http = builder.build()?;
    let discovery = registration_response(
        http.get(issuer.join("/.well-known/openid-configuration")?)
            .send()
            .await
            .context("Identity discovery unavailable")?,
    )
    .await?;
    ensure!(
        discovery["issuer"]
            .as_str()
            .map(|s| s.trim_end_matches('/'))
            == Some(request.issuer.trim_end_matches('/')),
        "Discovery issuer differs from selected identity provider"
    );
    let application_id = record["application_id"]
        .as_str()
        .context("Registration application ID missing")?
        .to_owned();
    let mut variables = BTreeMap::from([
        ("application-id".into(), json!(application_id)),
        ("project-id".into(), record["project_id"].clone()),
        ("project-name".into(), json!(request.project_name)),
        ("name".into(), json!(request.name)),
        ("redirect-uri".into(), preview["callback"].clone()),
        (
            "loopback-development".into(),
            preview["development_mode"].clone(),
        ),
    ]);
    if request.creates_project() && record["stage"] == "prepared" {
        let project = profile
            .project_creation
            .as_ref()
            .context("Project creation is unavailable")?;
        let organization = if request.organization_id.is_empty() {
            let endpoint = issuer.join(&project.organization_lookup_path)?;
            ensure!(
                endpoint.origin() == issuer.origin(),
                "Organization lookup changed origin"
            );
            let account = registration_response(
                http.get(endpoint)
                    .bearer_auth(credential)
                    .send()
                    .await
                    .context("Could not read provider account organization")?,
            )
            .await?;
            account.pointer(&project.organization_id_pointer).and_then(Value::as_str).filter(|v| !v.is_empty()).context("Provider account organization missing; supply the organization ID in advanced settings")?.to_owned()
        } else {
            request.organization_id.clone()
        };
        variables.insert("organization-id".into(), json!(organization));
        let body = render_registration(&project.body, &variables)?;
        let endpoint = issuer.join(&project.create_path)?;
        ensure!(
            endpoint.origin() == issuer.origin(),
            "Project creation changed origin"
        );
        let mut post = http.post(endpoint).json(&body).bearer_auth(credential);
        for (key, value) in &project.headers {
            post = post.header(key, value);
        }
        record["stage"] = json!("project_creation_uncertain");
        atomic_write(&journal, &serde_json::to_vec_pretty(&record)?)?;
        let response = registration_response(post.send().await.context("Project creation outcome is uncertain; inspect the recorded project ID before recovery")?).await.context("Project creation did not finish; inspect the recorded project ID before recovery")?;
        let project_id = response
            .pointer(&project.project_id_pointer)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .context("Project ID missing; inspect the recorded project ID before recovery")?;
        record["project_id"] = json!(project_id);
        record["stage"] = json!("project_created");
        atomic_write(&journal, &serde_json::to_vec_pretty(&record)?)?;
        variables.insert("project-id".into(), json!(project_id));
    }
    let body = render_registration(&profile.body, &variables)?;
    let endpoint = issuer.join(&profile.create_path)?;
    ensure!(
        endpoint.origin() == issuer.origin(),
        "Registration endpoint changed origin"
    );
    let mut post = http.post(endpoint).json(&body).bearer_auth(credential);
    for (key, value) in &profile.headers {
        post = post.header(key, value);
    }
    record["stage"] = json!("creation_uncertain");
    atomic_write(&journal, &serde_json::to_vec_pretty(&record)?)?;
    let response = post.send().await.context(
        "Registration outcome is uncertain. Inspect the journal's application ID before retrying",
    )?;
    // Definite validation/authentication rejection is safe to resume using the
    // recorded project. Conflicts, server errors and lost replies are uncertain.
    if matches!(
        response.status().as_u16(),
        400 | 401 | 403 | 404 | 405 | 422
    ) {
        record["stage"] = json!("client_rejected");
        atomic_write(&journal, &serde_json::to_vec_pretty(&record)?)?;
        anyhow::bail!(
            "Client creation rejected (HTTP {}). The project is preserved. Correct the API credential or provider permissions, then retry this reviewed plan; Selfhost will reuse the recorded project.",
            response.status().as_u16()
        );
    }
    let response = registration_response(response).await.context(
        "Registration did not finish; inspect the recorded application ID before retrying",
    )?;
    atomic_write(
        &directory.join(format!("{}-response.json", request.id)),
        &serde_json::to_vec_pretty(&response)?,
    )?;
    let mut new = registration_config(&request).providers.remove(0);
    new.client_id = response
        .pointer(&profile.client_id_pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .context("Client ID missing; provider response was saved privately")?
        .into();
    new.client_secret = response
        .pointer(&profile.client_secret_pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .context("Client secret missing; provider response was saved privately")?
        .into();
    let mut config = previous.unwrap_or_default();
    config.public_url = request.selfhost_url;
    config.providers.push(new);
    atomic_write(
        &journal,
        &serde_json::to_vec_pretty(
            &json!({"stage":"client_created","application_id":application_id,"project_id":record["project_id"],"config":config,"revision":revision}),
        )?,
    )?;
    store.save_login_config_checked(&mut config, Some(&expected))?;
    atomic_write(
        &journal,
        &serde_json::to_vec_pretty(
            &json!({"stage":"connected","application_id":application_id,"project_id":record["project_id"],"provider_id":request.id,"revision":revision}),
        )?,
    )?;
    Ok(
        json!({"saved":true,"application_id":application_id,"project_id":record["project_id"],"public_url":config.origin(),"callback":preview["callback"],"config":config.public()}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyLogin {
    pub config: LoginConfig,
    pub expected_revision: String,
    pub callbacks_confirmed: bool,
}

pub fn plan(store: &Store, config: &LoginConfig) -> Result<Value> {
    config.validate()?;
    let previous = store.login_config()?;
    let state_revision = login_revision(previous.as_ref())?;
    let revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(&state_revision, config))?)
    );
    let callback = format!("{}/auth/callback", config.origin());
    Ok(json!({
        "revision":revision,
        "previous_url":previous.as_ref().map(|c|c.origin()),
        "public_url":config.origin(),
        "address_changed":previous.as_ref().is_some_and(|c|c.origin()!=config.origin()),
        "callback":callback,
        "providers":config.providers.iter().map(|p|json!({"id":p.id,"name":p.name,"issuer":p.issuer,"client_id":p.client_id,"administrators":p.admin_subjects.len(),"callback":callback})).collect::<Vec<_>>(),
        "backup":previous.is_some(),
        "steps":["Register the exact callback URL on each identity provider client. Keep the previous callback until the new sign-in has been tested.","For a domain address, configure DNS and a TLS reverse proxy to Selfhost before saving.","Keep this local recovery session open while testing sign-in at the new address. Existing identity-provider sessions expire when settings change."]
    }))
}

pub fn apply(
    store: &Store,
    mut config: LoginConfig,
    expected_revision: &str,
    callbacks_confirmed: bool,
) -> Result<Value> {
    ensure!(
        callbacks_confirmed,
        "Confirm the exact callback has been registered at every provider"
    );
    let previous = store.login_config()?;
    let state_revision = login_revision(previous.as_ref())?;
    let proposed = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(&state_revision, &config))?)
    );
    ensure!(
        crate::auth::equal(expected_revision, &proposed),
        "Login settings or draft changed. Review a fresh plan."
    );
    store.save_login_config_checked(&mut config, Some(&state_revision))?;
    Ok(
        json!({"saved":true,"public_url":config.origin(),"callback":format!("{}/auth/callback",config.origin()),"backup_created":previous.is_some()}),
    )
}

/// Explicit supported manifest names; native provider paths live in contributor profiles.
pub fn read_manifest(directory: &Path) -> Result<LoginConfig> {
    let directory = directory
        .canonicalize()
        .context("Identity directory does not exist")?;
    for name in ["selfhost-login.json", ".selfhost-login.json"] {
        if let Some(bytes) = safe_read(&directory, name)? {
            let config: LoginConfig =
                serde_json::from_slice(&bytes).context("Invalid Selfhost login manifest")?;
            config.validate()?;
            return Ok(config);
        }
    }
    anyhow::bail!(
        "No selfhost-login.json found. Run identity inspect, then provide issuer, registered client credentials, and exact administrator subject IDs in the manifest."
    )
}

fn safe_read(root: &Path, relative: &str) -> Result<Option<Vec<u8>>> {
    let path = root.join(relative);
    if !path.exists() {
        return Ok(None);
    }
    let resolved = path.canonicalize()?;
    ensure!(
        resolved.starts_with(root),
        "Identity configuration points outside the selected directory"
    );
    ensure!(
        std::fs::metadata(&resolved)?.is_file()
            && std::fs::metadata(&resolved)?.len() <= 1024 * 1024,
        "Identity configuration must be a regular file smaller than 1 MiB"
    );
    Ok(Some(std::fs::read(resolved)?))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryProfile {
    id: String,
    name: String,
    image_markers: Vec<String>,
    environment_issuer: Vec<String>,
    instructions: Vec<String>,
    #[serde(default)]
    native_yaml: Vec<NativeSource>,
    #[serde(default)]
    origin_parts: Option<OriginParts>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeSource {
    file: String,
    values: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginParts {
    host: String,
    port: String,
    secure: String,
}

/// Never evaluates shell expansion, Compose interpolation, env files, or provider commands.
pub fn inspect(directory: &Path, selfhost_url: &str) -> Result<Value> {
    let url = identity_url(selfhost_url)?;
    ensure!(url.path() == "/", "Selfhost address must be an origin");
    let root = directory
        .canonicalize()
        .context("Identity directory does not exist")?;
    let mut images = Vec::new();
    let mut environment = BTreeMap::<String, String>::new();
    let mut sources = Vec::new();
    for name in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ] {
        if let Some(bytes) = safe_read(&root, name)? {
            let compose: Value =
                serde_yaml::from_slice(&bytes).context("Invalid Compose document")?;
            sources.push(name.to_string());
            if let Some(services) = compose.get("services").and_then(Value::as_object) {
                for service in services.values() {
                    if let Some(image) = service.get("image").and_then(Value::as_str) {
                        images.push(image.to_owned());
                    }
                    if let Some(env) = service.get("environment") {
                        if let Some(map) = env.as_object() {
                            for (k, v) in map {
                                if let Some(v) = v.as_str() {
                                    environment.insert(k.clone(), v.into());
                                }
                            }
                        }
                        if let Some(list) = env.as_array() {
                            for item in list {
                                if let Some((k, v)) = item.as_str().and_then(|s| s.split_once('='))
                                {
                                    environment.insert(k.into(), v.into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(bytes) = safe_read(&root, ".env")? {
        sources.push(".env".into());
        for line in std::str::from_utf8(&bytes)?.lines() {
            if let Some((key, value)) = line
                .trim()
                .strip_prefix("export ")
                .unwrap_or(line.trim())
                .split_once('=')
            {
                let key = key.trim();
                if key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                    environment
                        .entry(key.into())
                        .or_insert_with(|| value.trim().trim_matches(['\'', '"']).into());
                }
            }
        }
    }
    let mut detected = Vec::new();
    for path in crate::catalog::BuiltinCatalog::iter()
        .filter(|s| s.starts_with("identity-directories/") && s.ends_with(".json"))
    {
        let file =
            crate::catalog::BuiltinCatalog::get(&path).context("Missing directory profile")?;
        let profile: DirectoryProfile = serde_json::from_slice(&file.data)?;
        let mut native = false;
        let mut hints = environment.clone();
        for source in &profile.native_yaml {
            if let Some(bytes) = safe_read(&root, &source.file)? {
                let document: Value =
                    serde_yaml::from_slice(&bytes).context("Invalid native identity YAML")?;
                for (key, pointer) in &source.values {
                    if let Some(value) = document.pointer(pointer)
                        && (value.is_string() || value.is_boolean() || value.is_number())
                    {
                        hints.entry(key.clone()).or_insert_with(|| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        });
                    }
                }
                native = true;
                sources.push(source.file.clone());
            }
        }
        if native
            || images.iter().any(|image| {
                profile
                    .image_markers
                    .iter()
                    .any(|marker| image.contains(marker))
            })
        {
            let mut issuer = profile.environment_issuer.iter().find_map(|key| {
                hints
                    .get(key)
                    .filter(|value| !value.contains(['$', '`']))
                    .and_then(|value| identity_url(value).ok())
                    .map(|url| url.to_string())
            });
            if issuer.is_none()
                && let Some(parts) = &profile.origin_parts
                && let (Some(host), Some(port), Some(secure)) = (
                    hints.get(&parts.host),
                    hints.get(&parts.port),
                    hints.get(&parts.secure),
                )
                && let Ok(port) = port.parse::<u16>()
                && matches!(secure.as_str(), "true" | "false")
                && !host.contains(['/', '@', '?', '#', '$', '`'])
            {
                let scheme = if secure == "true" { "https" } else { "http" };
                issuer = identity_url(&format!("{scheme}://{host}:{port}"))
                    .ok()
                    .map(|url| url.to_string());
            }
            let automatic = registration_profile(&profile.id).is_ok();
            let mut missing = vec!["admin_subjects"];
            if issuer.is_none() {
                missing.push("issuer");
            }
            let template=automatic.then(||json!({"provider":profile.id,"issuer":issuer.clone().unwrap_or_default(),"create_project":true,"project_name":"Selfhost","selfhost_url":url.origin().ascii_serialization(),"id":"home","name":"Home identity","admin_subjects":[]}));
            detected.push(json!({"id":profile.id,"name":profile.name,"issuer_hint":issuer,"instructions":profile.instructions,"automatic_registration":automatic,"registration_template":template,"missing_registration_fields":missing}));
        }
    }
    let manifest = match ["selfhost-login.json", ".selfhost-login.json"]
        .iter()
        .find(|name| root.join(name).exists())
    {
        Some(name) => {
            sources.push((*name).into());
            let config = read_manifest(&root)?;
            Some(config.public())
        }
        None => None,
    };
    Ok(
        json!({"sources":sources,"detected":detected,"callback":format!("{}/auth/callback",url.origin().ascii_serialization()),"manifest":manifest,"next_steps":["Use an existing OIDC client or register a Web application with authorization code and PKCE S256.","Set the exact callback shown here. For loopback HTTP, enable your provider's local development option if required.","Run selfhost identity setup in this directory for guided registration or connection details; no JSON file is required.","Apply on this host using the dashboard data directory, or paste the private setup code into Access or selfhost identity connect on the dashboard host. Review exact administrator subject IDs before confirming."]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_does_not_evaluate_env_or_expose_secrets() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("compose.yaml"),"services:\n  idp:\n    image: quay.io/keycloak/keycloak:26\n    environment:\n      KC_HOSTNAME: https://identity.example.com\n      CLIENT_SECRET: private-value\n").unwrap();
        std::fs::write(
            temp.path().join(".env"),
            "SECRET=$(do-not-execute)\nPASSWORD=private-value",
        )
        .unwrap();
        let result = inspect(temp.path(), "http://localhost:8797").unwrap();
        assert_eq!(
            result["detected"][0]["issuer_hint"],
            "https://identity.example.com/"
        );
        let output = result.to_string();
        assert!(!output.contains("private-value") && !output.contains("do-not-execute"));
        assert!(inspect(temp.path(), "http://example.com").is_err());
    }
    #[test]
    fn native_zitadel_origin_is_detected_without_disclosing_configuration() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("zitadel.yaml"),"ExternalDomain: identity.example.com\nExternalPort: 443\nExternalSecure: true\nMasterkey: private-masterkey\n").unwrap();
        let result = inspect(temp.path(), "http://localhost:8797").unwrap();
        assert_eq!(
            result["detected"][0]["issuer_hint"],
            "https://identity.example.com/"
        );
        assert_eq!(result["detected"][0]["automatic_registration"], true);
        assert!(!result.to_string().contains("private-masterkey"));
    }
    fn draft() -> LoginConfig {
        LoginConfig {
            public_url: "http://localhost:8797".into(),
            providers: vec![crate::auth::LoginProvider {
                id: "home".into(),
                name: "Home".into(),
                issuer: "https://identity.example.com".into(),
                client_id: "selfhost".into(),
                client_secret: "private-client-secret".into(),
                ca_certificate: String::new(),
                admin_subjects: vec!["owner".into()],
            }],
        }
    }
    #[test]
    fn reviewed_address_change_creates_backup_and_rejects_stale_drafts() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let initial = draft();
        let preview = plan(&store, &initial).unwrap();
        assert!(!preview.to_string().contains("private-client-secret"));
        assert!(
            apply(
                &store,
                initial.clone(),
                preview["revision"].as_str().unwrap(),
                false
            )
            .is_err()
        );
        apply(
            &store,
            initial.clone(),
            preview["revision"].as_str().unwrap(),
            true,
        )
        .unwrap();
        let mut changed = initial.clone();
        changed.public_url = "https://selfhost.example.com".into();
        assert!(
            store.save_login_config(changed.clone()).is_err(),
            "Legacy CLI must not bypass address-change review"
        );
        let preview = plan(&store, &changed).unwrap();
        assert_eq!(preview["address_changed"], true);
        let mut other = initial;
        other.providers[0].name = "Changed elsewhere".into();
        store.save_login_config(other).unwrap();
        assert!(
            apply(
                &store,
                changed.clone(),
                preview["revision"].as_str().unwrap(),
                true
            )
            .is_err()
        );
        let preview = plan(&store, &changed).unwrap();
        apply(&store, changed, preview["revision"].as_str().unwrap(), true).unwrap();
        assert!(
            std::fs::read_dir(temp.path().join("login-backups"))
                .unwrap()
                .count()
                >= 2
        );
    }
    #[tokio::test]
    async fn registration_creates_real_client_once_and_saves_private_credentials() {
        use axum::{
            Json, Router,
            http::HeaderMap,
            routing::{get, post},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = issuer.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let router=Router::new().route("/.well-known/openid-configuration",get(move||{let issuer=discovery.clone();async move{Json(json!({"issuer":issuer}))}})).route("/zitadel.application.v2.ApplicationService/CreateApplication",post(move|headers:HeaderMap,Json(body):Json<Value>|{let calls=calls.clone();async move{
            assert_eq!(headers["authorization"],"Bearer ephemeral-token");assert_eq!(body["oidcConfiguration"]["developmentMode"],true);assert_eq!(body["oidcConfiguration"]["redirectUris"],json!(["http://localhost:8797/auth/callback"]));calls.fetch_add(1,Ordering::SeqCst);Json(json!({"oidcConfiguration":{"clientId":"new-client","clientSecret":"one-time-secret"}}))
        }}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let request = RegistrationRequest {
            provider: "zitadel".into(),
            issuer,
            provider_project: "test-project".into(),
            create_project: true,
            project_name: default_project_name(),
            organization_id: String::new(),
            selfhost_url: "http://localhost:8797".into(),
            id: "home".into(),
            name: "Home".into(),
            admin_subjects: vec!["owner".into()],
            ca_certificate: String::new(),
        };
        let preview = registration_plan(&store, &request).unwrap();
        let revision = preview["revision"].as_str().unwrap();
        let result = register(&store, request.clone(), revision, "ephemeral-token")
            .await
            .unwrap();
        assert_eq!(result["saved"], true);
        assert!(!result.to_string().contains("one-time-secret"));
        assert_eq!(
            store.login_config().unwrap().unwrap().providers[0].client_secret,
            "one-time-secret"
        );
        assert!(
            register(&store, request, revision, "ephemeral-token")
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let journal =
            std::fs::read_to_string(temp.path().join("identity-registrations/home.json")).unwrap();
        assert!(!journal.contains("ephemeral-token"));
        server.abort();
    }
    #[tokio::test]
    async fn lost_registration_reply_is_journaled_and_never_retried() {
        use axum::{
            Json, Router,
            http::StatusCode,
            routing::{get, post},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = issuer.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let router = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(move || {
                    let issuer = discovery.clone();
                    async move { Json(json!({"issuer":issuer})) }
                }),
            )
            .route(
                "/zitadel.application.v2.ApplicationService/CreateApplication",
                post(move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        StatusCode::BAD_GATEWAY
                    }
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let request = RegistrationRequest {
            provider: "zitadel".into(),
            issuer,
            provider_project: "test-project".into(),
            create_project: true,
            project_name: default_project_name(),
            organization_id: String::new(),
            selfhost_url: "http://localhost:8797".into(),
            id: "home".into(),
            name: "Home".into(),
            admin_subjects: vec!["owner".into()],
            ca_certificate: String::new(),
        };
        let preview = registration_plan(&store, &request).unwrap();
        let revision = preview["revision"].as_str().unwrap();
        assert!(
            register(&store, request.clone(), revision, "temporary-token")
                .await
                .is_err()
        );
        assert!(
            register(&store, request, revision, "temporary-token")
                .await
                .unwrap_err()
                .to_string()
                .contains("record already exists")
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(store.login_config().unwrap().is_none());
        let record: Value = serde_json::from_slice(
            &std::fs::read(temp.path().join("identity-registrations/home.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(record["stage"], "creation_uncertain");
        assert!(!record.to_string().contains("temporary-token"));
        server.abort();
    }

    async fn project_fixture(failure: &str) {
        use axum::{
            Json, Router,
            http::{HeaderMap, StatusCode},
            response::IntoResponse,
            routing::{get, post},
        };
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = issuer.clone();
        let project_calls = Arc::new(AtomicUsize::new(0));
        let client_calls = Arc::new(AtomicUsize::new(0));
        let saved_id = Arc::new(Mutex::new(String::new()));
        let p = project_calls.clone();
        let c = client_calls.clone();
        let project_id = saved_id.clone();
        let client_project_id = saved_id.clone();
        let uncertain = failure == "project_uncertain";
        let reject_client = failure == "client_rejected";
        let router = Router::new()
            .route("/.well-known/openid-configuration",get(move || {let issuer=discovery.clone(); async move {Json(json!({"issuer":issuer}))}}))
            .route("/auth/v1/users/me",get(|headers:HeaderMap| async move {
                assert_eq!(headers["authorization"],"Bearer ephemeral-api-token");
                Json(json!({"user":{"id":"human-owner","details":{"resourceOwner":"owner-organization"},"human":{"profile":{"displayName":"Test owner"}}}}))
            }))
            .route("/zitadel.project.v2.ProjectService/CreateProject",post(move |headers:HeaderMap,Json(body):Json<Value>| {
                let p=p.clone();let project_id=project_id.clone();async move {
                    assert_eq!(headers["connect-protocol-version"],"1");
                    assert_eq!(headers["authorization"],"Bearer ephemeral-api-token");
                    assert_eq!(body["name"],"Selfhost");assert_eq!(body["organizationId"],"owner-organization");
                    let id=body["projectId"].as_str().unwrap().to_string();
                    *project_id.lock().unwrap()=id.clone();p.fetch_add(1,Ordering::SeqCst);
                    if uncertain {StatusCode::BAD_GATEWAY.into_response()} else {Json(json!({"projectId":id})).into_response()}
                }
            }))
            .route("/zitadel.application.v2.ApplicationService/CreateApplication",post(move |Json(body):Json<Value>| {
                let c=c.clone();let project_id=client_project_id.clone();async move {
                    assert_eq!(body["projectId"].as_str().unwrap(),*project_id.lock().unwrap());
                    let previous=c.fetch_add(1,Ordering::SeqCst);
                    if reject_client && previous==0 {StatusCode::FORBIDDEN.into_response()} else {Json(json!({"oidcConfiguration":{"clientId":"new-client","clientSecret":"one-time-client-secret"}})).into_response()}
                }
            }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let request:RegistrationRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":issuer,"selfhost_url":"http://localhost:8797","id":"home","name":"Home","admin_subjects":["human-owner"]})).unwrap();
        assert!(request.creates_project());
        assert_eq!(request.project_name, "Selfhost");
        let account = registration_account(&request, "ephemeral-api-token")
            .await
            .unwrap();
        assert_eq!(account["suggested_subject"], "human-owner");
        assert_eq!(account["organization_id"], "owner-organization");
        let preview = registration_plan(&store, &request).unwrap();
        assert_eq!(preview["create_project"], true);
        let revision = preview["revision"].as_str().unwrap();
        let first = register(&store, request.clone(), revision, "ephemeral-api-token").await;
        if uncertain {
            assert!(first.is_err());
            assert!(
                register(&store, request, revision, "ephemeral-api-token")
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("record already exists")
            );
            assert!(store.login_config().unwrap().is_none());
            assert_eq!(client_calls.load(Ordering::SeqCst), 0);
        } else {
            let result = if reject_client {
                assert!(
                    first
                        .unwrap_err()
                        .to_string()
                        .contains("project is preserved")
                );
                register(&store, request, revision, "ephemeral-api-token")
                    .await
                    .unwrap()
            } else {
                first.unwrap()
            };
            assert_eq!(result["saved"], true);
            assert_eq!(
                result["project_id"].as_str().unwrap(),
                *saved_id.lock().unwrap()
            );
            assert_eq!(
                client_calls.load(Ordering::SeqCst),
                if reject_client { 2 } else { 1 }
            );
            assert!(!result.to_string().contains("one-time-client-secret"));
        }
        assert_eq!(project_calls.load(Ordering::SeqCst), 1);
        let journal =
            std::fs::read_to_string(temp.path().join("identity-registrations/home.json")).unwrap();
        assert!(!journal.contains("ephemeral-api-token"));
        server.abort();
    }
    #[tokio::test]
    async fn new_project_registration_creates_project_then_client() {
        project_fixture("success").await;
    }
    #[tokio::test]
    async fn uncertain_project_creation_is_not_retried() {
        project_fixture("project_uncertain").await;
    }
    #[tokio::test]
    async fn rejected_client_resumes_with_saved_project_without_duplicate() {
        project_fixture("client_rejected").await;
    }

    #[tokio::test]
    async fn machine_token_is_never_suggested_as_human_administrator() {
        use axum::{Json, Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discovery = issuer.clone();
        let router=Router::new().route("/.well-known/openid-configuration",get(move||{let issuer=discovery.clone();async move{Json(json!({"issuer":issuer}))}}))
            .route("/auth/v1/users/me",get(||async{Json(json!({"user":{"id":"machine-owner","machine":{"name":"Automation"},"details":{"resourceOwner":"organization"}}}))}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let request:RegistrationRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":issuer,"selfhost_url":"http://localhost:8797","id":"home","name":"Home","admin_subjects":[]})).unwrap();
        let result = registration_account(&request, "token").await.unwrap();
        assert_eq!(result["human"], false);
        assert!(result["suggested_subject"].is_null());
        assert!(!result.to_string().contains("machine-owner"));
        server.abort();
    }
}
