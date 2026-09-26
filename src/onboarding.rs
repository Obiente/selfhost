//! Reviewed, declarative HTTP onboarding. Every remote write is journaled before dispatch.
use crate::core::{Store, atomic_write, private_dir};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
    pub modes: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub minimum_length: usize,
    pub maximum_length: usize,
    #[serde(default)]
    pub characters: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    #[serde(default)]
    pub expect: BTreeMap<String, Value>,
    #[serde(default)]
    pub response_text: bool,
    pub id: String,
    pub label: String,
    pub modes: Vec<String>,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: Value,
    #[serde(default)]
    pub form: bool,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub capture: BTreeMap<String, String>,
    #[serde(default)]
    pub each_app: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub state_text_contains: Option<String>,
    pub schema: u32,
    pub name: String,
    pub fields: Vec<Field>,
    pub state_path: String,
    pub state_pointer: String,
    pub initial_state: String,
    pub board_url: String,
    pub default_icon: String,
    pub app_name_max: usize,
    pub app_description_max: usize,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.steps.len() <= 32,
            "Invalid onboarding profile"
        );
        path(&self.state_path)?;
        ensure!(
            self.state_text_contains
                .as_ref()
                .is_none_or(|marker| !marker.is_empty() && marker.len() <= 512),
            "Invalid initial HTML marker"
        );
        let mut ids = std::collections::HashSet::new();
        for step in &self.steps {
            ensure!(
                crate::setup::slug(&step.id) && ids.insert(&step.id),
                "Invalid onboarding step ID"
            );
            path(&step.path)?;
            ensure!(
                ["GET", "POST"].contains(&step.method.as_str()),
                "Unsupported onboarding method"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppLink {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub description: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnboardingRequest {
    pub mode: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub apps: Vec<AppLink>,
}
#[derive(Default, Serialize, Deserialize)]
struct Receipt {
    started: bool,
    completed: bool,
    pending: Option<String>,
    api_key: String,
    board_url: String,
    apps_added: usize,
    items_added: usize,
    #[serde(default)]
    resources: BTreeMap<String, String>,
    #[serde(default)]
    finished_steps: Vec<String>,
    #[serde(default)]
    linked_urls: Vec<String>,
}
fn path(value: &str) -> Result<()> {
    ensure!(
        value.starts_with('/')
            && !value.starts_with("//")
            && !value.contains(['\\', '\r', '\n', '#']),
        "Use a relative app API path"
    );
    Ok(())
}
fn template(text: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let end = rest.find('}').context("Unclosed onboarding placeholder")?;
        output.push_str(vars.get(&rest[..end]).context("Missing onboarding input")?);
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}
fn render(value: &Value, vars: &BTreeMap<String, String>) -> Result<Value> {
    Ok(match value {
        Value::String(v) => Value::String(template(v, vars)?),
        Value::Array(v) => Value::Array(v.iter().map(|v| render(v, vars)).collect::<Result<_>>()?),
        Value::Object(v) => Value::Object(
            v.iter()
                .map(|(k, v)| Ok((k.clone(), render(v, vars)?)))
                .collect::<Result<_>>()?,
        ),
        v => v.clone(),
    })
}

struct Session {
    client: reqwest::Client,
    base: String,
    cookies: BTreeMap<String, String>,
}
impl Session {
    fn new(base: String) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base,
            cookies: BTreeMap::new(),
        })
    }
    async fn request(&mut self, step: &Step, vars: &BTreeMap<String, String>) -> Result<Value> {
        path(&step.path)?;
        let mut req = self
            .client
            .request(step.method.parse()?, format!("{}{}", self.base, step.path))
            .header("Origin", &self.base);
        if !self.cookies.is_empty() {
            req = req.header(
                "Cookie",
                self.cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        for (k, v) in &step.headers {
            let v = render(&json!(v), vars)?;
            req = req.header(k, v.as_str().context("Invalid header")?);
        }
        if step.method == "POST" {
            let body = render(&step.body, vars)?;
            req = if step.form {
                req.form(body.as_object().context("Form must be an object")?)
            } else {
                req.json(&body)
            };
        }
        let mut response = req.send().await.map_err(|_| {
            anyhow::anyhow!("App request failed; inspect the saved setup status before retrying")
        })?;
        ensure!(
            response.status().is_success(),
            "App rejected setup step {} (HTTP {}); no remote response or credentials are logged",
            step.id,
            response.status().as_u16()
        );
        for h in response.headers().get_all("set-cookie") {
            if let Ok(raw) = h.to_str()
                && let Some((k, v)) = raw.split(';').next().unwrap_or("").split_once('=')
            {
                self.cookies.insert(k.into(), v.into());
            }
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("Could not read app response")?
        {
            ensure!(
                bytes.len() + chunk.len() <= 1024 * 1024,
                "App response exceeds 1 MiB"
            );
            bytes.extend_from_slice(&chunk);
        }
        let value = if step.response_text {
            json!(String::from_utf8(bytes).context("App text response is not UTF-8")?)
        } else if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).context("App response is not JSON")?
        };
        for (pointer, expected) in &step.expect {
            ensure!(
                value.pointer(pointer) == Some(expected),
                "App did not confirm setup step {}; inspect the saved receipt before retrying",
                step.id
            );
        }
        Ok(value)
    }
}
impl Store {
    fn onboarding_profile(&self, id: &str, service: &str) -> Result<Option<Profile>> {
        let p = self.project(id)?;
        let s = p
            .services
            .iter()
            .find(|s| s.app == service)
            .context("Service not found")?;
        Ok(s.definition.onboarding.clone())
    }
    fn onboarding_path(&self, id: &str, service: &str) -> Result<PathBuf> {
        ensure!(crate::setup::slug(service), "Invalid service ID");
        Ok(self
            .project_dir(id)?
            .join("onboarding")
            .join(format!("{service}.json")))
    }
    fn onboarding_receipt(&self, id: &str, service: &str) -> Result<Receipt> {
        let p = self.onboarding_path(id, service)?;
        if p.exists() {
            Ok(serde_json::from_slice(&std::fs::read(p)?)?)
        } else {
            Ok(Receipt::default())
        }
    }
    fn onboarding_save(&self, id: &str, service: &str, r: &Receipt) -> Result<()> {
        let p = self.onboarding_path(id, service)?;
        private_dir(p.parent().unwrap())?;
        atomic_write(&p, &serde_json::to_vec_pretty(r)?)
    }
    pub(crate) fn onboarding_key(&self, id: &str, service: &str) -> Result<String> {
        let receipt = self.onboarding_receipt(id, service)?;
        ensure!(
            receipt.completed,
            "Finish automatic app setup before using its saved API key"
        );
        Ok(receipt.api_key)
    }
    pub fn onboarding_info(&self, id: &str, service: &str) -> Result<Value> {
        let Some(profile) = self.onboarding_profile(id, service)? else {
            return Ok(json!({"supported":false}));
        };
        let r = self.onboarding_receipt(id, service)?;
        let mut suggestions = Vec::new();
        for p in self.read()?.projects {
            for s in p.services {
                if p.id == id && s.app == service {
                    continue;
                }
                let server = self.server(&p.server_id)?;
                if !server.app_host.is_empty() {
                    suggestions.push(json!({"name":s.definition.name,"url":format!("http://{}:{}",server.app_host,s.port),"icon":s.definition.icon,"description":s.definition.description}));
                }
            }
        }
        for app in self.existing_apps()? {
            if let Ok(url) = reqwest::Url::parse(&app.url)
                && ["http", "https"].contains(&url.scheme())
                && url.username().is_empty()
                && url.password().is_none()
            {
                suggestions.push(json!({"name":app.name,"url":app.url,"icon":"","description":"Linked existing application"}));
            }
        }
        Ok(
            json!({"supported":true,"profile":{"name":profile.name,"fields":profile.fields,"supports_sync":profile.steps.iter().any(|s|s.modes.iter().any(|m|m=="sync")),"modes":profile.steps.iter().flat_map(|s|s.modes.clone()).collect::<std::collections::BTreeSet<_>>(),"accepts_apps":profile.steps.iter().any(|s|s.each_app)},"state":{"started":r.started,"has_api_key":!r.api_key.is_empty(),"completed":r.completed,"pending":r.pending,"linked_urls":r.linked_urls},"suggestions":suggestions}),
        )
    }
    pub(crate) fn onboarding_destination_revision(
        &self,
        id: &str,
        service: &str,
    ) -> Result<String> {
        let receipt = self.onboarding_receipt(id, service)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                self.onboarding_profile(id, service)?,
                &receipt.api_key,
                &receipt.board_url,
                receipt.resources.get("board_id:0")
            ))?)
        ))
    }
    /// Resolve the port from the currently running, label-owned container, never from saved metadata alone.
    pub(crate) async fn onboarding_target(
        &self,
        id: &str,
        service: &str,
    ) -> Result<(String, String)> {
        let project = self.project(id)?;
        ensure!(
            project.server_id == "local",
            "Run app setup on the Docker host; remote HTTP onboarding requires a verified tunnel"
        );
        self.project_docker(&project, true)?;
        let selected = project
            .services
            .iter()
            .find(|s| s.app == service)
            .context("Service missing")?;
        let container = self
            .service_container(id, service)
            .await?
            .context("Start the app before setting it up")?;
        let mut cmd = self.project_docker(&project, false)?;
        cmd.args(["inspect", &container]);
        let data: Value = serde_json::from_str(&crate::core::run(cmd, 10).await?)?;
        let c = &data[0];
        ensure!(
            c["State"]["Running"] == true
                && c["Config"]["Labels"]["com.docker.compose.project"] == format!("selfhost-{id}")
                && c["Config"]["Labels"]["com.docker.compose.service"] == service,
            "Container ownership changed"
        );
        ensure!(
            c["Config"]["Image"] == selected.image,
            "Container image differs from the selected deployment"
        );
        let ports = c["NetworkSettings"]["Ports"]
            [format!("{}/tcp", selected.definition.container_port)]
        .as_array()
        .context("App has no published HTTP port")?;
        let binding = ports
            .iter()
            .find(|p| p["HostIp"] == "127.0.0.1" || p["HostIp"] == "0.0.0.0")
            .context("App needs a local IPv4 port binding")?;
        let port: u16 = binding["HostPort"]
            .as_str()
            .context("Missing app port")?
            .parse()?;
        ensure!(
            port == selected.port && port > 0,
            "Container published port differs from the reviewed deployment"
        );
        Ok((
            format!("http://127.0.0.1:{port}"),
            c["Id"].as_str().context("Missing container ID")?.into(),
        ))
    }
    async fn onboarding_prepare(
        &self,
        id: &str,
        service: &str,
        request: &OnboardingRequest,
    ) -> Result<(
        Profile,
        BTreeMap<String, String>,
        Session,
        String,
        String,
        String,
    )> {
        let profile = self
            .onboarding_profile(id, service)?
            .context("App has no automatic setup profile")?;
        profile.validate()?;
        ensure!(
            ["bootstrap", "connect", "sync"].contains(&request.mode.as_str()),
            "Choose bootstrap, connect or sync"
        );
        ensure!(request.apps.len() <= 100, "Choose at most 100 app links");
        let receipt = self.onboarding_receipt(id, service)?;
        ensure!(
            receipt.pending.is_none(),
            "Setup already started and a remote write is uncertain; inspect the app and private receipt before retrying"
        );
        ensure!(
            profile
                .steps
                .iter()
                .any(|s| s.modes.contains(&request.mode)),
            "This app does not support the selected setup mode"
        );
        ensure!(
            request.apps.is_empty() || profile.steps.iter().any(|s| s.each_app),
            "This app does not accept service links"
        );
        ensure!(
            !receipt.started || (request.mode == "sync" && receipt.completed),
            "Setup already started; inspect its private receipt and app before any further changes"
        );
        let mut vars = BTreeMap::new();
        if request.mode == "sync" {
            ensure!(
                receipt.completed && !receipt.api_key.is_empty(),
                "Complete app setup before synchronizing links"
            );
            vars.insert("api_key".into(), receipt.api_key.clone());
            vars.insert(
                "board_id".into(),
                receipt
                    .resources
                    .get("board_id:0")
                    .context("Saved board missing")?
                    .clone(),
            );
        }
        let mut urls: std::collections::HashSet<_> = receipt.linked_urls.iter().cloned().collect();
        for app in &request.apps {
            ensure!(
                urls.insert(app.url.clone()),
                "This service URL was already linked or appears twice; review the dashboard before adding it again"
            );
        }
        for field in &profile.fields {
            if !field.modes.contains(&request.mode) {
                continue;
            }
            let v = request
                .inputs
                .get(&field.id)
                .or(field.default.as_ref())
                .cloned()
                .unwrap_or_default();
            ensure!(
                !field.required || !v.trim().is_empty(),
                "Enter {}",
                field.label
            );
            ensure!(
                v.encode_utf16().count() <= field.maximum_length && !v.contains(['\r', '\n', '\0']),
                "Invalid {}",
                field.label
            );
            ensure!(
                v.encode_utf16().count() >= field.minimum_length
                    && (field.characters.is_empty()
                        || v.chars().all(|c| field.characters.contains(c))),
                "Invalid {}",
                field.label
            );
            vars.insert(field.id.clone(), v);
        }
        for key in request.inputs.keys() {
            ensure!(
                profile.fields.iter().any(|f| &f.id == key),
                "Unknown setup input"
            );
        }
        for app in &request.apps {
            ensure!(
                !app.name.trim().is_empty()
                    && app.name.encode_utf16().count() <= profile.app_name_max
                    && app.description.encode_utf16().count() <= profile.app_description_max,
                "Invalid app link"
            );
            for link in [&app.url, &app.icon] {
                if link.is_empty() {
                    continue;
                }
                let url = reqwest::Url::parse(link)?;
                ensure!(
                    ["http", "https"].contains(&url.scheme())
                        && url.username().is_empty()
                        && url.password().is_none(),
                    "App links must be HTTP URLs without credentials"
                );
            }
        }
        let (base, container) = self.onboarding_target(id, service).await?;
        vars.insert("base".into(), base.clone());
        let mut session = Session::new(base)?;
        let state = session
            .request(
                &Step {
                    expect: BTreeMap::new(),
                    response_text: profile.state_text_contains.is_some(),
                    id: "inspect".into(),
                    label: String::new(),
                    modes: vec![],
                    method: "GET".into(),
                    path: profile.state_path.clone(),
                    body: Value::Null,
                    form: false,
                    headers: BTreeMap::new(),
                    capture: BTreeMap::new(),
                    each_app: false,
                },
                &vars,
            )
            .await?;
        let status = if let Some(marker) = &profile.state_text_contains {
            if state.as_str().is_some_and(|text| text.contains(marker)) {
                profile.initial_state.clone()
            } else {
                "configured".into()
            }
        } else {
            state
                .pointer(&profile.state_pointer)
                .map(|v| match v {
                    Value::String(v) => v.clone(),
                    Value::Bool(v) => v.to_string(),
                    _ => "configured".into(),
                })
                .unwrap_or_else(|| "configured".into())
        };
        if request.mode == "bootstrap" {
            ensure!(
                status == profile.initial_state,
                "This app has already begun setup. Finish in the app, then connect using its API key"
            );
        }
        let mut reviewed = request.clone();
        reviewed.inputs.retain(|k, _| {
            profile
                .fields
                .iter()
                .any(|f| &f.id == k && f.kind != "secret")
        });
        let revision = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &container,
                &profile,
                &reviewed,
                &status,
                self.setup(id)?,
                &receipt.finished_steps,
                &receipt.linked_urls
            ))?)
        );
        Ok((profile, vars, session, revision, status, container))
    }
    pub async fn onboarding_plan(
        &self,
        id: &str,
        service: &str,
        request: OnboardingRequest,
    ) -> Result<Value> {
        let (profile, _, _, revision, status, _) =
            self.onboarding_prepare(id, service, &request).await?;
        Ok(
            json!({"revision":revision,"status":status,"steps":profile.steps.iter().filter(|s|s.modes.contains(&request.mode)).map(|s|&s.label).collect::<Vec<_>>(),"apps":request.apps,"warnings":profile.warnings}),
        )
    }
    pub async fn onboarding_apply(
        &self,
        id: &str,
        service: &str,
        request: OnboardingRequest,
        revision: &str,
    ) -> Result<Value> {
        let _lock = self.project_lock(id)?;
        self.onboarding_apply_locked(id, service, request, revision)
            .await
    }
    /// Caller holds the project operation lock, including any recurring-task destination recheck.
    pub(crate) async fn onboarding_apply_locked(
        &self,
        id: &str,
        service: &str,
        request: OnboardingRequest,
        revision: &str,
    ) -> Result<Value> {
        let (profile, mut vars, mut session, current, _, container) =
            self.onboarding_prepare(id, service, &request).await?;
        ensure!(
            current == revision,
            "App or configuration changed; review a fresh setup plan"
        );
        let mut receipt = if request.mode == "sync" {
            self.onboarding_receipt(id, service)?
        } else {
            Receipt::default()
        };
        receipt.started = true;
        receipt.completed = false;
        self.onboarding_save(id, service, &receipt)?;
        let mut app_vars = vec![BTreeMap::<String, String>::new(); request.apps.len().max(1)];
        for step in profile
            .steps
            .iter()
            .filter(|s| s.modes.contains(&request.mode))
        {
            let count = if step.each_app { request.apps.len() } else { 1 };
            for (index, app_variables) in app_vars.iter_mut().enumerate().take(count) {
                if step.each_app {
                    vars.extend(app_variables.clone());
                    let app = &request.apps[index];
                    for (k, v) in [
                        ("name", &app.name),
                        ("url", &app.url),
                        (
                            "icon",
                            if app.icon.is_empty() {
                                &profile.default_icon
                            } else {
                                &app.icon
                            },
                        ),
                        ("description", &app.description),
                    ] {
                        vars.insert(k.into(), v.clone());
                    }
                }
                receipt.pending = Some(format!("{}:{index}", step.id));
                self.onboarding_save(id, service, &receipt)?;
                // Recheck the Docker-owned endpoint before every request carrying credentials.
                let (base, current_container) = self.onboarding_target(id, service).await?;
                ensure!(
                    base == session.base && current_container == container,
                    "App endpoint changed during setup"
                );
                let result = session.request(step, &vars).await?;
                for (k, pointer) in &step.capture {
                    let value=result.pointer(pointer).and_then(Value::as_str).context("App did not return a required setup value; inspect the private setup status")?;
                    vars.insert(k.clone(), value.into());
                    if k.ends_with("_id") {
                        receipt
                            .resources
                            .insert(format!("{k}:{index}"), value.into());
                    }
                    if step.each_app {
                        app_variables.insert(k.clone(), value.into());
                    }
                }
                if let Some(key) = vars.get("api_key") {
                    receipt.api_key = key.clone();
                }
                if vars.contains_key("board_id") && request.mode != "sync" {
                    receipt.board_url = render(&json!(profile.board_url), &vars)?
                        .as_str()
                        .unwrap_or("")
                        .into();
                }
                if step.each_app && step.capture.contains_key("app_id") {
                    receipt.apps_added += 1;
                }
                if step.each_app && !step.capture.contains_key("app_id") {
                    receipt.items_added += 1;
                }
                receipt.finished_steps.push(format!("{}:{index}", step.id));
                receipt.pending = None;
                self.onboarding_save(id, service, &receipt)?;
            }
        }
        receipt
            .linked_urls
            .extend(request.apps.iter().map(|app| app.url.clone()));
        receipt.completed = true;
        if receipt.board_url.is_empty() && !profile.board_url.is_empty() {
            receipt.board_url = template(&profile.board_url, &vars)?;
        }
        self.onboarding_save(id, service, &receipt)?;
        Ok(
            json!({"completed":true,"board_url":receipt.board_url,"apps_added":receipt.apps_added,"items_added":receipt.items_added,"has_api_key":!receipt.api_key.is_empty()}),
        )
    }
}
/// CLI secrets can be kept out of JSON using `env:VARIABLE_NAME` input values.
pub fn read_request(path: &std::path::Path) -> Result<OnboardingRequest> {
    let mut request: OnboardingRequest = serde_json::from_slice(&std::fs::read(path)?)?;
    for value in request.inputs.values_mut() {
        if let Some(key) = value.strip_prefix("env:") {
            ensure!(
                !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "Invalid input environment variable"
            );
            *value =
                std::env::var(key).context("Set the requested setup input environment variable")?;
        }
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CreateProject, EditProject, EditService};
    #[tokio::test]
    async fn setup_responses_support_html_and_reject_semantic_failure() {
        use axum::{Json, Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/html", get(|| async { "<title>Setup</title>" }))
                    .route(
                        "/error",
                        get(|| async { Json(json!({"ok":false,"error":"private sentinel"})) }),
                    )
                    .route("/ok", get(|| async { Json(json!({"ok":true})) })),
            )
            .await
            .unwrap();
        });
        let mut session = Session::new(base).unwrap();
        let mut step: Step = serde_json::from_value(json!({"id":"setup","label":"Setup","modes":["bootstrap"],"method":"GET","path":"/html","response_text":true})).unwrap();
        assert_eq!(
            session.request(&step, &BTreeMap::new()).await.unwrap(),
            json!("<title>Setup</title>")
        );
        step.response_text = false;
        step.path = "/error".into();
        step.expect.insert("/ok".into(), json!(true));
        let error = session
            .request(&step, &BTreeMap::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("did not confirm"));
        assert!(!error.contains("private sentinel"));
        step.path = "/ok".into();
        assert!(session.request(&step, &BTreeMap::new()).await.is_ok());
        server.abort();
    }
    #[tokio::test]
    async fn completed_receipt_with_uncertain_write_cannot_sync_again() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Fixture".into(),
                apps: vec!["homarr".into()],
            })
            .unwrap();
        store
            .onboarding_save(
                &p.id,
                "homarr",
                &Receipt {
                    started: true,
                    completed: true,
                    pending: Some("app:0".into()),
                    api_key: "private".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let result = store
            .onboarding_plan(
                &p.id,
                "homarr",
                OnboardingRequest {
                    mode: "sync".into(),
                    inputs: BTreeMap::new(),
                    apps: vec![],
                },
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("uncertain"));
    }
    #[test]
    fn substituted_values_are_never_templates() {
        let vars = BTreeMap::from([
            ("name".into(), "{password}".into()),
            ("password".into(), "private-password".into()),
        ]);
        assert_eq!(
            render(&json!({"name":"{name}","password":"{password}"}), &vars).unwrap(),
            json!({"name":"{password}","password":"private-password"})
        );
    }
    #[tokio::test]
    async fn interrupted_receipt_blocks_before_network_or_secret_use() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Fixture".into(),
                apps: vec!["homarr".into()],
            })
            .unwrap();
        store
            .onboarding_save(
                &p.id,
                "homarr",
                &Receipt {
                    started: true,
                    pending: Some("admin:0".into()),
                    api_key: "private-key".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let request = OnboardingRequest {
            mode: "bootstrap".into(),
            inputs: BTreeMap::new(),
            apps: vec![],
        };
        assert!(
            store
                .onboarding_plan(&p.id, "homarr", request)
                .await
                .unwrap_err()
                .to_string()
                .contains("already started")
        );
        let public = store.onboarding_info(&p.id, "homarr").unwrap().to_string();
        assert!(!public.contains("private-key"));
        assert!(store.onboarding_key(&p.id, "homarr").is_err());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "Creates and removes an isolated local Docker Homarr fixture"]
    async fn homarr_bootstrap_and_private_board_live() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = store
            .create(CreateProject {
                server_id: "local".into(),
                name: "Onboarding fixture".into(),
                apps: vec!["homarr".into()],
            })
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let p = store
            .edit(
                &p.id,
                EditProject {
                    name: p.name,
                    access: "local".into(),
                    services: vec![EditService {
                        app: "homarr".into(),
                        image: p.services[0].image.clone(),
                        port,
                    }],
                },
            )
            .unwrap();
        let result: Result<()> = async {
            store.action(&p.id, "start").await?;
            let password = format!("Fixture!A1{}", crate::core::token(8)?);
            let request = OnboardingRequest {
                mode: "bootstrap".into(),
                inputs: BTreeMap::from([
                    ("username".into(), "fixture-admin".into()),
                    ("password".into(), password.clone()),
                    ("board".into(), "Selfhost-fixture".into()),
                ]),
                apps: vec![
                    AppLink {
                        name: "First".into(),
                        url: "https://one.example.com".into(),
                        ..Default::default()
                    },
                    AppLink {
                        name: "{password}".into(),
                        url: "https://two.example.com".into(),
                        ..Default::default()
                    },
                ],
            };
            let mut plan = None;
            for _ in 0..60 {
                if let Ok(p) = store
                    .onboarding_plan(&p.id, "homarr", request.clone())
                    .await
                {
                    plan = Some(p);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            let plan = plan.context("Homarr did not become ready")?;
            ensure!(
                !plan.to_string().contains(&password),
                "Plan leaked password"
            );
            ensure!(
                store
                    .onboarding_apply(&p.id, "homarr", request.clone(), "stale")
                    .await
                    .is_err(),
                "Stale plan accepted"
            );
            let out = store
                .onboarding_apply(
                    &p.id,
                    "homarr",
                    request.clone(),
                    plan["revision"].as_str().unwrap(),
                )
                .await?;
            ensure!(
                out["completed"] == true && out["apps_added"] == 2 && out["items_added"] == 2,
                "Unexpected completion counts"
            );
            ensure!(
                !out.to_string().contains(&password),
                "Result leaked password"
            );
            ensure!(
                store
                    .onboarding_plan(&p.id, "homarr", request)
                    .await
                    .is_err(),
                "Repeated setup accepted"
            );
            let sync = OnboardingRequest {
                mode: "sync".into(),
                inputs: BTreeMap::new(),
                apps: vec![AppLink {
                    name: "Third".into(),
                    url: "https://three.example.com".into(),
                    ..Default::default()
                }],
            };
            let sync_plan = store.onboarding_plan(&p.id, "homarr", sync.clone()).await?;
            let synced = store
                .onboarding_apply(
                    &p.id,
                    "homarr",
                    sync.clone(),
                    sync_plan["revision"].as_str().unwrap(),
                )
                .await?;
            ensure!(
                synced["apps_added"] == 3 && synced["items_added"] == 3,
                "Saved-key sync counts incorrect"
            );
            ensure!(
                store.onboarding_plan(&p.id, "homarr", sync).await.is_err(),
                "Duplicate sync accepted"
            );
            let key = store.onboarding_key(&p.id, "homarr")?;
            ensure!(!key.is_empty(), "Missing saved API token");
            let client = reqwest::Client::builder().no_proxy().build()?;
            let base = format!("http://127.0.0.1:{port}");
            let apps: Value = client
                .get(format!("{base}/api/apps"))
                .header("ApiKey", &key)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let app_entries = apps.as_array().context("Apps not array")?;
            for expected in [
                "https://one.example.com",
                "https://two.example.com",
                "https://three.example.com",
            ] {
                ensure!(
                    app_entries
                        .iter()
                        .filter(|app| app["href"] == expected)
                        .count()
                        == 1,
                    "Expected exactly one reviewed app URL in {} entries",
                    app_entries.len()
                );
            }
            ensure!(
                apps.to_string().contains("{password}") && !apps.to_string().contains(&password),
                "Template-looking name was expanded"
            );
            let input = serde_json::to_string(&json!({"json":{"name":"Selfhost-fixture"}}))?;
            let board: Value = client
                .get(format!("{base}/api/trpc/board.getBoardByName"))
                .query(&[("input", input)])
                .header("ApiKey", &key)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let board = &board["result"]["data"]["json"];
            ensure!(board["isPublic"] == false, "Board must be private");
            let items = board["items"].as_array().context("Missing board items")?;
            ensure!(items.len() == 3, "Wrong board item count");
            let ids: std::collections::HashSet<_> = items
                .iter()
                .map(|i| i["options"]["appId"].as_str().unwrap_or(""))
                .collect();
            ensure!(ids.len() == 3, "Board links lost distinct app IDs");
            ensure!(
                !std::fs::read_to_string(store.onboarding_path(&p.id, "homarr")?)?
                    .contains(&password),
                "Password persisted"
            );
            // A reviewed task reconciles existing sources, then sources added later, without duplicates.
            let first = store.create(CreateProject {
                server_id: "local".into(),
                name: "First task source".into(),
                apps: vec!["gotify".into()],
            })?;
            let task_request = crate::tasks::TaskRequest {
                name: "Fixture automatic links".into(),
                kind: crate::tasks::TaskKind::DashboardLinks,
                destination: crate::tasks::Destination {
                    project_id: p.id.clone(),
                    service: "homarr".into(),
                },
                managed: true,
                existing: true,
                project_ids: vec![],
                existing_ids: vec![],
                interval_seconds: 300,
                action: String::new(),
            };
            let task_plan = Box::pin(store.task_plan(task_request.clone())).await?;
            let rule =
                Box::pin(store.task_create(task_request, task_plan["revision"].as_str().unwrap()))
                    .await?;
            let task_id = rule["id"].as_str().unwrap();
            ensure!(
                Box::pin(store.task_run(task_id)).await?["status"] == "complete",
                "Initial task failed"
            );
            ensure!(
                Box::pin(store.task_run(task_id)).await?["status"] == "idle",
                "Unchanged task did not deduplicate"
            );
            let second = store.create(CreateProject {
                server_id: "local".into(),
                name: "Second task source".into(),
                apps: vec!["uptime-kuma".into()],
            })?;
            let existing = crate::adoption::ExistingApp {
                id: "existing-fixture".into(),
                name: "Existing source fixture".into(),
                url: "https://existing.example.test".into(),
                server_id: "local".into(),
                container_id: "fixture".into(),
                container_name: "fixture".into(),
                image: "fixture:1".into(),
                image_id: "fixture".into(),
                profile: store
                    .existing_profiles()?
                    .into_values()
                    .next()
                    .context("Missing existing profile")?,
                allowed_actions: vec![],
                linked_at: crate::core::now(),
            };
            std::fs::write(
                store.root.join("existing-apps.json"),
                serde_json::to_vec(&vec![existing])?,
            )?;
            ensure!(
                Box::pin(store.task_run(task_id)).await?["status"] == "complete",
                "New-source task failed"
            );
            ensure!(
                Box::pin(store.task_run(task_id)).await?["status"] == "idle",
                "Second task did not deduplicate"
            );
            let task_apps: Value = client
                .get(format!("{base}/api/apps"))
                .header("ApiKey", &key)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            for expected in [
                format!("http://localhost:{}", first.services[0].port),
                format!("http://localhost:{}", second.services[0].port),
                "https://existing.example.test".into(),
            ] {
                ensure!(
                    task_apps
                        .as_array()
                        .context("Missing apps")?
                        .iter()
                        .filter(|a| a["href"] == expected)
                        .count()
                        == 1,
                    "Task-created URL was missing or duplicated"
                );
            }
            store.task_set_enabled(task_id, false)?;
            ensure!(
                Box::pin(store.task_run(task_id)).await.is_err(),
                "Disabled task executed"
            );
            store.task_remove(task_id, "Fixture automatic links")?;
            // Simulate connecting a fully configured instance without Selfhost's prior receipt.
            std::fs::remove_file(store.onboarding_path(&p.id, "homarr")?)?;
            let bootstrap = OnboardingRequest {
                mode: "bootstrap".into(),
                inputs: BTreeMap::from([
                    ("username".into(), "new-admin".into()),
                    ("password".into(), password.clone()),
                ]),
                apps: vec![],
            };
            ensure!(
                store
                    .onboarding_plan(&p.id, "homarr", bootstrap)
                    .await
                    .is_err(),
                "Configured app permitted administrator takeover"
            );
            let connect = OnboardingRequest {
                mode: "connect".into(),
                inputs: BTreeMap::from([
                    ("api_key".into(), key),
                    ("board".into(), "Connected-fixture".into()),
                ]),
                apps: vec![],
            };
            let connect_plan = store
                .onboarding_plan(&p.id, "homarr", connect.clone())
                .await?;
            let connected = store
                .onboarding_apply(
                    &p.id,
                    "homarr",
                    connect,
                    connect_plan["revision"].as_str().unwrap(),
                )
                .await?;
            ensure!(
                connected["completed"] == true && connected["has_api_key"] == true,
                "Existing-key connection failed"
            );
            let mut changed = store.project(&p.id)?;
            changed.services[0].port = port.saturating_sub(1);
            store.edit(
                &p.id,
                EditProject {
                    name: changed.name,
                    access: "local".into(),
                    services: vec![EditService {
                        app: "homarr".into(),
                        image: changed.services[0].image.clone(),
                        port: changed.services[0].port,
                    }],
                },
            )?;
            ensure!(
                store.onboarding_target(&p.id, "homarr").await.is_err(),
                "Stale port accepted"
            );
            Ok(())
        }
        .await;
        // These Compose resources were created only by this fixture, under its random project ID.
        let cleanup = store
            .docker(&p, &["down", "--volumes", "--remove-orphans"])
            .await;
        cleanup.unwrap();
        result.unwrap();
    }
}
