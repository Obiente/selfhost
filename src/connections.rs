//! OIDC provisioning contracts belong to catalog profiles, not app-specific Rust branches.
use crate::{
    core::{Store, atomic_write, private_dir, token},
    integrations::{Profile, expand},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcClient {
    pub callback_path: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub environment_json: BTreeMap<String, Value>,
    #[serde(default)]
    pub administrator_environment: BTreeMap<String, String>,
    #[serde(default)]
    pub administrator_role: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcProvider {
    pub create_path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    pub client_id_pointer: String,
    pub client_secret_pointer: String,
}
impl OidcClient {
    pub fn has_environment(&self) -> bool {
        !self.environment.is_empty() || !self.environment_json.is_empty()
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectRequest {
    pub provider: String,
    pub issuer: String,
    #[serde(default)]
    pub provider_project: String,
    #[serde(default = "yes")]
    pub create_project: bool,
    #[serde(default = "project_name")]
    pub project_name: String,
    #[serde(default)]
    pub organization_id: String,
    #[serde(default)]
    pub administrator_subject: String,
    #[serde(default)]
    pub replace_existing: bool,
    #[serde(default)]
    pub ca_certificate: String,
    pub app_url: String,
    pub name: String,
    #[serde(default)]
    pub credential: String,
    #[serde(default)]
    pub revision: String,
}
fn yes() -> bool {
    true
}
fn project_name() -> String {
    "Selfhost".into()
}
impl ConnectRequest {
    fn creates_project(&self) -> bool {
        self.create_project && self.provider_project.is_empty()
    }
}
#[derive(Serialize, Deserialize)]
struct Connection {
    service: String,
    server_id: String,
    issuer: String,
    provider_project: String,
    callback: String,
    name: String,
    application_id: String,
    stage: String,
    client_id: String,
    client_secret: String,
    client: OidcClient,
    #[serde(default)]
    administrator_subject: String,
    #[serde(default)]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    revision: String,
    #[serde(default)]
    source_revision: String,
}
impl Connection {
    fn public(&self) -> Value {
        json!({"service":self.service,"issuer":self.issuer,"provider_project":self.provider_project,"callback":self.callback,"name":self.name,"application_id":self.application_id,"stage":self.stage,"client_id":self.client_id,"administrator_subject":self.administrator_subject,"administrator_role":if self.administrator_subject.is_empty(){""}else{&self.client.administrator_role}})
    }
}
fn secure_origin(text: &str) -> Result<reqwest::Url> {
    let url = crate::auth::identity_url(text)?;
    ensure!(
        url.path() == "/",
        "Use a provider or app origin without a path"
    );
    Ok(url)
}
fn render(value: &Value, variables: &BTreeMap<String, Value>) -> Result<Value> {
    Ok(match value {
        Value::String(s) => json!(expand(s, variables)?),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| render(v, variables))
                .collect::<Result<_>>()?,
        ),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), render(v, variables)?)))
                .collect::<Result<_>>()?,
        ),
        v => v.clone(),
    })
}
fn connection_environment(
    record: &Connection,
    app_url: &str,
    discovery: &Value,
) -> Result<BTreeMap<String, String>> {
    if !record.client.has_environment() {
        return Ok(BTreeMap::new());
    }
    let mut variables = BTreeMap::from([
        ("issuer-url".into(), json!(record.issuer)),
        (
            "discovery-url".into(),
            json!(format!(
                "{}/.well-known/openid-configuration",
                record.issuer.trim_end_matches('/')
            )),
        ),
        ("callback-url".into(), json!(record.callback)),
        ("name".into(), json!(record.name)),
        ("client-id".into(), json!(record.client_id)),
        ("client-secret".into(), json!(record.client_secret)),
        ("app-url".into(), json!(app_url.trim_end_matches('/'))),
        (
            "administrator-subject".into(),
            json!(record.administrator_subject),
        ),
    ]);
    let mut templates = record.client.environment.clone();
    if !record.administrator_subject.is_empty() {
        templates.extend(record.client.administrator_environment.clone());
    }
    let json_templates = serde_json::to_string(&record.client.environment_json)?;
    for (variable, field) in [
        ("authorization-url", "authorization_endpoint"),
        ("token-url", "token_endpoint"),
        ("userinfo-url", "userinfo_endpoint"),
        ("jwks-url", "jwks_uri"),
    ] {
        if !json_templates.contains(&format!("{{{{{variable}}}}}"))
            && !templates
                .values()
                .any(|v| v.contains(&format!("{{{{{variable}}}}}")))
        {
            continue;
        }
        let value = discovery[field]
            .as_str()
            .context("Provider discovery is missing an OIDC endpoint")?;
        crate::auth::identity_url(value)?;
        variables.insert(variable.into(), json!(value));
    }
    let mut result: BTreeMap<String, String> = templates
        .iter()
        .map(|(k, v)| Ok((k.clone(), expand(v, &variables)?)))
        .collect::<Result<_>>()?;
    for (key, template) in &record.client.environment_json {
        result.insert(
            key.clone(),
            serde_json::to_string(&render(template, &variables)?)?,
        );
    }
    Ok(result)
}
async fn bounded_json(mut response: reqwest::Response) -> Result<Value> {
    ensure!(
        response.status().is_success(),
        "Identity provider rejected the request (HTTP {})",
        response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Identity provider response interrupted")?
    {
        ensure!(
            bytes.len() + chunk.len() <= 1024 * 1024,
            "Identity provider response is too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("Identity provider returned invalid JSON")
}
impl Store {
    pub fn identity_providers(&self) -> Result<BTreeMap<String, Profile>> {
        let mut profiles = BTreeMap::new();
        for path in crate::catalog::BuiltinCatalog::iter()
            .filter(|s| s.starts_with("integrations/") && s.ends_with(".json"))
        {
            let file = crate::catalog::BuiltinCatalog::get(&path).context("Missing profile")?;
            let profile: Profile = serde_json::from_slice(&file.data)?;
            profile.validate()?;
            if profile.oidc_provider.is_some() {
                profiles.insert(
                    path.trim_start_matches("integrations/")
                        .trim_end_matches(".json")
                        .into(),
                    profile,
                );
            }
        }
        for app in &self.catalog {
            if let Some(profile) = &app.integration
                && profile.oidc_provider.is_some()
            {
                profiles.insert(app.id.clone(), profile.clone());
            }
        }
        Ok(profiles)
    }
    pub fn connection_state(&self, id: &str, service: &str) -> Result<Value> {
        self.integration(id, service)?;
        ensure!(crate::setup::slug(service), "Invalid service");
        let path = self
            .project_dir(id)?
            .join("connections")
            .join(format!("{service}.json"));
        if !path.exists() {
            return Ok(Value::Null);
        }
        let record: Connection = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(record.public())
    }
    pub fn connection_plan(
        &self,
        id: &str,
        service: &str,
        request: &ConnectRequest,
    ) -> Result<Value> {
        let project = self.project(id)?;
        self.project_docker(&project, true)?;
        let profile = self.integration(id, service)?;
        let client = profile
            .oidc_client
            .context("This app has no OIDC connection contract")?;
        let providers = self.identity_providers()?;
        let provider = providers
            .get(&request.provider)
            .context("Unknown identity provider")?;
        let contract = provider
            .oidc_provider
            .as_ref()
            .context("Missing provider contract")?;
        let issuer = secure_origin(&request.issuer)?;
        let app = secure_origin(&request.app_url)?;
        ensure!(
            client.callback_path.starts_with('/') && !client.callback_path.starts_with("//"),
            "Invalid callback path"
        );
        ensure!(
            contract.create_path.starts_with('/') && !contract.create_path.starts_with("//"),
            "Invalid provider API path"
        );
        let callback = app.join(&client.callback_path)?;
        ensure!(callback.origin() == app.origin(), "Callback changed origin");
        ensure!(
            !request.name.trim().is_empty() && request.name.len() <= 100,
            "Provide a login name"
        );
        ensure!(
            request.provider_project.len() <= 200
                && (request.creates_project() || !request.provider_project.is_empty()),
            "Choose a provider project or create a new one"
        );
        if request.creates_project() {
            ensure!(
                !request.project_name.trim().is_empty() && request.project_name.len() <= 100,
                "Provide a project name"
            );
            ensure!(
                crate::identity_setup::registration_profile(&request.provider)?
                    .project_creation
                    .is_some(),
                "This provider requires an existing project"
            );
        }
        ensure!(
            request.administrator_subject.is_empty()
                || (!client.administrator_environment.is_empty()
                    && request.administrator_subject.len() <= 200),
            "This app has no verified administrator mapping"
        );
        // Subject IDs are embedded in a declarative JMESPath literal. Reject syntax, not just shell characters.
        ensure!(
            request
                .administrator_subject
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:@".contains(&b)),
            "Administrator subject contains unsupported characters"
        );
        let environment_keys: Vec<_> = client
            .environment
            .keys()
            .chain(client.environment_json.keys())
            .chain(
                if request.administrator_subject.is_empty() {
                    None
                } else {
                    Some(client.administrator_environment.keys())
                }
                .into_iter()
                .flatten(),
            )
            .collect();
        if !environment_keys.is_empty() {
            let setup = self.setup(id)?;
            let existing =
                crate::integrations::compose_environment(&setup.compose["services"][service])?;
            ensure!(
                request.replace_existing
                    || !environment_keys.iter().any(|k| existing.contains_key(*k)),
                "Existing login environment settings would be replaced; review them and explicitly allow replacement"
            );
        }
        let details = json!({"provider":request.provider,"issuer":issuer.as_str().trim_end_matches('/'),"provider_project":request.provider_project,"create_project":request.creates_project(),"project_name":request.project_name,"organization_id":request.organization_id,"callback":callback.as_str(),"name":request.name,"service":service,"action":client.action,"server":project.server_id,"administrator_subject":request.administrator_subject,"administrator_role":client.administrator_role,"environment_keys":environment_keys,"replace_existing":request.replace_existing,"restart_required":client.has_environment(),"warnings":profile.warnings});
        use sha2::{Digest, Sha256};
        let revision = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &details,
                &client,
                &contract,
                (request.creates_project() || !request.administrator_subject.is_empty())
                    .then(|| crate::identity_setup::registration_profile(&request.provider))
                    .transpose()?,
                &project,
                &self.setup(id)?,
                &self.server(&self.project(id)?.server_id)?,
                &request.ca_certificate,
                self.standalone.as_ref().map(|d| d.revision()).transpose()?
            ))?)
        );
        Ok(json!({"details":details,"revision":revision}))
    }
    pub async fn connection_account(
        &self,
        id: &str,
        service: &str,
        request: &ConnectRequest,
    ) -> Result<Value> {
        let profile = self.integration(id, service)?;
        let client = profile
            .oidc_client
            .context("This app has no OIDC connection contract")?;
        let registration = crate::identity_setup::RegistrationRequest {
            provider: request.provider.clone(),
            issuer: request.issuer.clone(),
            provider_project: request.provider_project.clone(),
            create_project: request.create_project,
            project_name: request.project_name.clone(),
            organization_id: request.organization_id.clone(),
            selfhost_url: request.app_url.clone(),
            id: "app".into(),
            name: request.name.clone(),
            admin_subjects: vec![],
            ca_certificate: request.ca_certificate.clone(),
        };
        let mut account =
            crate::identity_setup::registration_account(&registration, &request.credential).await?;
        account["administrator_role"] = json!(client.administrator_role);
        Ok(account)
    }
    pub(crate) fn connection_source_revision(&self, id: &str) -> Result<String> {
        use sha2::{Digest, Sha256};
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &self.project(id)?,
                &self.setup(id)?,
                &self.server(&self.project(id)?.server_id)?,
                self.standalone.as_ref().map(|d| d.revision()).transpose()?
            ))?)
        ))
    }
    pub async fn connection_create(
        &self,
        id: &str,
        service: &str,
        request: ConnectRequest,
    ) -> Result<Value> {
        let dir = self.project_dir(id)?.join("connections");
        private_dir(&dir)?;
        ensure!(crate::setup::slug(service), "Invalid service");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(format!("{service}.lock")))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("A connection operation is already running")?;
        let (plan, source_revision) = {
            let _project_lock = self.project_lock(id)?;
            (
                self.connection_plan(id, service, &request)?,
                self.connection_source_revision(id)?,
            )
        };
        ensure!(
            plan["revision"].as_str() == Some(&request.revision),
            "Review a fresh connection preview"
        );
        ensure!(
            !request.credential.is_empty() && request.credential.len() <= 8192,
            "Provide an identity-provider access token"
        );
        let path = dir.join(format!("{service}.json"));
        let profile = self.integration(id, service)?;
        let provider = self
            .identity_providers()?
            .remove(&request.provider)
            .context("Provider missing")?
            .oidc_provider
            .unwrap();
        let issuer = secure_origin(&request.issuer)?;
        let endpoint = issuer.join(&provider.create_path)?;
        ensure!(
            endpoint.origin() == issuer.origin(),
            "Provider endpoint changed origin"
        );
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30));
        if !request.ca_certificate.is_empty() {
            for cert in reqwest::Certificate::from_pem_bundle(request.ca_certificate.as_bytes())? {
                builder = builder.add_root_certificate(cert);
            }
        }
        let http = builder.build()?;
        let discovery_url = issuer.join("/.well-known/openid-configuration")?;
        let discovery = bounded_json(
            http.get(discovery_url)
                .send()
                .await
                .context("Could not read identity provider discovery")?,
        )
        .await?;
        ensure!(
            discovery["issuer"]
                .as_str()
                .map(|s| s.trim_end_matches('/'))
                == Some(issuer.as_str().trim_end_matches('/')),
            "Discovery issuer does not match the selected provider"
        );
        if !request.administrator_subject.is_empty() {
            let account = self.connection_account(id, service, &request).await?;
            ensure!(
                account["human"] == true
                    && account["suggested_subject"].as_str()
                        == Some(&request.administrator_subject),
                "Administrator must be the verified human account owning this credential; machine accounts cannot be administrators"
            );
        }
        let mut record: Connection = if path.exists() {
            let saved: Connection = serde_json::from_slice(&std::fs::read(&path)?)?;
            ensure!(
                saved.stage == "project_created" && saved.revision == request.revision,
                "A connection record already exists with an uncertain or completed outcome. Resume app configuration, or inspect its recorded provider IDs before recovery"
            );
            saved
        } else {
            Connection {
                service: service.into(),
                server_id: self.project(id)?.server_id,
                issuer: issuer.as_str().trim_end_matches('/').into(),
                provider_project: if request.creates_project() {
                    format!("selfhost-{}", token(12)?)
                } else {
                    request.provider_project.clone()
                },
                callback: plan["details"]["callback"].as_str().unwrap().into(),
                name: request.name.clone(),
                application_id: format!("selfhost-{}", token(12)?),
                stage: "prepared".into(),
                client_id: String::new(),
                client_secret: String::new(),
                client: profile.oidc_client.unwrap(),
                administrator_subject: request.administrator_subject.clone(),
                environment: BTreeMap::new(),
                revision: request.revision.clone(),
                source_revision,
            }
        };
        if request.creates_project() && record.stage == "prepared" {
            let creation = crate::identity_setup::registration_profile(&request.provider)?
                .project_creation
                .context("Project creation unavailable")?;
            let organization = if request.organization_id.is_empty() {
                self.connection_account(id, service, &request).await?["organization_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .context("Provider organization missing; set its ID in advanced settings")?
                    .to_owned()
            } else {
                request.organization_id.clone()
            };
            let variables = BTreeMap::from([
                ("project-id".into(), json!(record.provider_project)),
                ("project-name".into(), json!(request.project_name)),
                ("organization-id".into(), json!(organization)),
            ]);
            let endpoint = issuer.join(&creation.create_path)?;
            ensure!(
                endpoint.origin() == issuer.origin(),
                "Project endpoint changed origin"
            );
            let mut post = http.post(endpoint).bearer_auth(&request.credential).json(
                &crate::identity_setup::render_registration(&creation.body, &variables)?,
            );
            for (key, value) in &creation.headers {
                post = post.header(key, value);
            }
            record.stage = "project_creation_uncertain".into();
            atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
            let response = bounded_json(post.send().await.context("Project creation outcome is uncertain; inspect recorded project ID before recovery")?).await?;
            record.provider_project = response
                .pointer(&creation.project_id_pointer)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .context("Project ID missing; inspect provider before recovery")?
                .into();
            record.stage = "project_created".into();
            atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
        }
        let variables = BTreeMap::from([
            ("project-id".into(), json!(record.provider_project)),
            ("application-id".into(), json!(record.application_id)),
            ("name".into(), json!(request.name)),
            ("redirect-uri".into(), json!(record.callback)),
            (
                "loopback-development".into(),
                json!(secure_origin(&request.app_url)?.scheme() == "http"),
            ),
        ]);
        let body = crate::identity_setup::render_registration(&provider.body, &variables)?;
        let mut post = http.post(endpoint).json(&body);
        for (key, value) in &provider.headers {
            post = post.header(key, value);
        }
        record.environment = connection_environment(&record, &request.app_url, &discovery)?;
        record.stage = "creation_uncertain".into();
        // Persist BEFORE the request: a lost reply must never trigger a second registration.
        atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
        let response = post.bearer_auth(&request.credential).send().await.context("Client creation outcome is unknown. Inspect the recorded application ID at the provider before recovery")?;
        let result = bounded_json(response).await.context("Client creation did not complete. Inspect the connection record and provider before recovery")?;
        // Preserve the one-time response privately even if a future provider schema changes.
        atomic_write(
            &dir.join(format!("{}-response.json", record.application_id)),
            &serde_json::to_vec_pretty(&result)?,
        )?;
        record.client_id = result
            .pointer(&provider.client_id_pointer)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .context("Provider did not return a client ID; response saved privately")?
            .into();
        record.client_secret = result
            .pointer(&provider.client_secret_pointer)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .context("Provider did not return a client secret; response saved privately")?
            .into();
        record.environment = connection_environment(&record, &request.app_url, &discovery)?;
        record.stage = "client_created".into();
        atomic_write(&path, &serde_json::to_vec_pretty(&record)?)?;
        self.configure_connection(id, &path, &mut record).await
    }
    pub async fn connection_resume(&self, id: &str, service: &str) -> Result<Value> {
        ensure!(crate::setup::slug(service), "Invalid service");
        self.project_docker(&self.project(id)?, true)?;
        let dir = self.project_dir(id)?.join("connections");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(format!("{service}.lock")))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("A connection operation is already running")?;
        let path = dir.join(format!("{service}.json"));
        let mut record: Connection = serde_json::from_slice(&std::fs::read(&path)?)?;
        ensure!(
            matches!(
                record.stage.as_str(),
                "client_created"
                    | "configuring_app"
                    | "app_configuration_failed"
                    | "awaiting_login_test"
            ) && !record.client_secret.is_empty(),
            "Creation outcome is unknown. Inspect the recorded application ID in your provider. Do not create another client blindly."
        );
        self.configure_connection(id, &path, &mut record).await
    }
    async fn configure_connection(
        &self,
        id: &str,
        path: &std::path::Path,
        record: &mut Connection,
    ) -> Result<Value> {
        let profile = self.integration(id, &record.service)?;
        ensure!(
            self.project(id)?.server_id == record.server_id,
            "The app moved to another server; review its connection before recovery"
        );
        ensure!(
            serde_json::to_value(&profile.oidc_client)? == serde_json::to_value(&record.client)?,
            "App connection contract changed; review before recovery"
        );
        if record.client.has_environment() {
            let _lock = self.project_lock(id)?;
            ensure!(
                record.source_revision == self.connection_source_revision(id)?,
                "App files changed since connection preview; review the saved client and configuration before recovery"
            );
            let mut setup = self.setup(id)?;
            let mut values = crate::integrations::compose_environment(
                &setup.compose["services"][&record.service],
            )?;
            for (key, value) in &record.environment {
                values.insert(key.clone(), value.replace('$', "$$"));
            }
            setup.compose["services"][&record.service]["environment"] = json!(values);
            record.stage = "configuring_app".into();
            atomic_write(path, &serde_json::to_vec_pretty(record)?)?;
            self.save_setup_locked(id, setup)?;
            record.source_revision = self.connection_source_revision(id)?;
            record.stage = "awaiting_login_test".into();
            atomic_write(path, &serde_json::to_vec_pretty(record)?)?;
            return Ok(
                json!({"connection":record.public(),"action":{"ok":true,"restart_required":true,"message":"Login settings saved. Recreate this service and test sign-in. Check this app's recovery options before ending your current session."}}),
            );
        }
        let variables = BTreeMap::from([
            ("name".into(), json!(record.name)),
            ("client-id".into(), json!(record.client_id)),
            ("client-secret".into(), json!(record.client_secret)),
            (
                "discovery-url".into(),
                json!(format!(
                    "{}/.well-known/openid-configuration",
                    record.issuer
                )),
            ),
        ]);
        let inputs = record
            .client
            .inputs
            .iter()
            .map(|(k, v)| Ok((k.clone(), json!(expand(v, &variables)?))))
            .collect::<Result<_>>()?;
        record.stage = "configuring_app".into();
        atomic_write(path, &serde_json::to_vec_pretty(record)?)?;
        let result = Box::pin(self.integration_action_checked(
            id,
            &record.service,
            &record.client.action,
            inputs,
            (!record.source_revision.is_empty()).then_some(record.source_revision.as_str()),
        ))
        .await;
        record.stage = if result.as_ref().is_ok_and(|r| r["ok"] == true) {
            "awaiting_login_test"
        } else {
            "app_configuration_failed"
        }
        .into();
        atomic_write(path, &serde_json::to_vec_pretty(record)?)?;
        Ok(
            json!({"connection":record.public(),"action":result.unwrap_or_else(|_|json!({"ok":false,"message":"App configuration failed. The client is saved; resume after fixing the app."}))}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_oidc_profiles_only_require_declared_discovery_endpoints() {
        let gotify: Profile =
            serde_json::from_str(include_str!("../catalog/integrations/gotify.json")).unwrap();
        let mut connection: Connection = serde_json::from_value(json!({"service":"gotify","server_id":"local","issuer":"https://id.example.com","provider_project":"fixture","callback":"https://push.example.com/auth/oidc/callback","name":"Selfhost","application_id":"app","stage":"created","client_id":"client","client_secret":"fixture-secret","client":gotify.oidc_client})).unwrap();
        let output =
            connection_environment(&connection, "https://push.example.com", &json!({})).unwrap();
        assert_eq!(output["GOTIFY_OIDC_ISSUER"], "https://id.example.com");
        assert_eq!(output["GOTIFY_OIDC_REDIRECTURL"], connection.callback);
        let linkding: Profile =
            serde_json::from_str(include_str!("../catalog/integrations/linkding.json")).unwrap();
        connection.client = linkding.oidc_client.unwrap();
        let mut discovery = json!({"authorization_endpoint":"https://id.example.com/auth","token_endpoint":"https://id.example.com/token","userinfo_endpoint":"https://id.example.com/userinfo"});
        assert!(
            connection_environment(&connection, "https://links.example.com", &discovery).is_err()
        );
        discovery["jwks_uri"] = json!("https://id.example.com/keys");
        assert_eq!(
            connection_environment(&connection, "https://links.example.com", &discovery).unwrap()["OIDC_OP_JWKS_ENDPOINT"],
            "https://id.example.com/keys"
        );
        discovery["jwks_uri"] = json!("http://id.example.com/keys");
        assert!(
            connection_environment(&connection, "https://links.example.com", &discovery).is_err()
        );
    }
    #[tokio::test]
    async fn provisioning_saves_progress_and_resume_never_creates_a_second_client() {
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
        let discovery_issuer = issuer.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let router=Router::new().route("/.well-known/openid-configuration",get(move||{let issuer=discovery_issuer.clone();async move{Json(json!({"issuer":issuer}))}})).route("/zitadel.application.v2.ApplicationService/CreateApplication",post(move|headers:HeaderMap,Json(body):Json<Value>|{let calls=calls.clone();async move{
            assert_eq!(headers["authorization"],"Bearer ephemeral-test-token");assert_eq!(headers["connect-protocol-version"],"1");
            assert_eq!(body["projectId"],"test-project");assert_eq!(body["oidcConfiguration"]["redirectUris"],json!(["https://files.example.com/index.php/apps/user_oidc/code"]));
            assert_eq!(body["oidcConfiguration"]["authMethodType"],"OIDC_AUTH_METHOD_TYPE_BASIC");
            assert!(body["applicationId"].as_str().unwrap().starts_with("selfhost-"));calls.fetch_add(1,Ordering::SeqCst);
            Json(json!({"applicationId":body["applicationId"],"oidcConfiguration":{"clientId":"saved-client","clientSecret":"one-time-test-secret"}}))
        }}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let server_config = store
            .add_server(crate::infrastructure::Server {
                id: String::new(),
                name: "Missing test engine".into(),
                provider: crate::infrastructure::Provider::DockerContext,
                endpoint: "selfhost-test-missing".into(),
                group: String::new(),
                read_only: false,
                app_host: String::new(),
            })
            .unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Connection test".into(),
                apps: vec!["nextcloud".into()],
                server_id: server_config.id,
            })
            .unwrap();
        let mut request = ConnectRequest {
            provider: "zitadel".into(),
            issuer,
            provider_project: "test-project".into(),
            create_project: false,
            project_name: "Selfhost".into(),
            organization_id: String::new(),
            administrator_subject: String::new(),
            replace_existing: false,
            ca_certificate: String::new(),
            app_url: "https://files.example.com".into(),
            name: "Test login".into(),
            credential: "ephemeral-test-token".into(),
            revision: String::new(),
        };
        request.revision = store
            .connection_plan(&project.id, "nextcloud", &request)
            .unwrap()["revision"]
            .as_str()
            .unwrap()
            .into();
        let result = Box::pin(store.connection_create(&project.id, "nextcloud", request))
            .await
            .unwrap();
        assert_eq!(result["connection"]["stage"], "app_configuration_failed");
        assert!(!result.to_string().contains("one-time-test-secret"));
        let saved = std::fs::read_to_string(
            store
                .project_dir(&project.id)
                .unwrap()
                .join("connections/nextcloud.json"),
        )
        .unwrap();
        assert!(saved.contains("one-time-test-secret"));
        assert!(!saved.contains("ephemeral-test-token"));
        Box::pin(store.connection_resume(&project.id, "nextcloud"))
            .await
            .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        // A reviewed login connection must not silently apply to changed source files.
        let mut edited = store.setup(&project.id).unwrap();
        edited
            .environment
            .insert("UNRELATED_CHANGE".into(), "changed".into());
        store.save_setup(&project.id, edited).unwrap();
        let original: Connection = serde_json::from_str(&saved).unwrap();
        let error = Box::pin(store.integration_action_checked(
            &project.id,
            "nextcloud",
            "status",
            BTreeMap::new(),
            Some(&original.source_revision),
        ))
        .await
        .unwrap_err();
        assert!(error.to_string().contains("App files or server changed"));
        let refused = Box::pin(store.connection_resume(&project.id, "nextcloud"))
            .await
            .unwrap();
        assert_eq!(refused["action"]["ok"], false);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let path = store
            .project_dir(&project.id)
            .unwrap()
            .join("connections/nextcloud.json");
        let mut record: Connection = serde_json::from_str(&saved).unwrap();
        record.stage = "creation_uncertain".into();
        atomic_write(&path, &serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            Box::pin(store.connection_resume(&project.id, "nextcloud"))
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        server.abort();
    }
    #[tokio::test]
    async fn grafana_default_project_selects_verified_human_and_writes_portable_environment() {
        use axum::{
            Json, Router,
            routing::{get, post},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let discover = issuer.clone();
        let projects = Arc::new(AtomicUsize::new(0));
        let p = projects.clone();
        let clients = Arc::new(AtomicUsize::new(0));
        let c = clients.clone();
        let router=Router::new()
            .route("/.well-known/openid-configuration",get(move||{let issuer=discover.clone();async move{Json(json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"userinfo_endpoint":format!("{issuer}/userinfo")}))}}))
            .route("/auth/v1/users/me",get(||async{Json(json!({"user":{"id":"human-owner","human":{},"details":{"resourceOwner":"organization"}}}))}))
            .route("/zitadel.project.v2.ProjectService/CreateProject",post(move|Json(body):Json<Value>|{let p=p.clone();async move{assert_eq!(body["name"],"Selfhost");assert_eq!(body["organizationId"],"organization");p.fetch_add(1,Ordering::SeqCst);Json(json!({"projectId":"dedicated-project"}))}}))
            .route("/zitadel.application.v2.ApplicationService/CreateApplication",post(move|Json(body):Json<Value>|{let c=c.clone();async move{assert_eq!(body["projectId"],"dedicated-project");assert_eq!(body["oidcConfiguration"]["redirectUris"],json!(["http://localhost:3000/login/generic_oauth"]));assert_eq!(body["oidcConfiguration"]["developmentMode"],true);c.fetch_add(1,Ordering::SeqCst);Json(json!({"oidcConfiguration":{"clientId":"client","clientSecret":"literal-$secret"}}))}}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Metrics".into(),
                apps: vec!["grafana".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let before = store.setup(&project.id).unwrap();
        let mut request:ConnectRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":issuer,"app_url":"http://localhost:3000","name":"Home","administrator_subject":"human-owner","credential":"ephemeral"})).unwrap();
        let account = store
            .connection_account(&project.id, "grafana", &request)
            .await
            .unwrap();
        assert_eq!(account["human"], true);
        assert_eq!(
            account["administrator_role"],
            "Grafana organization administrator"
        );
        request.revision = store
            .connection_plan(&project.id, "grafana", &request)
            .unwrap()["revision"]
            .as_str()
            .unwrap()
            .into();
        let result = Box::pin(store.connection_create(&project.id, "grafana", request))
            .await
            .unwrap();
        assert_eq!(result["action"]["restart_required"], true);
        assert!(!result.to_string().contains("literal-$secret"));
        let after = store.setup(&project.id).unwrap();
        assert_eq!(before.environment, after.environment);
        let env = &after.compose["services"]["grafana"]["environment"];
        assert_eq!(
            env["GF_AUTH_GENERIC_OAUTH_ROLE_ATTRIBUTE_PATH"],
            "sub == 'human-owner' && 'Admin' || 'Viewer'"
        );
        assert_eq!(
            env["GF_AUTH_GENERIC_OAUTH_CLIENT_SECRET"],
            "literal-$$secret"
        );
        assert_eq!(env["GF_AUTH_DISABLE_LOGIN_FORM"], "false");
        assert_eq!(
            env["GF_AUTH_GENERIC_OAUTH_ALLOW_ASSIGN_GRAFANA_ADMIN"],
            "false"
        );
        assert_eq!(
            before.compose["services"]["grafana"]["environment"]["GF_SECURITY_ADMIN_PASSWORD"],
            env["GF_SECURITY_ADMIN_PASSWORD"]
        );
        Box::pin(store.connection_resume(&project.id, "grafana"))
            .await
            .unwrap();
        assert_eq!(projects.load(Ordering::SeqCst), 1);
        assert_eq!(clients.load(Ordering::SeqCst), 1);
        let record = std::fs::read_to_string(
            store
                .project_dir(&project.id)
                .unwrap()
                .join("connections/grafana.json"),
        )
        .unwrap();
        assert!(!record.contains("ephemeral"));
        server.abort();
    }
    #[tokio::test]
    async fn connection_rejects_machine_administrator_before_creating_resources() {
        use axum::{Json, Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let i = issuer.clone();
        let router=Router::new().route("/.well-known/openid-configuration",get(move||{let i=i.clone();async move{Json(json!({"issuer":i}))}})).route("/auth/v1/users/me",get(||async{Json(json!({"user":{"id":"machine","machine":{},"details":{"resourceOwner":"org"}}}))}));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Metrics".into(),
                apps: vec!["grafana".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let mut request:ConnectRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":issuer,"app_url":"http://localhost:3000","name":"Home","administrator_subject":"machine","credential":"ephemeral"})).unwrap();
        request.revision = store
            .connection_plan(&project.id, "grafana", &request)
            .unwrap()["revision"]
            .as_str()
            .unwrap()
            .into();
        let error = Box::pin(store.connection_create(&project.id, "grafana", request))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("verified human"));
        assert!(
            !store
                .project_dir(&project.id)
                .unwrap()
                .join("connections/grafana.json")
                .exists()
        );
        server.abort();
    }
    #[tokio::test]
    async fn uncertain_project_creation_is_never_retried() {
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
        let i = issuer.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let router = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(move || {
                    let i = i.clone();
                    async move { Json(json!({"issuer":i})) }
                }),
            )
            .route(
                "/zitadel.project.v2.ProjectService/CreateProject",
                post(move || {
                    let c = c.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Files".into(),
                apps: vec!["nextcloud".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let mut request:ConnectRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":issuer,"app_url":"http://localhost:8080","name":"Home","organization_id":"org","credential":"ephemeral"})).unwrap();
        request.revision = store
            .connection_plan(&project.id, "nextcloud", &request)
            .unwrap()["revision"]
            .as_str()
            .unwrap()
            .into();
        assert!(
            Box::pin(store.connection_create(&project.id, "nextcloud", request.clone()))
                .await
                .is_err()
        );
        assert_eq!(
            store.connection_state(&project.id, "nextcloud").unwrap()["stage"],
            "project_creation_uncertain"
        );
        assert!(
            Box::pin(store.connection_create(&project.id, "nextcloud", request))
                .await
                .is_err()
        );
        assert!(
            Box::pin(store.connection_resume(&project.id, "nextcloud"))
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        server.abort();
    }
    #[test]
    fn connection_defaults_and_existing_setting_guards() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let project = store
            .create(crate::core::CreateProject {
                name: "Metrics".into(),
                apps: vec!["grafana".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let mut request:ConnectRequest=serde_json::from_value(json!({"provider":"zitadel","issuer":"https://identity.example.com","app_url":"http://localhost:3000","name":"Home"})).unwrap();
        let plan = store
            .connection_plan(&project.id, "grafana", &request)
            .unwrap();
        assert_eq!(plan["details"]["create_project"], true);
        let mut setup = store.setup(&project.id).unwrap();
        setup.compose["services"]["grafana"]["environment"]["GF_AUTH_GENERIC_OAUTH_CLIENT_ID"] =
            json!("existing");
        store.save_setup(&project.id, setup).unwrap();
        assert!(
            store
                .connection_plan(&project.id, "grafana", &request)
                .is_err()
        );
        request.replace_existing = true;
        assert!(
            store
                .connection_plan(&project.id, "grafana", &request)
                .is_ok()
        );
        request.administrator_subject = "owner' || 'Admin".into();
        assert!(
            store
                .connection_plan(&project.id, "grafana", &request)
                .is_err()
        );
    }
    #[test]
    fn origins_and_json_templates_preserve_boundaries() {
        for bad in [
            "http://id.example",
            "https://user:pass@id.example",
            "https://id.example/path",
            "https://id.example?token=x",
            "https://id.example/#x",
        ] {
            assert!(secure_origin(bad).is_err());
        }
        assert!(secure_origin("https://id.example:8443").is_ok());
        let input = BTreeMap::from([("name".into(), json!("literal {{secret}} \" quoted"))]);
        assert_eq!(
            render(&json!({"name":"{{name}}","nested":["{{name}}"]}), &input).unwrap()["name"],
            input["name"]
        );
    }
    #[test]
    fn structured_oidc_environment_escapes_secrets_and_validates_endpoints() {
        let record: Connection = serde_json::from_value(json!({
            "service":"app", "server_id":"local", "issuer":"https://id.example.com",
            "provider_project":"project", "callback":"https://app.example.com/callback",
            "name":"Example", "application_id":"client", "stage":"configured",
            "client_id":"id", "client_secret":"a\"b\\c {{issuer-url}}",
            "client":{"callback_path":"/callback","environment_json":{"AUTH":{"secret":"{{client-secret}}","url":"{{token-url}}"}}}
        })).unwrap();
        let environment = connection_environment(
            &record,
            "https://app.example.com",
            &json!({"token_endpoint":"https://id.example.com/token"}),
        )
        .unwrap();
        let value: Value = serde_json::from_str(&environment["AUTH"]).unwrap();
        assert_eq!(value["secret"], record.client_secret);
        assert_eq!(value["url"], "https://id.example.com/token");
        assert!(
            connection_environment(
                &record,
                "https://app.example.com",
                &json!({"token_endpoint":"http://id.example.com/token"})
            )
            .is_err()
        );
        assert!(connection_environment(&record, "https://app.example.com", &json!({})).is_err());
    }
    #[test]
    fn dozzle_catalogue_oidc_substitutes_actual_client_credentials() {
        let profile: Profile =
            serde_json::from_slice(include_bytes!("../catalog/integrations/dozzle.json")).unwrap();
        let client = profile.oidc_client.unwrap();
        let record: Connection = serde_json::from_value(json!({
            "service":"dozzle", "server_id":"local", "issuer":"https://id.example.com",
            "provider_project":"project", "callback":"https://logs.example.com/api/auth/callback",
            "name":"Logs", "application_id":"client", "stage":"configured",
            "client_id":"actual-client", "client_secret":"secret-with-$-and-{{braces}}", "client":client
        })).unwrap();
        let values =
            connection_environment(&record, "https://logs.example.com", &json!({})).unwrap();
        assert_eq!(values["DOZZLE_AUTH_OIDC_ISSUER"], record.issuer);
        assert_eq!(values["DOZZLE_AUTH_OIDC_CLIENT_ID"], record.client_id);
        assert_eq!(
            values["DOZZLE_AUTH_OIDC_CLIENT_SECRET"],
            record.client_secret
        );
    }
}
