//! Selfhost's own login. Provider brands are configuration, authorization is explicit.
use crate::core::{Store, atomic_write, now, private_dir, token};
use anyhow::{Context, Result, ensure};
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginProvider {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    /// Optional private CA certificate bundle, never a TLS verification bypass.
    #[serde(default)]
    pub ca_certificate: String,
    pub admin_subjects: Vec<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginConfig {
    pub public_url: String,
    pub providers: Vec<LoginProvider>,
}
impl LoginConfig {
    pub fn validate(&self) -> Result<()> {
        let base = identity_url(&self.public_url)?;
        ensure!(
            base.path() == "/",
            "Selfhost's public URL must be an origin without a path"
        );
        ensure!(
            !self.providers.is_empty() && self.providers.len() <= 32,
            "Configure between 1 and 32 identity providers"
        );
        let mut ids = std::collections::HashSet::new();
        for p in &self.providers {
            ensure!(
                crate::setup::slug(&p.id) && ids.insert(&p.id),
                "Invalid or duplicate login provider ID"
            );
            identity_url(&p.issuer)?;
            http_client(p)?;
            ensure!(
                !p.name.trim().is_empty()
                    && p.name.len() <= 100
                    && !p.client_id.is_empty()
                    && p.client_id.len() <= 512
                    && p.client_secret.len() <= 8192,
                "Invalid login provider"
            );
            ensure!(
                !p.admin_subjects.is_empty()
                    && p.admin_subjects.len() <= 100
                    && p.admin_subjects
                        .iter()
                        .all(|s| !s.is_empty() && s.len() <= 512 && s != "*"),
                "Choose explicit provider subject IDs allowed to administer Selfhost"
            );
        }
        Ok(())
    }
    pub fn public(&self) -> Value {
        json!({"public_url":self.public_url,"providers":self.providers.iter().map(|p|json!({"id":p.id,"name":p.name,"issuer":p.issuer,"client_id":p.client_id,"ca_certificate":p.ca_certificate,"has_secret":!p.client_secret.is_empty(),"admin_subjects":p.admin_subjects})).collect::<Vec<_>>()})
    }
    pub fn origin(&self) -> String {
        reqwest::Url::parse(&self.public_url)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_default()
    }
    pub fn is_loopback(&self) -> bool {
        reqwest::Url::parse(&self.public_url).is_ok_and(|url| loopback_url(&url))
    }
}
fn http_client(provider: &LoginProvider) -> Result<reqwest::Client> {
    ensure!(
        provider.ca_certificate.len() <= 64 * 1024,
        "Certificate bundle is too large"
    );
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20));
    if !provider.ca_certificate.is_empty() {
        let certificates =
            reqwest::Certificate::from_pem_bundle(provider.ca_certificate.as_bytes())
                .context("Invalid identity CA certificate")?;
        ensure!(!certificates.is_empty(), "Certificate bundle is empty");
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    Ok(builder.build()?)
}
async fn secure_request(
    http: reqwest::Client,
    request: openidconnect::HttpRequest,
    loopback_origin: Option<String>,
) -> std::io::Result<openidconnect::HttpResponse> {
    let url = identity_url(&request.uri().to_string()).map_err(|_| {
        std::io::Error::other("Identity endpoints require HTTPS or configured loopback")
    })?;
    if url.scheme() != "https"
        && loopback_origin.as_deref() != Some(url.origin().ascii_serialization().as_str())
    {
        return Err(std::io::Error::other("Identity endpoints must use HTTPS"));
    }
    let request = reqwest::Request::try_from(request)
        .map_err(|_| std::io::Error::other("Invalid identity request"))?;
    let mut response = http
        .execute(request)
        .await
        .map_err(|_| std::io::Error::other("Identity endpoint unavailable"))?;
    let mut builder = axum::http::Response::builder().status(response.status());
    for (name, value) in response.headers() {
        builder = builder.header(name, value);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| std::io::Error::other("Identity response unavailable"))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err(std::io::Error::other("Identity response too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    builder
        .body(bytes)
        .map_err(|_| std::io::Error::other("Invalid identity response"))
}
pub fn identity_url(text: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(text)?;
    let loopback = loopback_url(&url);
    ensure!(
        (url.scheme() == "https" || (url.scheme() == "http" && loopback))
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use HTTPS, or HTTP on localhost or a loopback IP, without credentials, query, or fragment"
    );
    Ok(url)
}
fn loopback_url(url: &reqwest::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}
impl Store {
    pub fn login_config(&self) -> Result<Option<LoginConfig>> {
        let path = self.root.join("login.json");
        if !path.exists() {
            return Ok(None);
        }
        let config: LoginConfig = serde_json::from_slice(&std::fs::read(path)?)?;
        config.validate()?;
        Ok(Some(config))
    }
    pub fn save_login_config(&self, mut config: LoginConfig) -> Result<()> {
        self.save_login_config_checked(&mut config, None)
    }
    pub fn save_login_config_checked(
        &self,
        config: &mut LoginConfig,
        expected: Option<&str>,
    ) -> Result<()> {
        use fs2::FileExt;
        private_dir(&self.root)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("login-settings.lock"))?;
        lock.lock_exclusive()?;
        let previous = self.login_config()?;
        if expected.is_none() {
            ensure!(
                previous
                    .as_ref()
                    .is_none_or(|old| old.origin() == config.origin()),
                "Changing Selfhost's address requires identity plan and identity apply with callback confirmation, or the reviewed dashboard flow."
            );
        }
        if let Some(expected) = expected {
            ensure!(
                equal(expected, &login_revision(previous.as_ref())?),
                "Login settings changed. Review a fresh plan before saving."
            );
        }
        // Empty secrets preserve an existing credential only for the identical client.
        if let Some(old) = &previous {
            for p in &mut config.providers {
                if p.client_secret.is_empty()
                    && let Some(previous) = old.providers.iter().find(|o| {
                        o.id == p.id && o.issuer == p.issuer && o.client_id == p.client_id
                    })
                {
                    p.client_secret = previous.client_secret.clone();
                }
            }
        }
        config.validate()?;
        if let Some(old) = previous {
            let backups = self.root.join("login-backups");
            private_dir(&backups)?;
            atomic_write(
                &backups.join(format!("{}-{}.json", now(), token(8)?)),
                &serde_json::to_vec_pretty(&old)?,
            )?;
        }
        atomic_write(
            &self.root.join("login.json"),
            &serde_json::to_vec_pretty(&config)?,
        )
    }
}
pub fn login_revision(config: Option<&LoginConfig>) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&config)?)
    ))
}
struct Pending {
    provider: LoginProvider,
    config_revision: String,
    state: String,
    nonce: Nonce,
    verifier: PkceCodeVerifier,
    metadata: CoreProviderMetadata,
    expires: u64,
}
struct Session {
    provider: String,
    subject: String,
    name: String,
    created: u64,
    last_active: u64,
    expires: u64,
    config_revision: String,
    local_origin: Option<String>,
    client_key: String,
}
pub const SESSION_SECONDS: u64 = 8 * 3600;
pub const SETUP_SESSION_SECONDS: u64 = 30 * 60;
pub const IDLE_SECONDS: u64 = 30 * 60;
#[derive(Clone, Serialize)]
pub struct Account {
    pub kind: &'static str,
    pub name: String,
    pub provider: String,
    pub subject: String,
    pub role: &'static str,
    pub created_at: u64,
    pub expires_at: u64,
    pub idle_expires_at: u64,
}
pub struct LoginState {
    pending: Mutex<HashMap<String, Pending>>,
    sessions: Mutex<HashMap<String, Session>>,
    recovery: Mutex<Option<(String, u64)>>,
    attempts: Mutex<Vec<u64>>,
    discovery: tokio::sync::Semaphore,
}
impl Default for LoginState {
    fn default() -> Self {
        Self {
            pending: Mutex::default(),
            sessions: Mutex::default(),
            recovery: Mutex::default(),
            attempts: Mutex::default(),
            discovery: tokio::sync::Semaphore::new(4),
        }
    }
}
pub fn cookie<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=");
    let mut found = None;
    for header in headers.get_all(axum::http::header::COOKIE) {
        for part in header.to_str().ok()?.split(';') {
            if let Some(value) = part.trim().strip_prefix(&prefix) {
                if found.is_some() || value.is_empty() {
                    return None;
                }
                found = Some(value);
            }
        }
    }
    found
}
pub fn cookie_value(name: &str, value: &str, age: u64) -> String {
    format!("{name}={value}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={age}")
}
fn revision(config: &LoginConfig) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(config)?)))
}
pub fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (a, b)| d | (a ^ b)) == 0
}
impl LoginState {
    pub fn with_recovery(link: String) -> Self {
        Self {
            recovery: Mutex::new(Some((link, now() + 600))),
            ..Self::default()
        }
    }
    pub fn recover(&self, supplied: &str, origin: &str) -> Result<(String, String)> {
        self.recover_session(supplied, origin, SESSION_SECONDS, "Local administrator")
    }
    pub fn recover_setup(&self, supplied: &str, origin: &str) -> Result<(String, String)> {
        self.recover_session(
            supplied,
            origin,
            SETUP_SESSION_SECONDS,
            "Setup administrator",
        )
    }
    fn recover_session(
        &self,
        supplied: &str,
        origin: &str,
        lifetime: u64,
        name: &str,
    ) -> Result<(String, String)> {
        let mut recovery = self
            .recovery
            .lock()
            .map_err(|_| anyhow::anyhow!("Login unavailable"))?;
        ensure!(
            recovery
                .as_ref()
                .is_some_and(|(key, expiry)| *expiry > now() && equal(key, supplied)),
            "Recovery link expired or already used. Restart selfhost serve for a new link."
        );
        let id = token(32)?;
        let client_key = token(32)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Login unavailable"))?;
        sessions.insert(
            id.clone(),
            Session {
                provider: String::new(),
                subject: String::new(),
                name: name.into(),
                created: now(),
                last_active: now(),
                expires: now() + lifetime,
                config_revision: String::new(),
                local_origin: Some(origin.into()),
                client_key: client_key.clone(),
            },
        );
        *recovery = None;
        Ok((id, client_key))
    }
    pub fn account(
        &self,
        id: &str,
        config: Option<&LoginConfig>,
        origin: Option<&str>,
        client_key: &str,
    ) -> Option<Account> {
        let mut sessions = self.sessions.lock().ok()?;
        sessions.retain(|_, s| s.expires > now() && s.last_active + IDLE_SECONDS > now());
        let s = sessions.get(id)?;
        if let Some(origin) = origin {
            if s.local_origin.as_deref() != Some(origin) || !equal(&s.client_key, client_key) {
                return None;
            }
        } else {
            if s.local_origin.is_some() {
                return None;
            }
        }
        let provider = if s.provider.is_empty() && origin.is_some() {
            "One-use sign-in".into()
        } else {
            let config = config?;
            if s.config_revision != revision(config).ok()? {
                return None;
            }
            config
                .providers
                .iter()
                .find(|p| p.id == s.provider && p.admin_subjects.contains(&s.subject))?
                .name
                .clone()
        };
        Some(Account {
            kind: if s.provider.is_empty() {
                "local"
            } else {
                "oidc"
            },
            name: s.name.clone(),
            provider,
            subject: s.subject.clone(),
            role: "Administrator",
            created_at: s.created,
            expires_at: s.expires,
            idle_expires_at: s.last_active + IDLE_SECONDS,
        })
    }
    /// Bind an OIDC session on loopback to this origin and a separate browser proof.
    pub fn bind_local_session(&self, id: &str, origin: &str) -> Result<String> {
        let url = identity_url(origin)?;
        ensure!(
            loopback_url(&url) && url.path() == "/",
            "Local sessions require a loopback origin"
        );
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Login unavailable"))?;
        let session = sessions.get_mut(id).context("Login session expired")?;
        ensure!(
            !session.provider.is_empty() && session.local_origin.is_none(),
            "Session cannot be rebound"
        );
        let proof = token(32)?;
        session.local_origin = Some(url.origin().ascii_serialization());
        session.client_key = proof.clone();
        Ok(proof)
    }
    pub fn activity(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock()
            && let Some(s) = sessions.get_mut(id)
            && s.expires > now()
            && s.last_active + IDLE_SECONDS > now()
        {
            s.last_active = now();
        }
    }
    pub fn logout(&self, id: &str, all: bool) {
        if let Ok(mut sessions) = self.sessions.lock() {
            if all && let Some(s) = sessions.get(id) {
                let identity = (
                    s.provider.clone(),
                    s.subject.clone(),
                    s.local_origin.clone(),
                );
                sessions.retain(|_, s| {
                    (
                        s.provider.clone(),
                        s.subject.clone(),
                        s.local_origin.clone(),
                    ) != identity
                });
            } else {
                sessions.remove(id);
            }
        }
    }
    pub async fn begin(&self, config: &LoginConfig, id: &str) -> Result<(String, String)> {
        let provider = config
            .providers
            .iter()
            .find(|p| p.id == id)
            .context("Unknown login provider")?
            .clone();
        // Bound work before discovery makes outbound requests. This global limit is
        // independent of proxy-controlled IP headers and resets without growing maps.
        let _permit = self
            .discovery
            .try_acquire()
            .context("Too many simultaneous logins; try again later")?;
        {
            let mut attempts = self
                .attempts
                .lock()
                .map_err(|_| anyhow::anyhow!("Login unavailable"))?;
            attempts.retain(|at| *at + 60 > now());
            ensure!(
                attempts.len() < 30,
                "Too many login attempts; try again in a minute"
            );
            attempts.push(now());
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow::anyhow!("Login unavailable"))?;
            pending.retain(|_, p| p.expires > now());
            ensure!(
                pending.len() < 256,
                "Too many pending logins; try again later"
            );
        }
        let http = http_client(&provider)?;
        let issuer_url = identity_url(&provider.issuer)?;
        let local =
            (issuer_url.scheme() == "http").then(|| issuer_url.origin().ascii_serialization());
        let secure_http = |request| secure_request(http.clone(), request, local.clone());
        let metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(provider.issuer.clone())?,
            &secure_http,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Unable to discover this OpenID Connect provider"))?;
        for endpoint in [
            metadata.authorization_endpoint().as_str(),
            metadata
                .token_endpoint()
                .context("Provider has no token endpoint")?
                .as_str(),
        ] {
            let url = identity_url(endpoint)?;
            ensure!(
                url.scheme() == "https"
                    || local.as_deref() == Some(url.origin().ascii_serialization().as_str()),
                "Plaintext identity endpoint must share the explicitly configured loopback issuer origin"
            );
        }
        let client = CoreClient::from_provider_metadata(
            metadata.clone(),
            ClientId::new(provider.client_id.clone()),
            (!provider.client_secret.is_empty())
                .then(|| ClientSecret::new(provider.client_secret.clone())),
        )
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/auth/callback",
            config.origin()
        ))?);
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(challenge)
            .url();
        let browser = token(32)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Login state unavailable"))?;
        pending.retain(|_, p| p.expires > now());
        ensure!(
            pending.len() < 256,
            "Too many pending logins; try again later"
        );
        pending.insert(
            browser.clone(),
            Pending {
                provider,
                config_revision: revision(config)?,
                state: state.secret().clone(),
                nonce,
                verifier,
                metadata,
                expires: now() + 600,
            },
        );
        Ok((url.to_string(), browser))
    }
    pub async fn finish(
        &self,
        config: &LoginConfig,
        browser: &str,
        state: &str,
        code: &str,
    ) -> Result<String> {
        ensure!(
            code.len() <= 8192 && state.len() <= 512,
            "Invalid login callback"
        );
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Login state unavailable"))?
            .remove(browser)
            .context("Login expired or already used")?;
        ensure!(
            pending.expires > now()
                && equal(&pending.state, state)
                && pending.config_revision == revision(config)?,
            "Login expired or state changed; sign in again"
        );
        let http = http_client(&pending.provider)?;
        let issuer_url = identity_url(&pending.provider.issuer)?;
        let local =
            (issuer_url.scheme() == "http").then(|| issuer_url.origin().ascii_serialization());
        let secure_http = |request| secure_request(http.clone(), request, local.clone());
        let client = CoreClient::from_provider_metadata(
            pending.metadata,
            ClientId::new(pending.provider.client_id),
            (!pending.provider.client_secret.is_empty())
                .then(|| ClientSecret::new(pending.provider.client_secret)),
        )
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/auth/callback",
            config.origin()
        ))?);
        let response = client
            .exchange_code(AuthorizationCode::new(code.into()))?
            .set_pkce_verifier(pending.verifier)
            .request_async(&secure_http)
            .await
            .map_err(|_| anyhow::anyhow!("Identity provider rejected the login exchange"))?;
        let id_token = response
            .id_token()
            .context("Provider did not return an identity token")?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &pending.nonce)
            .map_err(|_| anyhow::anyhow!("Identity token validation failed"))?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                response.access_token(),
                id_token.signing_alg()?,
                id_token.signing_key(&verifier)?,
            )?;
            ensure!(&actual == expected, "Access token validation failed");
        }
        let subject = claims.subject().as_str().to_owned();
        ensure!(
            pending.provider.admin_subjects.contains(&subject),
            "This identity has not been granted access to Selfhost"
        );
        let id = token(32)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("Session storage unavailable"))?;
        sessions.retain(|_, s| s.expires > now());
        ensure!(sessions.len() < 1024, "Too many active sessions");
        sessions.insert(
            id.clone(),
            Session {
                provider: pending.provider.id,
                name: claims
                    .preferred_username()
                    .map(|v| v.as_str().to_owned())
                    .unwrap_or_else(|| subject.clone()),
                subject,
                created: now(),
                last_active: now(),
                expires: now() + SESSION_SECONDS,
                config_revision: revision(config)?,
                local_origin: None,
                client_key: String::new(),
            },
        );
        Ok(id)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> LoginConfig {
        LoginConfig {
            public_url: "https://selfhost.example.com".into(),
            providers: vec![LoginProvider {
                id: "identity".into(),
                name: "Identity".into(),
                issuer: "https://id.example.com/realms/home".into(),
                client_id: "selfhost".into(),
                client_secret: "secret".into(),
                ca_certificate: String::new(),
                admin_subjects: vec!["owner-subject".into()],
            }],
        }
    }
    #[test]
    fn explicit_authorization_and_secret_redaction() {
        let mut c = config();
        c.validate().unwrap();
        assert!(!c.public().to_string().contains("\"secret\""));
        c.providers[0].admin_subjects = vec!["*".into()];
        assert!(c.validate().is_err());
        c = config();
        c.public_url = "http://selfhost.example.com".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn loopback_identity_urls_are_explicit_and_public_http_is_rejected() {
        for valid in [
            "http://localhost:8797",
            "http://127.0.0.1:8080/realms/home",
            "http://[::1]:8080",
            "https://identity.example.com",
        ] {
            assert!(identity_url(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "http://localhost.example.com",
            "http://127.0.0.1.example.com",
            "http://192.168.1.2",
            "http://example.com",
            "https://user:secret@example.com",
            "http://localhost:8797?redirect=evil",
        ] {
            assert!(identity_url(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn local_oidc_preserves_authorization_and_requires_origin_proof() {
        for scheme in ["http", "https"] {
            let origin = format!("{scheme}://localhost:8797");
            let other = format!("{scheme}://localhost:9999");
            let state = LoginState::default();
            let c = config();
            state.sessions.lock().unwrap().insert(
                "oidc".into(),
                Session {
                    provider: "identity".into(),
                    subject: "owner-subject".into(),
                    name: "Owner".into(),
                    created: now(),
                    last_active: now(),
                    expires: now() + SESSION_SECONDS,
                    config_revision: revision(&c).unwrap(),
                    local_origin: None,
                    client_key: String::new(),
                },
            );
            let proof = state.bind_local_session("oidc", &origin).unwrap();
            assert!(state.account("oidc", Some(&c), None, "").is_none());
            assert!(state.account("oidc", Some(&c), Some(&origin), "").is_none());
            assert!(
                state
                    .account("oidc", Some(&c), Some(&other), &proof)
                    .is_none()
            );
            assert_eq!(
                state
                    .account("oidc", Some(&c), Some(&origin), &proof)
                    .unwrap()
                    .kind,
                "oidc"
            );
            let mut changed = c;
            changed.providers[0].admin_subjects = vec!["someone-else".into()];
            assert!(
                state
                    .account("oidc", Some(&changed), Some(&origin), &proof)
                    .is_none()
            );
            assert!(state.bind_local_session("oidc", &origin).is_err());
        }
    }
    #[test]
    fn session_revocation_expiry_and_cookie_flags() {
        let c = config();
        let state = LoginState::default();
        state.sessions.lock().unwrap().insert(
            "test".into(),
            Session {
                provider: "identity".into(),
                subject: "owner-subject".into(),
                name: "Owner".into(),
                created: now(),
                last_active: now(),
                local_origin: None,
                client_key: String::new(),
                expires: now() + 60,
                config_revision: revision(&c).unwrap(),
            },
        );
        assert!(state.account("test", Some(&c), None, "").is_some());
        let mut changed = c.clone();
        changed.providers[0].admin_subjects = vec!["someone-else".into()];
        assert!(state.account("test", Some(&changed), None, "").is_none());
        state
            .sessions
            .lock()
            .unwrap()
            .get_mut("test")
            .unwrap()
            .expires = now().saturating_sub(1);
        assert!(state.account("test", Some(&c), None, "").is_none());
        state.logout("test", false);
        assert!(state.account("test", Some(&c), None, "").is_none());
        assert!(
            cookie_value("__Host-selfhost-session", "x", 60)
                .contains("HttpOnly; Secure; SameSite=Lax")
        );
    }
    #[test]
    fn local_sessions_require_both_credentials_and_cannot_replay_recovery() {
        let origin = "http://127.0.0.1:8797";
        let state = LoginState::with_recovery("one-use".into());
        assert!(state.recover("wrong", origin).is_err());
        let (id, proof) = state.recover("one-use", origin).unwrap();
        assert!(state.recover("one-use", origin).is_err());
        assert!(state.account(&id, None, Some(origin), "").is_none());
        assert!(
            state
                .account(&id, None, Some("http://localhost:8797"), &proof)
                .is_none()
        );
        assert!(state.account(&id, Some(&config()), None, &proof).is_none());
        let account = state.account(&id, None, Some(origin), &proof).unwrap();
        assert_eq!(account.kind, "local");
        let json = serde_json::to_string(&account).unwrap();
        assert!(!json.contains(&id) && !json.contains(&proof));
        state.logout(&id, false);
        assert!(state.account(&id, None, Some(origin), &proof).is_none());
        assert!(state.recover("one-use", origin).is_err());
        let expired = LoginState::with_recovery("expired".into());
        expired.recovery.lock().unwrap().as_mut().unwrap().1 = now() - 1;
        assert!(expired.recover("expired", origin).is_err());
    }
    #[test]
    fn setup_session_expires_even_when_active() {
        let state = LoginState::with_recovery("setup-link".into());
        let origin = "http://192.0.2.10:8372";
        let (id, proof) = state.recover_setup("setup-link", origin).unwrap();
        let account = state.account(&id, None, Some(origin), &proof).unwrap();
        assert_eq!(account.name, "Setup administrator");
        assert_eq!(
            account.expires_at - account.created_at,
            SETUP_SESSION_SECONDS
        );
        assert!(state.recover_setup("setup-link", origin).is_err());
        assert!(
            state
                .account(&id, None, Some("http://192.0.2.11:8372"), &proof)
                .is_none()
        );
        state.sessions.lock().unwrap().get_mut(&id).unwrap().expires = now() - 1;
        state.activity(&id);
        assert!(state.account(&id, None, Some(origin), &proof).is_none());
    }
    #[test]
    fn idle_expiry_is_not_extended_by_reads_or_revived_by_activity() {
        let origin = "http://127.0.0.1:8797";
        let state = LoginState::with_recovery("one-use".into());
        let (id, proof) = state.recover("one-use", origin).unwrap();
        let last = now() - 100;
        state
            .sessions
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .last_active = last;
        assert!(state.account(&id, None, Some(origin), &proof).is_some());
        assert_eq!(
            state.sessions.lock().unwrap().get(&id).unwrap().last_active,
            last
        );
        state.activity(&id);
        assert!(state.sessions.lock().unwrap().get(&id).unwrap().last_active > last);
        state
            .sessions
            .lock()
            .unwrap()
            .get_mut(&id)
            .unwrap()
            .last_active = now() - IDLE_SECONDS;
        state.activity(&id);
        assert!(state.account(&id, None, Some(origin), &proof).is_none());
    }
    #[test]
    fn all_sessions_logout_is_scoped_to_provider_and_subject() {
        let state = LoginState::default();
        let c = config();
        for (id, provider, subject) in [
            ("first", "identity", "owner-subject"),
            ("second", "identity", "owner-subject"),
            ("other", "identity", "other-subject"),
            ("other-provider", "elsewhere", "owner-subject"),
        ] {
            state.sessions.lock().unwrap().insert(
                id.into(),
                Session {
                    provider: provider.into(),
                    subject: subject.into(),
                    name: subject.into(),
                    created: now(),
                    last_active: now(),
                    expires: now() + SESSION_SECONDS,
                    config_revision: revision(&c).unwrap(),
                    local_origin: None,
                    client_key: String::new(),
                },
            );
        }
        assert_eq!(
            state.account("first", Some(&c), None, "").unwrap().subject,
            "owner-subject"
        );
        state.logout("first", true);
        let sessions = state.sessions.lock().unwrap();
        assert!(!sessions.contains_key("first") && !sessions.contains_key("second"));
        assert!(sessions.contains_key("other") && sessions.contains_key("other-provider"));
    }
    #[test]
    fn ambiguous_cookies_are_rejected() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cookie", "a=one; a=two".parse().unwrap());
        assert!(cookie(&headers, "a").is_none());
        headers.insert("cookie", "a=one".parse().unwrap());
        headers.append("cookie", "a=two".parse().unwrap());
        assert!(cookie(&headers, "a").is_none());
    }
    #[tokio::test]
    async fn login_limits_reject_before_discovery() {
        let state = LoginState::default();
        let permit = state.discovery.acquire_many(4).await.unwrap();
        assert!(
            state
                .begin(&config(), "identity")
                .await
                .unwrap_err()
                .to_string()
                .contains("simultaneous")
        );
        drop(permit);
        *state.attempts.lock().unwrap() = vec![now(); 30];
        assert!(
            state
                .begin(&config(), "identity")
                .await
                .unwrap_err()
                .to_string()
                .contains("attempts")
        );
    }
    #[tokio::test]
    async fn discovery_transport_rejects_plaintext_before_network_access() {
        let request = axum::http::Request::builder()
            .uri("http://127.0.0.1:1/discovery")
            .body(vec![])
            .unwrap();
        assert!(
            secure_request(http_client(&config().providers[0]).unwrap(), request, None)
                .await
                .unwrap_err()
                .to_string()
                .contains("HTTPS")
        );
        let mut c = config();
        c.providers[0].ca_certificate = "not a PEM certificate".into();
        assert!(c.validate().is_err());
    }
}
