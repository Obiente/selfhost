use crate::core::{self, CreateProject, EditProject, Schedule, Store};
use anyhow::Result;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

#[derive(RustEmbed)]
#[folder = "ui/dist/"]
struct Assets;
#[derive(Clone)]
struct App {
    store: Store,
    port: u16,
    login: Arc<crate::auth::LoginState>,
    shutdown: tokio_util::sync::CancellationToken,
}
type AppState = Arc<App>;
struct ApiError(anyhow::Error);
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self(e)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{:#}", self.0)})),
        )
            .into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        response
            .headers_mut()
            .insert("Referrer-Policy", "no-referrer".parse().unwrap());
        response
    }
}
type ApiResult<T> = std::result::Result<Json<T>, ApiError>;
async fn dashboard_runtime(State(app): State<AppState>) -> ApiResult<Value> {
    let mut value = crate::dashboard::summary(&app.store)?;
    value["port"] = json!(app.port);
    Ok(Json(value))
}
async fn stacks(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.stack_catalog()?))
}
async fn update_status(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.cached_update_status()?))
}
async fn update_check(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.update_status().await?))
}
async fn update_plan(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.plan_update().await?))
}
async fn update_stage(
    State(app): State<AppState>,
    Json(input): Json<crate::updates::UpdateApproval>,
) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.stage_update(input)).await?))
}
async fn update_jobs(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.update_jobs()?)))
}
async fn update_activate(
    State(app): State<AppState>,
    Json(input): Json<crate::updates::UpdateActivation>,
) -> ApiResult<Value> {
    let result = app.store.activate_update(input, Some(app.port))?;
    if result["shutdown_required"] == true {
        let shutdown = app.shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            shutdown.cancel();
        });
    }
    Ok(Json(result))
}
async fn update_recover(
    State(app): State<AppState>,
    Json(input): Json<crate::updates::UpdateActivation>,
) -> ApiResult<Value> {
    let result = app.store.recover_update(input, Some(app.port))?;
    if result["shutdown_required"] == true {
        let shutdown = app.shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            shutdown.cancel();
        });
    }
    Ok(Json(result))
}
async fn stack_create(
    State(app): State<AppState>,
    Json(input): Json<crate::stacks::Install>,
) -> ApiResult<Value> {
    let (project, instructions) = app.store.install_blueprint(input)?;
    Ok(Json(json!({"project":project,"instructions":instructions})))
}
async fn networking(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.networking()?))
}
async fn proxy_add(
    State(app): State<AppState>,
    Json(proxy): Json<crate::networking::Proxy>,
) -> ApiResult<Value> {
    Ok(Json(json!({"id":app.store.add_proxy(proxy)?})))
}
async fn proxy_save(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(mut proxy): Json<crate::networking::Proxy>,
) -> ApiResult<Value> {
    proxy.id = id;
    Ok(Json(json!({"id":app.store.save_proxy(proxy)?})))
}
async fn network_add(
    State(app): State<AppState>,
    Json(network): Json<crate::networking::PrivateNetwork>,
) -> ApiResult<Value> {
    Ok(Json(json!({"id":app.store.add_network(network)?})))
}
async fn route_plan(
    State(app): State<AppState>,
    Json(route): Json<crate::networking::Route>,
) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.route_plan(&route)).await?))
}
async fn route_probe(
    State(app): State<AppState>,
    Json(route): Json<crate::networking::Route>,
) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.route_probe(&route)).await?))
}
#[derive(Deserialize)]
struct RouteApply {
    route: crate::networking::Route,
    revision: String,
}
async fn route_apply(
    State(app): State<AppState>,
    Json(input): Json<RouteApply>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.route_apply(input.route, &input.revision)).await?,
    ))
}
fn request_origin(app: &App, headers: &axum::http::HeaderMap) -> Option<(String, bool)> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    // A configured HTTPS proxy may use a loopback name. Honor its configured
    // scheme before the direct-listener aliases, without trusting forwarded headers.
    if let Some(config) = app.store.login_config().ok().flatten() {
        let origin = config.origin();
        if origin
            .strip_prefix("https://")
            .or_else(|| origin.strip_prefix("http://"))
            == Some(host)
        {
            return Some((origin, config.is_loopback()));
        }
    }
    if [
        format!("127.0.0.1:{}", app.port),
        format!("localhost:{}", app.port),
    ]
    .contains(&host.to_string())
    {
        return Some((format!("http://{host}"), true));
    }
    None
}
#[derive(Clone)]
struct Authenticated {
    id: String,
    account: crate::auth::Account,
    cookie_name: String,
    secure: bool,
}
fn session_cookie(app: &App, local: bool) -> String {
    if local {
        format!("selfhost-local-{}", app.port)
    } else {
        "__Host-selfhost-session".into()
    }
}
fn oidc_cookie(app: &App, local: bool, purpose: &str) -> String {
    if local {
        format!("selfhost-oidc-{purpose}-{}", app.port)
    } else {
        format!("__Host-selfhost-{purpose}")
    }
}
fn oidc_cookie_value(
    app: &App,
    local: bool,
    secure: bool,
    purpose: &str,
    value: &str,
    age: u64,
) -> String {
    let name = oidc_cookie(app, local, purpose);
    if !secure {
        format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}")
    } else {
        crate::auth::cookie_value(&name, value, age)
    }
}
fn session_identity(app: &App, headers: &axum::http::HeaderMap) -> Option<Authenticated> {
    let (origin, local) = request_origin(app, headers)?;
    let config = app.store.login_config().ok().flatten();
    let oidc_name = oidc_cookie(app, local, "session");
    if config.as_ref().is_some_and(|c| c.origin() == origin)
        && let Some(id) = crate::auth::cookie(headers, &oidc_name)
    {
        let proof = headers
            .get("x-selfhost-oidc-client")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Some(account) =
            app.login
                .account(id, config.as_ref(), local.then_some(origin.as_str()), proof)
        {
            return Some(Authenticated {
                id: id.into(),
                account,
                cookie_name: oidc_name,
                secure: origin.starts_with("https://"),
            });
        }
    }
    if !local {
        return None;
    }
    let name = session_cookie(app, true);
    let id = crate::auth::cookie(headers, &name)?;
    let proof = headers
        .get("x-selfhost-client")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let account = app
        .login
        .account(id, config.as_ref(), Some(&origin), proof)?;
    Some(Authenticated {
        id: id.into(),
        account,
        cookie_name: name,
        secure: false,
    })
}
fn same_origin(headers: &axum::http::HeaderMap, origin: &str, mutation: bool) -> bool {
    if headers.get_all(header::ORIGIN).iter().count() > 1 {
        return false;
    }
    if let Some(received) = headers.get(header::ORIGIN) {
        if received.to_str().ok() != Some(origin) {
            return false;
        }
    } else if mutation {
        return false;
    }
    // Same-site is insufficient: another managed app can share the parent domain.
    !headers
        .get("sec-fetch-site")
        .is_some_and(|v| v != "same-origin")
}
async fn authorize(State(app): State<AppState>, mut request: Request, next: Next) -> Response {
    let Some((origin, _)) = request_origin(&app, request.headers()) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let mutation = !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    if !same_origin(request.headers(), &origin, mutation) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"Request origin is not allowed."})),
        )
            .into_response();
    }
    let Some(identity) = session_identity(&app, request.headers()) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Your session has ended. Sign in again."})),
        )
            .into_response();
    };
    request.extensions_mut().insert(identity);
    next.run(request).await
}
async fn security_headers(State(app): State<AppState>, request: Request, next: Next) -> Response {
    let public_https = request_origin(&app, request.headers())
        .is_some_and(|(origin, _)| origin.starts_with("https://"));
    let valid_host = request.headers().get_all(header::HOST).iter().count() == 1
        && request_origin(&app, request.headers()).is_some();
    let mut response = if valid_host {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    };
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("cross-origin-resource-policy", "same-origin"),
        ("cross-origin-opener-policy", "same-origin"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        ),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; object-src 'none'; img-src 'self' https: data:; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
    ] {
        response.headers_mut().insert(name, value.parse().unwrap());
    }
    if public_https {
        response.headers_mut().insert(
            "strict-transport-security",
            "max-age=31536000".parse().unwrap(),
        );
    }
    response
}
async fn login_info(State(app): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    let Some((origin, _)) = request_origin(&app, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if !same_origin(&headers, &origin, false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let config = app.store.login_config().ok().flatten();
    Json(json!({"providers":config.as_ref().map(|c|c.providers.iter().map(|p|json!({"id":p.id,"name":p.name,"login_url":format!("{}/auth/login/{}",c.origin(),p.id)})).collect::<Vec<_>>()).unwrap_or_default()})).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    token: String,
}
async fn local_login(
    State(app): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<Recovery>,
) -> Response {
    let Some((origin, true)) = request_origin(&app, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if !origin.starts_with("http://") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !same_origin(&headers, &origin, true) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match app.login.recover(&input.token, &origin) {
        Ok((id, client_key)) => {
            let mut response = Json(json!({"client_key":client_key})).into_response();
            // Local HTTP cookies have no port isolation. A second, origin-scoped key
            // prevents another loopback service from replaying an observed cookie.
            response.headers_mut().insert(header::SET_COOKIE, format!("{}={id}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}", session_cookie(&app, true), crate::auth::SESSION_SECONDS).parse().unwrap());
            response
        }
        Err(_) => (StatusCode::UNAUTHORIZED, Json(json!({"error":"Recovery link expired or already used. Restart selfhost serve for a new link."}))).into_response(),
    }
}
async fn account(
    axum::Extension(identity): axum::Extension<Authenticated>,
) -> Json<crate::auth::Account> {
    Json(identity.account)
}
async fn account_activity(
    State(app): State<AppState>,
    axum::Extension(identity): axum::Extension<Authenticated>,
) -> Json<Value> {
    app.login.activity(&identity.id);
    Json(json!({"ok":true}))
}
async fn login_begin(
    State(app): State<AppState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let config = app
        .store
        .login_config()?
        .ok_or_else(|| anyhow::anyhow!("Configure a login provider first"))?;
    if request_origin(&app, &headers).is_none_or(|(origin, _)| origin != config.origin()) {
        return Err(anyhow::anyhow!("Open Selfhost at its configured address to sign in").into());
    }
    let local = config.is_loopback();
    let (url, browser) = app.login.begin(&config, &id).await?;
    let mut response = axum::response::Redirect::temporary(&url).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        oidc_cookie_value(
            &app,
            local,
            config.origin().starts_with("https://"),
            "login",
            &browser,
            600,
        )
        .parse()
        .unwrap(),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}
#[derive(Deserialize)]
struct LoginCallback {
    code: Option<String>,
    state: Option<String>,
}
async fn login_callback(
    State(app): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<LoginCallback>,
    headers: axum::http::HeaderMap,
) -> Result<Response, ApiError> {
    let config = app
        .store
        .login_config()?
        .ok_or_else(|| anyhow::anyhow!("Login is not configured"))?;
    if request_origin(&app, &headers).is_none_or(|(origin, _)| origin != config.origin()) {
        return Err(anyhow::anyhow!("Invalid callback host").into());
    }
    let local = config.is_loopback();
    let login_cookie_name = oidc_cookie(&app, local, "login");
    let browser = crate::auth::cookie(&headers, &login_cookie_name)
        .ok_or_else(|| anyhow::anyhow!("Login browser state is missing; start again"))?;
    let id = app
        .login
        .finish(
            &config,
            browser,
            query.state.as_deref().unwrap_or(""),
            query.code.as_deref().unwrap_or(""),
        )
        .await?;
    let redirect = if local {
        format!(
            "/#client={}",
            app.login.bind_local_session(&id, &config.origin())?
        )
    } else {
        "/".into()
    };
    let mut response = axum::response::Redirect::to(&redirect).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        oidc_cookie_value(
            &app,
            local,
            config.origin().starts_with("https://"),
            "session",
            &id,
            8 * 3600,
        )
        .parse()
        .unwrap(),
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        oidc_cookie_value(
            &app,
            local,
            config.origin().starts_with("https://"),
            "login",
            "",
            0,
        )
        .parse()
        .unwrap(),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("Referrer-Policy", "no-referrer".parse().unwrap());
    Ok(response)
}
async fn login_logout(
    State(app): State<AppState>,
    axum::Extension(identity): axum::Extension<Authenticated>,
    Json(input): Json<Logout>,
) -> Response {
    app.login.logout(&identity.id, input.all);
    let value = if !identity.secure {
        format!(
            "{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
            identity.cookie_name
        )
    } else {
        crate::auth::cookie_value(&identity.cookie_name, "", 0)
    };
    let mut response = Json(json!({"ok":true})).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, value.parse().unwrap());
    response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Logout {
    #[serde(default)]
    all: bool,
}
async fn login_settings(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(
        app.store
            .login_config()?
            .map(|c| c.public())
            .unwrap_or(json!({"public_url":"","providers":[]})),
    ))
}
async fn login_registration_profiles() -> ApiResult<Value> {
    Ok(Json(json!(crate::identity_setup::registration_profiles()?)))
}
async fn deployments(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.deployments()?))
}
async fn deployment_create(
    State(app): State<AppState>,
    Json(input): Json<crate::deployments::CreateDeployment>,
) -> ApiResult<Value> {
    let (project, instructions) = app.store.create_deployment(input)?;
    Ok(Json(json!({"project":project,"instructions":instructions})))
}
async fn removal_reconcile(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Confirmation>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.reconcile_removal(&id, &input.confirmation)).await?,
    ))
}
async fn login_registration_plan(
    State(app): State<AppState>,
    Json(input): Json<crate::identity_setup::RegistrationRequest>,
) -> ApiResult<Value> {
    Ok(Json(crate::identity_setup::registration_plan(
        &app.store, &input,
    )?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationApply {
    request: crate::identity_setup::RegistrationRequest,
    revision: String,
    credential: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationAccount {
    request: crate::identity_setup::RegistrationRequest,
    credential: String,
}
async fn login_registration_account(Json(input): Json<RegistrationAccount>) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(crate::identity_setup::registration_account(
            &input.request,
            &input.credential,
        ))
        .await?,
    ))
}
async fn login_registration_apply(
    State(app): State<AppState>,
    Json(input): Json<RegistrationApply>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(crate::identity_setup::register(
            &app.store,
            input.request,
            &input.revision,
            &input.credential,
        ))
        .await?,
    ))
}
async fn login_plan(
    State(app): State<AppState>,
    Json(config): Json<crate::auth::LoginConfig>,
) -> ApiResult<Value> {
    Ok(Json(crate::identity_setup::plan(&app.store, &config)?))
}
async fn login_apply(
    State(app): State<AppState>,
    Json(input): Json<crate::identity_setup::ApplyLogin>,
) -> ApiResult<Value> {
    Ok(Json(crate::identity_setup::apply(
        &app.store,
        input.config,
        &input.expected_revision,
        input.callbacks_confirmed,
    )?))
}
async fn existing_profiles(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.existing_profiles()?)))
}
async fn existing_apps(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.existing_apps()?)))
}
async fn existing_link(
    State(app): State<AppState>,
    Json(input): Json<crate::adoption::LinkExisting>,
) -> ApiResult<Value> {
    Ok(Json(json!(Box::pin(app.store.link_existing(input)).await?)))
}
async fn existing_status(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.inspect_existing(&id)).await?))
}
async fn existing_stats(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.existing_stats(&id)).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    confirmation: String,
}
async fn existing_unlink(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Confirmation>,
) -> ApiResult<Value> {
    app.store.unlink_existing(&id, &input.confirmation)?;
    Ok(Json(json!({"ok":true})))
}
async fn existing_consent(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::adoption::ExistingConsent>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.consent_existing(&id, input)?)))
}
async fn existing_action(
    State(app): State<AppState>,
    Path((id, action)): Path<(String, String)>,
    Json(input): Json<Confirmation>,
) -> ApiResult<Value> {
    Ok(Json(
        json!({"output":Box::pin(app.store.existing_action(&id,&action,&input.confirmation)).await?}),
    ))
}
async fn removal_plan(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::removal::RemovalRequest>,
) -> ApiResult<Value> {
    Ok(Json(json!(
        Box::pin(app.store.plan_removal(&id, input)).await?
    )))
}
async fn removal_apply(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::removal::RemovalApply>,
) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.apply_removal(&id, input)).await?))
}
async fn removal_archives(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.removal_archives()?)))
}
async fn removal_restore(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Confirmation>,
) -> ApiResult<Value> {
    Ok(Json(json!(
        app.store
            .restore_removed_project(&id, &input.confirmation)?
    )))
}
async fn state(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(
        json!({"data": app.store.read()?, "catalog": app.store.catalog, "version": env!("CARGO_PKG_VERSION")}),
    ))
}
async fn docker(State(app): State<AppState>) -> Json<Value> {
    Json(match app.store.statuses().await {
        Ok(v) => v,
        Err(e) => json!({"available": false, "message": format!("{e:#}"), "containers": []}),
    })
}
async fn servers(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.servers()?)))
}
async fn add_server(
    State(app): State<AppState>,
    Json(input): Json<crate::infrastructure::Server>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.add_server(input)?)))
}
async fn edit_server(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::infrastructure::Server>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.edit_server(&id, input)?)))
}
async fn inventory(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(app.store.infrastructure_inventory(&id).await?))
}
async fn server_docker(State(app): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    Json(match app.store.server_statuses(&id).await {
        Ok(v) => v,
        Err(e) => json!({"available":false,"message":format!("{e:#}"),"containers":[]}),
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    server_id: String,
}
async fn relocation(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Destination>,
) -> ApiResult<Value> {
    Ok(Json(
        app.store.relocation_plan(&id, &input.server_id).await?,
    ))
}
async fn assign(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Destination>,
) -> ApiResult<Value> {
    Ok(Json(json!(
        app.store.assign_project(&id, &input.server_id).await?
    )))
}
async fn move_project(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Destination>,
) -> ApiResult<Value> {
    Ok(Json(json!(
        app.store.begin_move(&id, &input.server_id).await?
    )))
}
async fn moves(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.move_jobs()?)))
}
async fn recover_move(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.recover_move(&id).await?)))
}
async fn guest_moves(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.guest_moves()?)))
}
async fn guest_plan(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::migration::GuestMove>,
) -> ApiResult<Value> {
    Ok(Json(app.store.guest_move_plan(&id, &input).await?))
}
async fn guest_move(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::migration::GuestMove>,
) -> ApiResult<Value> {
    Ok(Json(app.store.migrate_guest(&id, &input).await?))
}
#[derive(Deserialize)]
struct GuestTask {
    node: String,
    task: String,
}
async fn guest_task(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<GuestTask>,
) -> ApiResult<Value> {
    Ok(Json(
        app.store.guest_task(&id, &input.node, &input.task).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionedProject {
    #[serde(default = "crate::infrastructure::local_id")]
    server_id: String,
    name: String,
    apps: Vec<String>,
    #[serde(default)]
    versions: std::collections::BTreeMap<String, crate::versions::Selection>,
}
async fn create(
    State(app): State<AppState>,
    Json(input): Json<VersionedProject>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.create_versioned(
        CreateProject {
            server_id: input.server_id,
            name: input.name,
            apps: input.apps
        },
        input.versions
    )?)))
}
async fn catalog_versions(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    let recipe = app
        .store
        .catalog
        .iter()
        .find(|recipe| recipe.id == id)
        .ok_or_else(|| ApiError(anyhow::anyhow!("Unknown app")))?;
    Ok(Json(crate::versions::options(recipe)))
}
async fn catalog_version_check(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    Ok(Json(app.store.catalog_version_check(&id).await?))
}
async fn project_versions(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    let project = app.store.project(&id)?;
    let selected = project
        .services
        .iter()
        .find(|item| item.app == service)
        .ok_or_else(|| ApiError(anyhow::anyhow!("Unknown service")))?;
    let mut choices = crate::versions::options(&selected.definition);
    choices["current_image"] = json!(selected.image);
    Ok(Json(choices))
}
async fn version_plan(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<crate::versions::Selection>,
) -> ApiResult<Value> {
    Ok(Json(app.store.version_plan(&id, &service, &input)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionApply {
    selection: crate::versions::Selection,
    revision: String,
}
async fn version_apply(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<VersionApply>,
) -> ApiResult<Value> {
    Ok(Json(app.store.version_apply(
        &id,
        &service,
        &input.selection,
        &input.revision,
    )?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomSetup {
    name: String,
    server_id: String,
    setup: crate::setup::Setup,
}
async fn create_setup(
    State(app): State<AppState>,
    Json(input): Json<CustomSetup>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.create_setup(
        &input.name,
        &input.server_id,
        input.setup
    )?)))
}
async fn get_setup(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.setup(&id)?)))
}
async fn save_setup(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(setup): Json<crate::setup::Setup>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.save_setup(&id, setup)?)))
}
async fn export_setup(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let bytes = app.store.export_setup(&id)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-tar"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=setup.tar",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
#[derive(Deserialize)]
struct ComposeText {
    text: String,
}
async fn parse_compose(Json(input): Json<ComposeText>) -> ApiResult<Value> {
    Ok(Json(crate::setup::parse_compose(&input.text)?))
}
async fn databases(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.database_sources()?)))
}
async fn database_engines() -> ApiResult<Value> {
    Ok(Json(json!(crate::catalog::database_drivers()?)))
}
async fn add_database(
    State(app): State<AppState>,
    Json(input): Json<crate::database::NewSource>,
) -> ApiResult<Value> {
    Ok(Json(json!({"id":app.store.add_database_source(input)?})))
}
async fn start_database(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    app.store.start_database_source(&id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn select_database(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::database::Selection>,
) -> ApiResult<Value> {
    app.store.configure_database(&id, input)?;
    Ok(Json(json!({"ok":true})))
}
async fn provision_database(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    app.store.provision_database(&id).await?;
    Ok(Json(json!({"ok":true})))
}
async fn download_database_backup(
    State(app): State<AppState>,
    Path((id, backup)): Path<(String, String)>,
) -> std::result::Result<Response, ApiError> {
    let file = app.store.database_backup_file(&id, &backup)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=database.dump",
            ),
        ],
        Body::from_stream(tokio_util::io::ReaderStream::new(
            tokio::fs::File::from_std(file),
        )),
    )
        .into_response())
}
async fn list_database_backups(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.database_backups(&id)?)))
}
async fn backup_database(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.backup_database(&id).await?)))
}
#[derive(Deserialize)]
struct RestoreDatabase {
    confirm_project: String,
}
async fn restore_database(
    State(app): State<AppState>,
    Path((id, backup)): Path<(String, String)>,
    Json(input): Json<RestoreDatabase>,
) -> ApiResult<Value> {
    Ok(Json(
        json!({"safety_backup":app.store.restore_database(&id,&backup,&input.confirm_project).await?}),
    ))
}
async fn edit(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<EditProject>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.edit(&id, input)?)))
}
async fn plan(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(app.store.render(&id)?))
}
#[derive(Deserialize)]
struct Action {
    action: String,
}
async fn action(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Action>,
) -> ApiResult<Value> {
    app.store.action(&id, &input.action).await?;
    Ok(Json(json!({"ok": true})))
}
async fn schedule(State(app): State<AppState>, Json(input): Json<Schedule>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.schedule(input)?)))
}
async fn tasks_list(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.tasks()?))
}
async fn task_destinations(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(app.store.task_destinations()?))
}
async fn task_plan(
    State(app): State<AppState>,
    Json(input): Json<crate::tasks::TaskRequest>,
) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.task_plan(input)).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskCreate {
    request: crate::tasks::TaskRequest,
    revision: String,
}
async fn task_create(
    State(app): State<AppState>,
    Json(input): Json<TaskCreate>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.task_create(input.request, &input.revision)).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskEnabled {
    enabled: bool,
}
async fn task_enabled(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<TaskEnabled>,
) -> ApiResult<Value> {
    Ok(Json(app.store.task_set_enabled(&id, input.enabled)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskRemoval {
    confirmation: String,
}
async fn task_remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<TaskRemoval>,
) -> ApiResult<Value> {
    app.store.task_remove(&id, &input.confirmation)?;
    Ok(Json(json!({"removed": true})))
}
async fn task_run(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(Box::pin(app.store.task_run(&id)).await?))
}
async fn service_stats(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(app.store.service_stats(&id, &service).await?))
}
async fn service_logs(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(
        json!({"logs": app.store.service_logs(&id, &service).await?}),
    ))
}
#[derive(Deserialize)]
struct AppChanges {
    #[serde(default)]
    values: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    revision: String,
}
#[derive(Deserialize)]
struct AppAction {
    action: String,
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, Value>,
}
async fn app_profile(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.integration(&id, &service)?)))
}
async fn app_config(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(app.store.integration_state(&id, &service).await?))
}
#[derive(Deserialize)]
struct AppRestore {
    backup: String,
    revision: Option<String>,
}
async fn app_backups(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(json!(app.store.integration_backups(&id, &service)?)))
}
async fn app_restore(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<AppRestore>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.integration_restore(
            &id,
            &service,
            &input.backup,
            input.revision.as_deref(),
        ))
        .await?,
    ))
}
async fn identity_providers(State(app): State<AppState>) -> ApiResult<Value> {
    Ok(Json(json!(
        app.store
            .identity_providers()?
            .into_iter()
            .map(|(id, p)| {
                let creates_project = crate::identity_setup::registration_profile(&id)
                    .is_ok_and(|profile| profile.project_creation.is_some());
                json!({"id":id,"name":p.name,"creates_project":creates_project})
            })
            .collect::<Vec<_>>()
    )))
}
async fn connection_state(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(app.store.connection_state(&id, &service)?))
}
async fn connection_plan(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<crate::connections::ConnectRequest>,
) -> ApiResult<Value> {
    Ok(Json(app.store.connection_plan(&id, &service, &input)?))
}
async fn connection_create(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<crate::connections::ConnectRequest>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.connection_create(&id, &service, input)).await?,
    ))
}
async fn connection_account(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<crate::connections::ConnectRequest>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.connection_account(&id, &service, &input)).await?,
    ))
}
async fn onboarding_info(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(app.store.onboarding_info(&id, &service)?))
}
async fn onboarding_plan(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<crate::onboarding::OnboardingRequest>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.onboarding_plan(&id, &service, input)).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OnboardingApply {
    request: crate::onboarding::OnboardingRequest,
    revision: String,
}
async fn onboarding_apply(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<OnboardingApply>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(
            app.store
                .onboarding_apply(&id, &service, input.request, &input.revision),
        )
        .await?,
    ))
}
async fn connection_resume(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
) -> ApiResult<Value> {
    Ok(Json(
        Box::pin(app.store.connection_resume(&id, &service)).await?,
    ))
}
async fn app_plan(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<AppChanges>,
) -> ApiResult<Value> {
    Ok(Json(
        app.store
            .integration_plan(&id, &service, &input.values)
            .await?,
    ))
}
async fn app_apply(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<AppChanges>,
) -> ApiResult<Value> {
    Ok(Json(
        app.store
            .integration_apply(&id, &service, input.values, &input.revision)
            .await?,
    ))
}
async fn app_action(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<AppAction>,
) -> ApiResult<Value> {
    Ok(Json(
        app.store
            .integration_action(&id, &service, &input.action, input.inputs)
            .await?,
    ))
}
async fn service_action(
    State(app): State<AppState>,
    Path((id, service)): Path<(String, String)>,
    Json(input): Json<Action>,
) -> ApiResult<Value> {
    Ok(Json(
        json!({"output": app.store.service_action(&id, &service, &input.action).await?}),
    ))
}
async fn mark_read(State(app): State<AppState>) -> ApiResult<Value> {
    app.store.mark_read()?;
    Ok(Json(json!({"ok": true})))
}
async fn restore(State(app): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    Ok(Json(json!(app.store.restore_config(&id)?)))
}
#[derive(Deserialize)]
struct Sync {
    target: String,
    key: String,
}
async fn sync(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Sync>,
) -> ApiResult<Value> {
    Ok(Json(
        json!({"added": app.store.dashboard_sync(&id, &input.target, input.key).await?}),
    ))
}
async fn asset(request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Some(file) = Assets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder().header(header::CONTENT_TYPE, mime.as_ref()).header(header::CACHE_CONTROL, "no-store").header("X-Content-Type-Options", "nosniff").header("Referrer-Policy", "no-referrer").header("Content-Security-Policy", "default-src 'self'; img-src 'self' https: data:; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'").body(Body::from(file.data.into_owned())).unwrap()
}
fn router(app: AppState) -> Router {
    let api = Router::new()
        .route("/updates", get(update_status))
        .route("/dashboard/runtime", get(dashboard_runtime))
        .route("/updates/check", post(update_check))
        .route("/updates/plan", post(update_plan))
        .route("/updates/stage", post(update_stage))
        .route("/updates/activate", post(update_activate))
        .route("/updates/recover", post(update_recover))
        .route("/updates/jobs", get(update_jobs))
        .route("/account", get(account))
        .route("/account/activity", post(account_activity))
        .route(
            "/login/registration-providers",
            get(login_registration_profiles),
        )
        .route("/deployments", get(deployments).post(deployment_create))
        .route("/removal/archives/{id}/reconcile", post(removal_reconcile))
        .route("/login/register/plan", post(login_registration_plan))
        .route("/login/register/account", post(login_registration_account))
        .route("/login/register/apply", post(login_registration_apply))
        .route("/login/plan", post(login_plan))
        .route("/login/apply", post(login_apply))
        .route("/existing/profiles", get(existing_profiles))
        .route("/existing", get(existing_apps).post(existing_link))
        .route("/existing/{id}", axum::routing::delete(existing_unlink))
        .route("/existing/{id}/status", get(existing_status))
        .route("/existing/{id}/stats", get(existing_stats))
        .route("/existing/{id}/permissions", put(existing_consent))
        .route("/existing/{id}/actions/{action}", post(existing_action))
        .route("/projects/{id}/removal/plan", post(removal_plan))
        .route("/projects/{id}/removal/apply", post(removal_apply))
        .route("/removal/archives", get(removal_archives))
        .route("/removal/archives/{id}/restore", post(removal_restore))
        .route("/state", get(state))
        .route("/stacks", get(stacks).post(stack_create))
        .route("/networking", get(networking))
        .route("/networking/proxies", post(proxy_add))
        .route("/networking/proxies/{id}", put(proxy_save))
        .route("/networking/networks", post(network_add))
        .route("/networking/routes/plan", post(route_plan))
        .route("/networking/routes/apply", post(route_apply))
        .route("/networking/routes/probe", post(route_probe))
        .route("/login/settings", get(login_settings))
        .route("/login/logout", post(login_logout))
        .route("/databases", get(databases).post(add_database))
        .route("/database-engines", get(database_engines))
        .route("/databases/{id}/start", post(start_database))
        .route("/projects/{id}/database", post(select_database))
        .route(
            "/projects/{id}/database/provision",
            post(provision_database),
        )
        .route("/setups", post(create_setup))
        .route("/compose/parse", post(parse_compose))
        .route("/projects/{id}/setup", get(get_setup).put(save_setup))
        .route("/projects/{id}/export", get(export_setup))
        .route("/docker", get(docker))
        .route("/guest-moves", get(guest_moves))
        .route("/moves", get(moves))
        .route("/moves/{id}/recover", post(recover_move))
        .route("/projects/{id}/move", post(move_project))
        .route("/servers/{id}/guest-move-plan", post(guest_plan))
        .route("/servers/{id}/guest-move", post(guest_move))
        .route("/servers/{id}/guest-task", post(guest_task))
        .route("/servers", get(servers).post(add_server))
        .route("/servers/{id}", put(edit_server))
        .route("/servers/{id}/inventory", get(inventory))
        .route("/servers/{id}/docker", get(server_docker))
        .route("/projects/{id}/relocation", post(relocation))
        .route("/projects/{id}/placement", put(assign))
        .route("/projects", post(create))
        .route(
            "/projects/{id}/database/backups",
            get(list_database_backups).post(backup_database),
        )
        .route(
            "/projects/{id}/database/backups/{backup}/restore",
            post(restore_database),
        )
        .route(
            "/projects/{id}/database/backups/{backup}/download",
            get(download_database_backup),
        )
        .route(
            "/projects/{id}/services/{service}/integration",
            get(app_profile),
        )
        .route("/identity-providers", get(identity_providers))
        .route(
            "/projects/{id}/services/{service}/onboarding",
            get(onboarding_info),
        )
        .route(
            "/projects/{id}/services/{service}/onboarding/plan",
            post(onboarding_plan),
        )
        .route(
            "/projects/{id}/services/{service}/onboarding/apply",
            post(onboarding_apply),
        )
        .route(
            "/projects/{id}/services/{service}/integration/connection/account",
            post(connection_account),
        )
        .route(
            "/projects/{id}/services/{service}/integration/backups",
            get(app_backups),
        )
        .route(
            "/projects/{id}/services/{service}/integration/restore",
            post(app_restore),
        )
        .route(
            "/projects/{id}/services/{service}/integration/connection",
            get(connection_state),
        )
        .route(
            "/projects/{id}/services/{service}/integration/connection/plan",
            post(connection_plan),
        )
        .route(
            "/projects/{id}/services/{service}/integration/connection/create",
            post(connection_create),
        )
        .route(
            "/projects/{id}/services/{service}/integration/connection/resume",
            post(connection_resume),
        )
        .route(
            "/projects/{id}/services/{service}/integration/config",
            get(app_config),
        )
        .route(
            "/projects/{id}/services/{service}/integration/plan",
            post(app_plan),
        )
        .route(
            "/projects/{id}/services/{service}/integration/apply",
            post(app_apply),
        )
        .route(
            "/projects/{id}/services/{service}/integration/actions",
            post(app_action),
        )
        .route("/projects/{id}", put(edit))
        .route("/projects/{id}/plan", get(plan))
        .route("/projects/{id}/actions", post(action))
        .route("/projects/{id}/sync", post(sync))
        .route(
            "/projects/{id}/services/{service}/stats",
            get(service_stats),
        )
        .route("/projects/{id}/services/{service}/logs", get(service_logs))
        .route(
            "/projects/{id}/services/{service}/actions",
            post(service_action),
        )
        .route("/schedules", post(schedule))
        .route("/tasks", get(tasks_list).post(task_create))
        .route("/catalog/{id}/versions", get(catalog_versions))
        .route("/catalog/{id}/versions/check", post(catalog_version_check))
        .route(
            "/projects/{id}/services/{service}/versions",
            get(project_versions),
        )
        .route(
            "/projects/{id}/services/{service}/version/plan",
            post(version_plan),
        )
        .route(
            "/projects/{id}/services/{service}/version/apply",
            post(version_apply),
        )
        .route("/tasks/destinations", get(task_destinations))
        .route("/tasks/plan", post(task_plan))
        .route("/tasks/{id}", axum::routing::delete(task_remove))
        .route("/tasks/{id}/enabled", post(task_enabled))
        .route("/tasks/{id}/run", post(task_run))
        .route("/notifications/read", post(mark_read))
        .route("/snapshots/{id}/restore", post(restore))
        .route_layer(middleware::from_fn_with_state(app.clone(), authorize));
    Router::new()
        .nest("/api", api)
        .route("/auth/local", post(local_login))
        .route("/auth/info", get(login_info))
        .route("/auth/login/{id}", get(login_begin))
        .route("/auth/callback", get(login_callback))
        .fallback(asset)
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            app.clone(),
            security_headers,
        ))
        .with_state(app)
}
async fn termination_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _=tokio::signal::ctrl_c()=>{},_=signal.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
pub async fn serve(store: Store, port: u16) -> Result<()> {
    // Keep a usable direct HTTP recovery alias when an HTTPS proxy uses the
    // listener's numeric loopback host and port as its configured public address.
    let recovery_host = if store
        .login_config()
        .ok()
        .flatten()
        .is_some_and(|c| c.origin() == format!("https://127.0.0.1:{port}"))
    {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let recovery = core::token(32)?;
    let app = Arc::new(App {
        store: store.clone(),
        login: Arc::new(crate::auth::LoginState::with_recovery(recovery.clone())),
        port: listener.local_addr()?.port(),
        shutdown: tokio_util::sync::CancellationToken::new(),
    });
    let router = router(app.clone());
    println!(
        "selfhost dashboard: http://{recovery_host}:{}/#token={}",
        app.port, recovery
    );
    println!("This local sign-in link expires in 10 minutes and can be used once.");
    println!("Schedules run while this process is open. Press Ctrl+C to stop.");
    let scheduler_shutdown = app.shutdown.clone();
    let scheduler = tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(30));
        let mut jobs = tokio::task::JoinSet::new();
        let mut task_runner: Option<tokio::task::JoinHandle<()>> = None;
        loop {
            tokio::select! {
                _=scheduler_shutdown.cancelled()=>break,
                _=timer.tick()=>{},
                Some(_)=jobs.join_next(), if !jobs.is_empty()=>continue,
            }
            if task_runner.as_ref().is_none_or(|job| job.is_finished()) {
                let task_store = store.clone();
                task_runner = Some(tokio::spawn(async move {
                    if let Err(error) = Box::pin(task_store.tasks_tick()).await {
                        eprintln!("Automatic tasks could not run: {error:#}");
                    }
                }));
            }
            match store.due() {
                Ok(schedules) => {
                    for schedule in schedules {
                        let store = store.clone();
                        jobs.spawn(async move {
                            if let Err(error) = store
                                .scheduled_action(&schedule.project_id, &schedule.action)
                                .await
                            {
                                eprintln!("Scheduled action failed: {error:#}");
                            }
                        });
                    }
                }
                Err(error) => eprintln!("Unable to read schedules: {error:#}"),
            }
        }
        while jobs.join_next().await.is_some() {}
        if let Some(job) = task_runner {
            let _ = job.await;
        }
    });
    let shutdown = app.shutdown.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! { _=termination_signal()=>{},_=shutdown.cancelled()=>{} }
            shutdown.cancel();
        })
        .await?;
    scheduler.await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    const HOST: &str = "127.0.0.1:8797";
    const ORIGIN: &str = "http://127.0.0.1:8797";
    fn app() -> (tempfile::TempDir, AppState) {
        let directory = tempfile::tempdir().unwrap();
        let app = Arc::new(App {
            store: Store::open(directory.path().into()).unwrap(),
            port: 8797,
            shutdown: tokio_util::sync::CancellationToken::new(),
            login: Arc::new(crate::auth::LoginState::with_recovery("test-link".into())),
        });
        (directory, app)
    }
    fn request(method: &str, path: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(path)
            .header("host", HOST)
            .header("content-type", "application/json")
    }
    async fn exchange(router: &Router) -> (String, String) {
        let response = router
            .clone()
            .oneshot(
                request("POST", "/auth/local")
                    .header("origin", ORIGIN)
                    .body(Body::from(r#"{"token":"test-link"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let set = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(set.contains("HttpOnly; SameSite=Strict"));
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap(),
        )
        .unwrap();
        (
            set.split(';').next().unwrap().into(),
            value["client_key"].as_str().unwrap().into(),
        )
    }
    #[tokio::test]
    async fn account_recovery_logout_and_api_authorization() {
        let (_directory, app) = app();
        let router = router(app);
        for (method, path) in [
            ("GET", "/api/account"),
            ("GET", "/api/state"),
            ("GET", "/api/login/settings"),
            ("POST", "/api/projects"),
            ("GET", "/api/projects/missing/export"),
            ("POST", "/api/projects/missing/services/app/actions"),
            ("GET", "/api/projects/missing/setup"),
            ("POST", "/api/networking/routes/apply"),
            ("PUT", "/api/login/settings"),
            ("GET", "/api/tasks"),
            ("GET", "/api/tasks/destinations"),
            ("POST", "/api/tasks/plan"),
            ("POST", "/api/tasks"),
            ("DELETE", "/api/tasks/missing"),
            ("POST", "/api/tasks/missing/enabled"),
            ("POST", "/api/tasks/missing/run"),
            ("GET", "/api/catalog/gotify/versions"),
            ("POST", "/api/catalog/gotify/versions/check"),
            ("GET", "/api/projects/missing/services/gotify/versions"),
            ("POST", "/api/projects/missing/services/gotify/version/plan"),
            (
                "POST",
                "/api/projects/missing/services/gotify/version/apply",
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    request(method, path)
                        .header("origin", ORIGIN)
                        .header("x-selfhost-token", "test-link")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {path}"
            );
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["x-frame-options"], "DENY");
        }
        let (cookie, proof) = exchange(&router).await;
        for key in ["", "wrong", &proof] {
            let response = router
                .clone()
                .oneshot(
                    request("GET", "/api/account")
                        .header("cookie", &cookie)
                        .header("x-selfhost-client", key)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            if key == proof {
                assert_eq!(response.status(), StatusCode::OK);
                let value: Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), 8192)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(value["kind"], "local");
                assert_eq!(value["role"], "Administrator");
                assert!(!value.to_string().contains(&proof));
            } else {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
        }
        let response = router
            .clone()
            .oneshot(
                request("POST", "/api/login/logout")
                    .header("cookie", &cookie)
                    .header("x-selfhost-client", &proof)
                    .header("origin", ORIGIN)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        let response = router
            .clone()
            .oneshot(
                request("GET", "/api/account")
                    .header("cookie", cookie)
                    .header("x-selfhost-client", proof)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = router
            .oneshot(
                request("POST", "/auth/local")
                    .header("origin", ORIGIN)
                    .body(Body::from(r#"{"token":"test-link"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn legacy_settings_put_cannot_skip_review_even_for_an_administrator() {
        let (_directory, app) = app();
        let router = router(app.clone());
        let (cookie, proof) = exchange(&router).await;
        let response = router
            .oneshot(
                request("PUT", "/api/login/settings")
                    .header("cookie", cookie)
                    .header("x-selfhost-client", proof)
                    .header("origin", ORIGIN)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(app.store.login_config().unwrap().is_none());
    }
    #[tokio::test]
    async fn https_loopback_keeps_secure_cookies_and_requires_local_session_binding() {
        let (_directory, app) = app();
        app.store
            .save_login_config(crate::auth::LoginConfig {
                public_url: "https://localhost:8797".into(),
                providers: vec![crate::auth::LoginProvider {
                    id: "home".into(),
                    name: "Home".into(),
                    issuer: "https://identity.example.com".into(),
                    client_id: "selfhost".into(),
                    client_secret: String::new(),
                    ca_certificate: String::new(),
                    admin_subjects: vec!["owner".into()],
                }],
            })
            .unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("host", "localhost:8797".parse().unwrap());
        assert_eq!(
            request_origin(&app, &headers),
            Some(("https://localhost:8797".into(), true))
        );
        assert!(
            oidc_cookie_value(&app, true, true, "session", "synthetic", 60)
                .starts_with("selfhost-oidc-session-8797=")
        );
        assert!(
            oidc_cookie_value(&app, true, true, "session", "synthetic", 60)
                .contains("HttpOnly; Secure; SameSite=Lax")
        );
        headers.insert("host", HOST.parse().unwrap());
        assert_eq!(request_origin(&app, &headers), Some((ORIGIN.into(), true)));
        let response = router(app)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/local")
                    .header("host", "localhost:8797")
                    .header("origin", "https://localhost:8797")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"token":"test-link"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "Recovery exchange remains limited to the direct HTTP listener"
        );
    }
    #[tokio::test]
    async fn csrf_host_fetch_metadata_and_cookie_ambiguity_fail_closed() {
        let (_directory, app) = app();
        let router = router(app);
        let (cookie, proof) = exchange(&router).await;
        for (origin, site) in [
            (None, None),
            (Some("http://localhost:8797"), None),
            (Some("null"), None),
            (Some(ORIGIN), Some("same-site")),
            (Some(ORIGIN), Some("cross-site")),
        ] {
            let mut req = request("POST", "/api/account/activity")
                .header("cookie", &cookie)
                .header("x-selfhost-client", &proof);
            if let Some(origin) = origin {
                req = req.header("origin", origin);
            }
            if let Some(site) = site {
                req = req.header("sec-fetch-site", site);
            }
            let response = router
                .clone()
                .oneshot(req.body(Body::from("{}")).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let response = router
            .clone()
            .oneshot(
                request("POST", "/api/account/activity")
                    .header("origin", ORIGIN)
                    .header("sec-fetch-site", "same-origin")
                    .header("cookie", &cookie)
                    .header("x-selfhost-client", &proof)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router
            .clone()
            .oneshot(
                request("GET", "/api/account")
                    .header("cookie", format!("{cookie}; {cookie}"))
                    .header("x-selfhost-client", &proof)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/account")
                    .header("host", "attacker.example")
                    .header("x-forwarded-host", HOST)
                    .header("cookie", cookie)
                    .header("x-selfhost-client", proof)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    #[tokio::test]
    async fn body_limits_and_security_headers_cover_errors_and_assets() {
        let (_directory, app) = app();
        let router = router(app);
        let response = router
            .clone()
            .oneshot(
                request("POST", "/auth/local")
                    .header("origin", ORIGIN)
                    .body(Body::from("x".repeat(2 * 1024 * 1024 + 1)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        for path in ["/", "/missing", "/auth/info"] {
            let response = router
                .clone()
                .oneshot(request("GET", path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert_eq!(response.headers()["referrer-policy"], "no-referrer");
            assert!(
                response.headers()["content-security-policy"]
                    .to_str()
                    .unwrap()
                    .contains("object-src 'none'")
            );
        }
    }
}
