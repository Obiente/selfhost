//! Provider-neutral inventory, native route plans and guarded proxy deployment.
use crate::core::{Store, atomic_write, private_dir, token};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proxy {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// SSH goes directly to the proxy host or guest, independent of Docker placement.
    #[serde(default)]
    pub ssh_alias: String,
    #[serde(default)]
    pub admin_url: String,
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub ca_certificate: String,
    #[serde(default)]
    pub config_directory: String,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
    #[serde(default = "read_only")]
    pub read_only: bool,
}
fn read_only() -> bool {
    true
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub id: String,
    pub proxy_id: String,
    pub project_id: String,
    pub service: String,
    pub domain: String,
    /// Must be HTTPS for cross-host hops; localhost HTTP is explicitly same-host/tunnel only.
    pub upstream: String,
    #[serde(default)]
    pub network_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateNetwork {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub description: String,
    /// Record the identities/endpoints and scoped policy managed by the network operator.
    pub endpoints: Vec<String>,
    pub policy_reference: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct NetworkData {
    proxies: Vec<Proxy>,
    routes: Vec<Route>,
    networks: Vec<PrivateNetwork>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub description: String,
    #[serde(default)]
    pub read_path: String,
    #[serde(default)]
    pub write_path: String,
    #[serde(default)]
    pub concurrency_path: String,
    #[serde(default)]
    pub domain_pointer: String,
    pub template: Value,
    #[serde(default)]
    pub loopback_http_template: Option<Value>,
    #[serde(default)]
    pub defaults: BTreeMap<String, Value>,
    #[serde(default)]
    pub settings_schema: BTreeMap<String, Setting>,
    #[serde(default)]
    pub preflight: Vec<Preflight>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub required_when: Option<(String, Value)>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preflight {
    pub path: String,
    pub pointer: String,
    pub condition: String,
    pub message: String,
}
fn validate_settings(profile: &ProxyProfile, settings: &BTreeMap<String, Value>) -> Result<()> {
    for (key, rule) in &profile.settings_schema {
        if let Some((other, value)) = &rule.required_when
            && settings.get(other) != Some(value)
        {
            continue;
        }
        let value = settings
            .get(key)
            .context(format!("Missing setting: {}", rule.label))?;
        let valid = match rule.kind.as_str() {
            "identifier" => value.as_str().is_some_and(alias),
            "certificate" => value.as_u64().is_some_and(|n| n > 0) || value == "new",
            "email" => value.as_str().is_some_and(|s| {
                s.contains('@') && !s.contains(['\r', '\n', ' ']) && s.len() < 254
            }),
            "accept" => value == true,
            "ca_pool" => {
                value.is_null()
                    || (value["provider"] == "file"
                        && value.as_object().is_some_and(|v| v.len() == 2)
                        && value["pem_files"].as_array().is_some_and(|a| {
                            !a.is_empty() && a.iter().all(|v| v.as_str().is_some_and(absolute_path))
                        }))
            }
            "paths" => value
                .as_array()
                .is_some_and(|a| a.iter().all(|v| v.as_str().is_some_and(absolute_path))),
            "path" => value.as_str().is_some_and(absolute_path),
            _ => false,
        };
        ensure!(valid, "Invalid or missing setting: {}", rule.label);
    }
    ensure!(
        settings
            .keys()
            .all(|k| profile.defaults.contains_key(k) || profile.settings_schema.contains_key(k)),
        "Unknown proxy setting"
    );
    Ok(())
}
fn absolute_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 512
        && !path.contains(['\n', '\r', '\0', ';', '{', '}'])
        && !path.split('/').any(|s| s == "..")
}
fn preflight_matches(value: Option<&Value>, condition: &str) -> bool {
    match condition {
        "not_true" => value != Some(&Value::Bool(true)),
        "https_listener" => value.and_then(Value::as_array).is_some_and(|a| {
            a.iter()
                .any(|v| v.as_str().is_some_and(|s| s.ends_with(":443")))
        }),
        "empty" => value.is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty)),
        _ => false,
    }
}
pub fn render(value: &Value, vars: &BTreeMap<String, Value>) -> Result<Value> {
    Ok(match value {
        Value::String(s) => {
            if let Some(k) = s.strip_prefix("{{").and_then(|s| s.strip_suffix("}}"))
                && let Some(v) = vars.get(k)
            {
                return Ok(v.clone());
            }
            json!(crate::integrations::expand(s, vars)?)
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| render(v, vars)).collect::<Result<_>>()?),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter_map(|(k, v)| {
                    // A null exact substitution omits an optional provider field.
                    let rendered = render(v, vars);
                    if rendered.as_ref().is_ok_and(Value::is_null)
                        && v.as_str()
                            .is_some_and(|s| s.starts_with("{{") && s.ends_with("}}"))
                    {
                        return None;
                    }
                    Some(
                        rendered
                            .and_then(|value| Ok((crate::integrations::expand(k, vars)?, value))),
                    )
                })
                .collect::<Result<_>>()?,
        ),
        v => v.clone(),
    })
}
fn digest(value: &impl Serialize) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
pub(crate) fn hostname(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|s| {
            !s.is_empty()
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.len() <= 63
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
fn alias(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn endpoint(p: &Proxy) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(&p.admin_url)?;
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "Use a proxy API origin without credentials or a path"
    );
    ensure!(
        url.scheme() == "https"
            || (!p.ssh_alias.is_empty()
                && url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
        "Proxy management needs HTTPS or an SSH connection to a loopback API"
    );
    Ok(url)
}
async fn ssh(alias: &str, command: &str, input: Vec<u8>) -> Result<String> {
    ensure!(self::alias(alias), "Use a trusted SSH host alias");
    let mut child = tokio::process::Command::new("ssh")
        .args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=10",
            "--",
            alias,
            command,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (status, bytes) = tokio::time::timeout(Duration::from_secs(60), async {
        let write = async {
            stdin.write_all(&input).await?;
            drop(stdin);
            Ok::<_, anyhow::Error>(())
        };
        let read = async {
            let mut bytes = Vec::new();
            stdout
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await?;
            ensure!(
                bytes.len() <= 2 * 1024 * 1024,
                "Proxy response is too large"
            );
            Ok::<_, anyhow::Error>(bytes)
        };
        let (_, status, bytes) = tokio::try_join!(write, async { Ok(child.wait().await?) }, read)?;
        Ok::<_, anyhow::Error>((status, bytes))
    })
    .await
    .context("Proxy operation timed out; inspect its state before retrying")??;
    ensure!(
        status.success(),
        "Proxy SSH operation failed; inspect permissions and service state"
    );
    Ok(String::from_utf8(bytes)?)
}
async fn request(
    p: &Proxy,
    method: &str,
    path: &str,
    body: Option<&Value>,
    etag: Option<&str>,
) -> Result<(Value, Option<String>)> {
    let origin = endpoint(p)?;
    let url = origin.join(path)?;
    ensure!(
        url.origin() == origin.origin(),
        "Proxy API path changed origin"
    );
    if !p.ssh_alias.is_empty() {
        // curl configuration is passed on stdin: tokens do not appear in process arguments.
        let mut config = format!(
            "url = {}\nrequest = {}\nheader = \"Content-Type: application/json\"\n",
            serde_json::to_string(url.as_str())?,
            serde_json::to_string(method)?
        );
        if !p.api_token.is_empty() {
            config.push_str(&format!(
                "header = {}\n",
                serde_json::to_string(&format!("Authorization: Bearer {}", p.api_token))?
            ));
        }
        if let Some(etag) = etag {
            config.push_str(&format!(
                "header = {}\n",
                serde_json::to_string(&format!("If-Match: {etag}"))?
            ));
        }
        if let Some(body) = body {
            config.push_str(&format!(
                "data-binary = {}\n",
                serde_json::to_string(&body.to_string())?
            ));
        }
        let out = ssh(
            &p.ssh_alias,
            "curl --silent --show-error --noproxy '*' --http1.1 --include --max-time 45 --config -",
            config.into_bytes(),
        )
        .await?;
        let (headers, body) = out
            .split_once("\r\n\r\n")
            .or_else(|| out.split_once("\n\n"))
            .context("Invalid proxy HTTP response")?;
        let status = headers
            .lines()
            .next()
            .and_then(|s| s.split_whitespace().nth(1))
            .context("Missing proxy status")?
            .parse::<u16>()?;
        ensure!(
            (200..300).contains(&status),
            "Proxy API rejected request (HTTP {status}); review current configuration"
        );
        let etag = headers.lines().find_map(|s| {
            s.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("etag"))
                .map(|(_, v)| v.trim().to_owned())
        });
        return Ok((
            if body.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(body)?
            },
            etag,
        ));
    }
    let mut client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(45));
    for certificate in reqwest::Certificate::from_pem_bundle(p.ca_certificate.as_bytes())? {
        client = client.add_root_certificate(certificate);
    }
    let client = client.build()?;
    let mut req = client.request(method.parse()?, url);
    if !p.api_token.is_empty() {
        req = req.bearer_auth(&p.api_token);
    }
    if let Some(body) = body {
        req = req.json(body);
    }
    if let Some(etag) = etag {
        req = req.header("If-Match", etag);
    }
    let mut response = req.send().await.context("Proxy API could not be reached")?;
    ensure!(
        response.status().is_success(),
        "Proxy rejected request (HTTP {}); review its configuration",
        response.status().as_u16()
    );
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 2 * 1024 * 1024,
            "Proxy response is too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok((
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        },
        etag,
    ))
}
impl Store {
    fn network_data(&self) -> Result<NetworkData> {
        let path = self.root.join("networking.json");
        if path.exists() {
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        } else {
            Ok(NetworkData::default())
        }
    }
    fn network_lock(&self) -> Result<std::fs::File> {
        private_dir(&self.root)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.root.join("networking.lock"))?;
        fs2::FileExt::try_lock_exclusive(&file)
            .context("Another networking operation is running")?;
        Ok(file)
    }
    pub fn proxy_profiles(&self) -> Result<BTreeMap<String, ProxyProfile>> {
        let mut result = BTreeMap::new();
        for name in crate::catalog::BuiltinCatalog::iter()
            .filter(|n| n.starts_with("infrastructure/proxies/") && n.ends_with(".json"))
        {
            let profile: ProxyProfile =
                serde_json::from_slice(&crate::catalog::BuiltinCatalog::get(&name).unwrap().data)?;
            result.insert(profile.id.clone(), profile);
        }
        let dir = self.root.join("proxy-profiles");
        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|s| s == "json") {
                    let profile: ProxyProfile =
                        serde_json::from_slice(&std::fs::read(entry.path())?)?;
                    ensure!(crate::setup::slug(&profile.id), "Invalid proxy profile ID");
                    result.insert(profile.id.clone(), profile);
                }
            }
        }
        Ok(result)
    }
    pub fn networking(&self) -> Result<Value> {
        let mut data = self.network_data()?;
        for p in &mut data.proxies {
            p.api_token = String::new();
        }
        let networks = crate::catalog::BuiltinCatalog::get("infrastructure/networks.json")
            .context("Missing network adapters")?;
        Ok(
            json!({"data":data,"proxy_profiles":self.proxy_profiles()?,"network_profiles":serde_json::from_slice::<Value>(&networks.data)?}),
        )
    }
    pub fn add_proxy(&self, mut p: Proxy) -> Result<String> {
        p.id = String::new();
        self.save_proxy(p)
    }
    pub fn save_proxy(&self, mut p: Proxy) -> Result<String> {
        let _lock = self.network_lock()?;
        ensure!(
            !p.name.trim().is_empty() && p.name.len() <= 100,
            "Name the proxy"
        );
        let profile = self
            .proxy_profiles()?
            .remove(&p.provider)
            .context("Unknown proxy profile")?;
        ensure!(
            p.ssh_alias.is_empty() || alias(&p.ssh_alias),
            "Invalid SSH alias"
        );
        ensure!(
            p.ca_certificate.len() <= 64 * 1024,
            "CA certificate bundle is too large"
        );
        if !p.ca_certificate.is_empty() {
            ensure!(
                p.ssh_alias.is_empty(),
                "For management through SSH, install the CA on the proxy host or use its loopback HTTP API"
            );
            ensure!(
                !reqwest::Certificate::from_pem_bundle(p.ca_certificate.as_bytes())?.is_empty(),
                "Invalid CA certificate bundle"
            );
        }
        if profile.driver == "file_watch" {
            ensure!(
                !p.ssh_alias.is_empty()
                    && p.config_directory.starts_with('/')
                    && !p.config_directory.contains(['\n', '\r', '\0'])
                    && !p.config_directory.split('/').any(|s| s == ".."),
                "Choose the proxy SSH alias and absolute watched configuration directory"
            );
        } else {
            endpoint(&p)?;
        }
        let mut data = self.network_data()?;
        if p.id.is_empty() {
            p.id = format!("proxy-{}", token(6)?);
            data.proxies.push(p.clone());
        } else {
            let old = data
                .proxies
                .iter()
                .find(|old| old.id == p.id)
                .context("Proxy not found")?;
            let same_target = old.provider == p.provider
                && old.admin_url == p.admin_url
                && old.ssh_alias == p.ssh_alias
                && old.config_directory == p.config_directory;
            ensure!(
                same_target || !data.routes.iter().any(|r| r.proxy_id == p.id),
                "This proxy has routes; add a separate connection to change its target"
            );
            if p.api_token.is_empty() && same_target {
                p.api_token = old.api_token.clone();
            }
            *data.proxies.iter_mut().find(|old| old.id == p.id).unwrap() = p.clone();
        }
        atomic_write(
            &self.root.join("networking.json"),
            &serde_json::to_vec_pretty(&data)?,
        )?;
        Ok(p.id)
    }
    pub fn add_network(&self, mut n: PrivateNetwork) -> Result<String> {
        let _lock = self.network_lock()?;
        ensure!(
            !n.name.is_empty()
                && !n.provider.is_empty()
                && !n.policy_reference.is_empty()
                && n.endpoints.len() >= 2
                && n.endpoints.len() <= 64,
            "Provide the network endpoints and its scoped access policy reference"
        );
        n.id = format!("network-{}", token(6)?);
        let mut data = self.network_data()?;
        data.networks.push(n.clone());
        atomic_write(
            &self.root.join("networking.json"),
            &serde_json::to_vec_pretty(&data)?,
        )?;
        Ok(n.id)
    }
    fn route_parts(&self, route: &Route) -> Result<(Proxy, ProxyProfile, Value)> {
        ensure!(
            crate::setup::slug(&route.id) && hostname(&route.domain),
            "Use a route ID and exact DNS hostname"
        );
        let data = self.network_data()?;
        let proxy = data
            .proxies
            .into_iter()
            .find(|p| p.id == route.proxy_id)
            .context("Proxy not found")?;
        let project = self.project(&route.project_id)?;
        ensure!(
            project.services.iter().any(|s| s.app == route.service),
            "Service is not part of this project"
        );
        if let Some(id) = &route.network_id {
            ensure!(
                data.networks.iter().any(|n| &n.id == id),
                "Private network not found"
            );
        }
        let upstream = reqwest::Url::parse(&route.upstream)?;
        let loopback_http = upstream.scheme() == "http"
            && !proxy.ssh_alias.is_empty()
            && matches!(
                upstream.host_str(),
                Some("127.0.0.1" | "localhost" | "[::1]")
            );
        ensure!(
            (upstream.scheme() == "https" || loopback_http)
                && upstream.username().is_empty()
                && upstream.password().is_none()
                && upstream.path() == "/"
                && upstream.query().is_none()
                && upstream.fragment().is_none(),
            "Use verified HTTPS for cross-server upstreams, or an HTTP loopback address on the proxy host with its SSH alias."
        );
        let profile = self
            .proxy_profiles()?
            .remove(&proxy.provider)
            .context("Proxy profile missing")?;
        let mut vars = profile.defaults.clone();
        vars.extend(proxy.settings.clone());
        validate_settings(&profile, &vars)?;
        vars.extend(BTreeMap::from([
            ("route-id".into(), json!(format!("selfhost-{}", route.id))),
            ("domain".into(), json!(route.domain)),
            (
                "upstream-url".into(),
                json!(upstream.as_str().trim_end_matches('/')),
            ),
            (
                "upstream-host".into(),
                json!(upstream.host_str().context("Missing upstream host")?),
            ),
            (
                "upstream-port".into(),
                json!(upstream.port_or_known_default().unwrap()),
            ),
            (
                "upstream-address".into(),
                json!(format!(
                    "{}:{}",
                    upstream.host_str().unwrap(),
                    upstream.port_or_known_default().unwrap()
                )),
            ),
        ]));
        let template = if loopback_http {
            profile
                .loopback_http_template
                .as_ref()
                .context("This proxy profile does not support loopback HTTP")?
        } else {
            &profile.template
        };
        let body = render(template, &vars)?;
        Ok((proxy, profile, body))
    }
    pub async fn route_plan(&self, route: &Route) -> Result<Value> {
        let (proxy, profile, body) = self.route_parts(route)?;
        let mut vars = profile.defaults.clone();
        vars.extend(proxy.settings.clone());
        let mut checks = BTreeMap::new();
        for check in &profile.preflight {
            let path = crate::integrations::expand(&check.path, &vars)?;
            if !checks.contains_key(&path) {
                checks.insert(
                    path.clone(),
                    request(&proxy, "GET", &path, None, None).await?,
                );
            }
            let (value, _) = &checks[&path];
            ensure!(
                preflight_matches(value.pointer(&check.pointer), &check.condition),
                "{}",
                check.message
            );
        }
        let (current, etag) = if profile.driver == "file_watch" {
            let path = format!(
                "{}/selfhost-{}.json",
                proxy.config_directory.trim_end_matches('/'),
                route.id
            );
            let command = format!(
                "if test -e {p} || test -L {p}; then printf true; else printf null; fi",
                p = quote(&path)
            );
            (
                serde_json::from_str(&ssh(&proxy.ssh_alias, &command, vec![]).await?)?,
                None,
            )
        } else {
            request(
                &proxy,
                "GET",
                &crate::integrations::expand(&profile.read_path, &vars)?,
                None,
                None,
            )
            .await?
        };
        let scope = if profile.concurrency_path.is_empty() {
            (current.clone(), etag.clone())
        } else {
            let path = crate::integrations::expand(&profile.concurrency_path, &vars)?;
            match checks.remove(&path) {
                Some(value) => value,
                None => request(&proxy, "GET", &path, None, None).await?,
            }
        };
        ensure!(
            profile.driver != "file_watch" || current.is_null(),
            "A configuration file already exists for this route ID; it will not be overwritten"
        );
        let matches = current
            .as_array()
            .map(|a| {
                a.iter().any(|v| {
                    v.pointer(&profile.domain_pointer)
                        .and_then(Value::as_array)
                        .is_some_and(|names| names.contains(&json!(route.domain)))
                })
            })
            .unwrap_or(false);
        ensure!(
            !matches,
            "This hostname already exists on the proxy. Adopt or edit that route explicitly before changing it."
        );
        Ok(
            json!({"route":route,"native_config":body,"revision":digest(&(&proxy,&profile,route,&current,&scope,self.project(&route.project_id)?,self.setup(&route.project_id)?))?,"concurrency_etag":scope.1,"can_apply":!proxy.read_only,"certificate_status":"not_checked","note":"The proxy obtains and renews browser certificates. DNS and challenge reachability must be configured. Cross-server upstream TLS verification remains enabled."}),
        )
    }
    pub async fn route_apply(&self, route: Route, revision: &str) -> Result<Value> {
        let _lock = self.network_lock()?;
        let _project = self.project_lock(&route.project_id)?;
        let project = self.project(&route.project_id)?;
        self.project_docker(&project, true)?;
        let (proxy, profile, body) = self.route_parts(&route)?;
        ensure!(!proxy.read_only, "Proxy is read-only");
        let plan = self.route_plan(&route).await?;
        ensure!(
            plan["revision"].as_str() == Some(revision),
            "Proxy configuration changed; review a new plan"
        );
        let dir = self.root.join("network-operations");
        private_dir(&dir)?;
        let journal = dir.join(format!("{}.json", route.id));
        ensure!(
            !journal.exists(),
            "This route has an operation record. Inspect it before retrying to avoid duplicate resources."
        );
        atomic_write(
            &journal,
            &serde_json::to_vec_pretty(&json!({"route":route,"stage":"applying","plan":plan}))?,
        )?;
        let mut vars = profile.defaults.clone();
        vars.extend(proxy.settings.clone());
        let response = match profile.driver.as_str() {
            "json_collection" => {
                let etag = plan["concurrency_etag"]
                    .as_str()
                    .context("Proxy did not provide a concurrency token")?;
                request(
                    &proxy,
                    "POST",
                    &crate::integrations::expand(&profile.write_path, &vars)?,
                    Some(&body),
                    Some(etag),
                )
                .await?
                .0
            }
            "rest_create" => {
                request(
                    &proxy,
                    "POST",
                    &crate::integrations::expand(&profile.write_path, &vars)?,
                    Some(&body),
                    None,
                )
                .await?
                .0
            }
            "file_watch" => {
                let path = format!(
                    "{}/selfhost-{}.json",
                    proxy.config_directory.trim_end_matches('/'),
                    route.id
                );
                let command = format!(
                    "set -eu; umask 077; test ! -e {p}; tmp=$(mktemp {t}); trap 'rm -f -- \"$tmp\"' EXIT; cat > \"$tmp\"; ln -- \"$tmp\" {p};",
                    p = quote(&path),
                    t = quote(&format!("{path}.XXXXXX"))
                );
                ssh(
                    &proxy.ssh_alias,
                    &command,
                    serde_json::to_vec_pretty(&body)?,
                )
                .await?;
                json!({"file":path})
            }
            _ => anyhow::bail!("This profile has no supported deployment driver"),
        };
        atomic_write(
            &journal,
            &serde_json::to_vec_pretty(
                &json!({"route":route,"stage":"configuration_submitted","response":response}),
            )?,
        )?;
        let mut data = self.network_data()?;
        data.routes.push(route);
        atomic_write(
            &self.root.join("networking.json"),
            &serde_json::to_vec_pretty(&data)?,
        )?;
        Ok(
            json!({"status":"configuration_submitted","message":"Proxy configuration submitted. Verify HTTPS and upstream reachability before treating the route as ready."}),
        )
    }
    pub async fn route_probe(&self, route: &Route) -> Result<Value> {
        let (proxy, _, _) = self.route_parts(route)?;
        ensure!(
            !proxy.ssh_alias.is_empty(),
            "Add the proxy host SSH alias to test from that host. Container network reachability must also be checked."
        );
        let command = format!(
            "curl --silent --show-error --noproxy '*' --fail --max-time 15 --output /dev/null --write-out '%{{http_code}}' -- {}",
            quote(&route.upstream)
        );
        let code = ssh(&proxy.ssh_alias, &command, vec![]).await?;
        Ok(
            json!({"upstream_tls_verified":route.upstream.starts_with("https://"),"http_status":code.trim(),"tested_from":proxy.name,"certificate_status":"public_certificate_not_checked"}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_templates_preserve_types_and_literal_data() {
        let vars = BTreeMap::from([
            ("port".into(), json!(8443)),
            ("host".into(), json!("literal {{port}}")),
        ]);
        assert_eq!(
            render(&json!({"port":"{{port}}","host":"{{host}}"}), &vars).unwrap(),
            json!({"port":8443,"host":"literal {{port}}"})
        );
        assert!(!hostname("example.com;touch"));
        assert!(!alias("-oProxyCommand=evil"));
    }
    #[test]
    fn native_profiles_enforce_certificates_and_preserve_secure_transports() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let profiles = store.proxy_profiles().unwrap();
        let npm = &profiles["nginx-proxy-manager"];
        let mut settings = npm.defaults.clone();
        assert!(validate_settings(npm, &settings).is_err());
        settings.insert("certificate-id".into(), json!("new"));
        assert!(validate_settings(npm, &settings).is_err());
        settings.insert("acme-email".into(), json!("admin@example.com"));
        settings.insert("acme-terms-agreed".into(), json!(true));
        validate_settings(npm, &settings).unwrap();
        let caddy = &profiles["caddy"];
        let mut settings = caddy.defaults.clone();
        settings.insert("server".into(), json!("srv0/../../other"));
        assert!(validate_settings(caddy, &settings).is_err());
        settings = caddy.defaults.clone();
        settings.insert(
            "upstream-ca".into(),
            json!({"provider":"http","endpoints":["http://attacker.example"]}),
        );
        assert!(validate_settings(caddy, &settings).is_err());
        assert!(!preflight_matches(Some(&json!([":80"])), "https_listener"));
        assert!(!preflight_matches(Some(&json!(true)), "not_true"));
        assert!(!preflight_matches(
            Some(&json!(["app.example.com"])),
            "empty"
        ));
        assert_eq!(
            render(
                &json!({"optional":"{{ca}}","literal":null}),
                &BTreeMap::from([("ca".into(), Value::Null)])
            )
            .unwrap(),
            json!({"literal":null})
        );
    }
    #[tokio::test]
    async fn remote_plaintext_and_read_only_routes_never_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let setup = crate::setup::Setup {
            compose: json!({"services":{"app":{"image":"nginx:alpine"}}}),
            environment: BTreeMap::new(),
            files: BTreeMap::new(),
            database: None,
        };
        let project = store.create_setup("Synthetic", "local", setup).unwrap();
        let id=store.add_proxy(serde_json::from_value(json!({"id":"","name":"Test","provider":"caddy","ssh_alias":"synthetic-proxy","admin_url":"http://127.0.0.1:2019"})).unwrap()).unwrap();
        let mut route = Route {
            id: "test".into(),
            proxy_id: id,
            project_id: project.id,
            service: "app".into(),
            domain: "app.example.com".into(),
            upstream: "http://10.0.0.5:8080".into(),
            network_id: None,
        };
        assert!(store.route_parts(&route).is_err());
        route.upstream = "http://127.0.0.1:8080".into();
        assert!(store.route_parts(&route).is_ok());
        assert!(
            store
                .route_apply(route, "unused")
                .await
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
        assert!(!dir.path().join("network-operations").exists());
    }
}
