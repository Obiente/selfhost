//! Interactive provider setup and private, expiring configuration transfer.
use crate::{
    auth::LoginConfig,
    core::{Store, atomic_write, now, private_dir, token},
    identity_setup::{self, RegistrationRequest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal, Write},
    path::Path,
};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    schema: u8,
    id: String,
    expires_at: u64,
    config: LoginConfig,
}
const MAX_CODE: usize = 128 * 1024;
fn encode(config: LoginConfig) -> Result<String> {
    config.validate()?;
    let value = Handoff {
        schema: 1,
        id: token(16)?,
        expires_at: now() + 3600,
        config,
    };
    let bytes = serde_json::to_vec(&value)?;
    let result = format!(
        "SHID1.{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    ensure!(
        result.len() <= MAX_CODE,
        "Login configuration is too large for a transfer code"
    );
    Ok(result)
}
fn decode(code: &str) -> Result<Handoff> {
    let code = code.trim();
    ensure!(code.len() <= MAX_CODE, "Transfer code is too large");
    let text = code
        .strip_prefix("SHID1.")
        .context("Paste a Selfhost identity setup code")?;
    ensure!(
        text.len() % 2 == 0 && text.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid transfer code"
    );
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let value: Handoff = serde_json::from_slice(&bytes)?;
    ensure!(
        value.schema == 1
            && value.id.len() == 32
            && value.id.bytes().all(|b| b.is_ascii_hexdigit()),
        "Unsupported transfer code"
    );
    ensure!(
        value.expires_at >= now() && value.expires_at <= now() + 3660,
        "This code expired or its clock is invalid. Generate a new code on the provider host"
    );
    value.config.validate()?;
    ensure!(
        value.config.providers.len() == 1,
        "Transfer one provider connection at a time"
    );
    Ok(value)
}
fn proposed(store: &Store, handoff: &Handoff) -> Result<LoginConfig> {
    let mut config = store.login_config()?.unwrap_or_else(|| LoginConfig {
        public_url: handoff.config.public_url.clone(),
        providers: vec![],
    });
    ensure!(
        config.origin() == handoff.config.origin(),
        "This dashboard uses a different address. Change its address using Access or dashboard domain first, then create a code for that address"
    );
    ensure!(
        !config
            .providers
            .iter()
            .any(|p| p.id == handoff.config.providers[0].id),
        "This provider connection already exists. Edit it under Access instead of importing it again"
    );
    config.providers.extend(handoff.config.providers.clone());
    config.validate()?;
    Ok(config)
}
pub fn handoff_plan(store: &Store, code: &str) -> Result<Value> {
    let handoff = decode(code)?;
    let used = store.root.join("identity-handoffs").join(&handoff.id);
    ensure!(!used.exists(), "This code was already applied");
    let config = proposed(store, &handoff)?;
    let mut plan = identity_setup::plan(store, &config)?;
    plan["administrators"] = json!(handoff.config.providers[0].admin_subjects);
    plan["expires_at"] = json!(handoff.expires_at);
    Ok(plan)
}
pub fn handoff_apply(store: &Store, code: &str, revision: &str, confirmed: bool) -> Result<Value> {
    use fs2::FileExt;
    let handoff = decode(code)?;
    let dir = store.root.join("identity-handoffs");
    private_dir(&dir)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("lock"))?;
    lock.try_lock_exclusive()
        .context("Another identity import is running")?;
    ensure!(
        !dir.join(&handoff.id).exists(),
        "This code was already applied"
    );
    let config = proposed(store, &handoff)?;
    let result = identity_setup::apply(store, config, revision, confirmed)?;
    atomic_write(&dir.join(handoff.id), b"used")?;
    Ok(result)
}
fn ask(label: &str, default: &str) -> Result<String> {
    print!(
        "{label}{}: ",
        if default.is_empty() {
            String::new()
        } else {
            format!(" [{default}]")
        }
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    ensure!(io::stdin().read_line(&mut answer)? > 0, "Setup cancelled");
    ensure!(answer.len() <= 8192, "Answer is too long");
    let answer = answer.trim();
    Ok(if answer.is_empty() {
        default.into()
    } else {
        answer.into()
    })
}
fn yes(label: &str, default: bool) -> Result<bool> {
    let answer = ask(
        &format!("{label} {}", if default { "[Y/n]" } else { "[y/N]" }),
        "",
    )?;
    Ok(if answer.is_empty() {
        default
    } else {
        matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes")
    })
}
fn secret(label: &str) -> Result<String> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        terminal,
    };
    ensure!(
        io::stdin().is_terminal(),
        "Use an interactive terminal for secret input"
    );
    print!("{label}: ");
    io::stdout().flush()?;
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
            let _ = crossterm::execute!(io::stdout(), event::DisableBracketedPaste);
        }
    }
    terminal::enable_raw_mode()?;
    let guard = Guard;
    crossterm::execute!(io::stdout(), event::EnableBracketedPaste)?;
    let mut value = String::new();
    loop {
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Enter => break,
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    anyhow::bail!("Setup cancelled")
                }
                KeyCode::Char(c) if !c.is_control() => value.push(c),
                _ => {}
            },
            Event::Paste(text) => value.push_str(text.trim()),
            _ => {}
        }
        ensure!(value.len() <= MAX_CODE, "Secret input is too long");
    }
    drop(guard);
    println!();
    Ok(value)
}
fn subject_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn show_plan(value: &Value) {
    println!("\nReview the connection:");
    for (key, label) in [
        ("public_url", "Dashboard"),
        ("provider", "Provider"),
        ("issuer", "Issuer"),
        ("project", "Project"),
        ("organization", "Organization"),
        ("callback", "Callback"),
        ("creates", "Creates"),
    ] {
        if let Some(text) = value[key].as_str() {
            println!("  {label}: {text}");
        }
    }
    if let Some(providers) = value["providers"].as_array() {
        for provider in providers {
            println!(
                "  {}: {} (client {})",
                provider["name"].as_str().unwrap_or("Provider"),
                provider["issuer"].as_str().unwrap_or(""),
                provider["client_id"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(administrators) = value["administrators"].as_array() {
        println!(
            "  New administrators: {}",
            administrators
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if value["backup"] == true {
        println!("  Existing login settings will be backed up.");
    }
}
pub async fn connect(store: &Store) -> Result<()> {
    ensure!(
        io::stdin().is_terminal(),
        "Run identity connect in an interactive terminal"
    );
    let code = secret("Paste the private Selfhost setup code")?;
    let plan = handoff_plan(store, &code)?;
    show_plan(&plan);
    if yes(
        "Apply this connection with these exact administrators and registered callback?",
        false,
    )? {
        let result = handoff_apply(store, &code, plan["revision"].as_str().unwrap(), true)?;
        println!(
            "Login configured at {}. Keep local recovery open while testing sign-in.",
            result["public_url"].as_str().unwrap_or("your dashboard")
        );
    } else {
        println!("No login settings changed.");
    }
    Ok(())
}
pub async fn wizard(store: &Store, directory: &Path) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Run identity setup in an interactive terminal"
    );
    println!(
        "Connect this identity provider to Selfhost. Provider configuration files are read without executing them."
    );
    let existing = store.login_config()?;
    let default_url = existing
        .as_ref()
        .map(LoginConfig::origin)
        .unwrap_or_else(|| "http://localhost:8372".into());
    let url = ask("Selfhost browser address", &default_url)?;
    crate::auth::identity_url(&url)?;
    let hints = identity_setup::inspect(directory, &url)?;
    let detected = hints["detected"]
        .as_array()
        .context("Invalid directory inspection")?;
    for item in detected {
        println!(
            "Detected: {} ({})",
            item["name"].as_str().unwrap_or("Provider"),
            item["id"].as_str().unwrap_or("")
        );
    }
    let provider = ask(
        "Provider ID",
        detected
            .first()
            .and_then(|v| v["id"].as_str())
            .unwrap_or("oidc"),
    )?;
    ensure!(
        crate::setup::slug(&provider),
        "Use a lowercase provider ID without spaces"
    );
    let hint = detected.iter().find(|v| v["id"] == provider);
    let issuer = ask(
        "Provider issuer URL",
        hint.and_then(|v| v["issuer_hint"].as_str()).unwrap_or(""),
    )?;
    crate::auth::identity_url(&issuer)?;
    println!(
        "The issuer must be reachable from the dashboard host and users' browsers. A provider's localhost address works only when those clients can reach that same host."
    );
    let id = ask("Connection ID", "home")?;
    ensure!(
        crate::setup::slug(&id),
        "Use a lowercase connection ID without spaces"
    );
    let name = ask(
        "Sign-in button name",
        hint.and_then(|v| v["name"].as_str())
            .unwrap_or("My identity provider"),
    )?;
    let same_default =
        existing.is_some() || store.root.join("dashboard-service/service.json").is_file();
    let same_host = yes(
        "Does this dashboard use Selfhost on this machine and this data directory?",
        same_default,
    )?;
    if same_host {
        println!("Dashboard data directory: {}", store.root.display());
    }
    let workspace = store.root.join("identity-assist").join(&id);
    let prepared = Store::open(workspace)?;
    let config = if let Some(previous) = prepared.login_config()? {
        println!(
            "A prepared connection already exists for {id} at {}.",
            previous.origin()
        );
        ensure!(
            previous.origin() == crate::auth::identity_url(&url)?.origin().ascii_serialization()
                && previous.providers.first().is_some_and(|p|
                    p.issuer.trim_end_matches('/') == issuer.trim_end_matches('/')
                ),
            "Use a different connection ID for a different provider or address"
        );
        ensure!(
            yes(
                "Reuse this prepared connection instead of creating another client?",
                true
            )?,
            "Setup cancelled; use another connection ID for a new client"
        );
        previous
    } else if identity_setup::registration_profile(&provider).is_ok()
        && yes("Create the Selfhost client automatically?", true)?
    {
        let credential = secret("Temporary provider API token (hidden)")?;
        let profile = identity_setup::registration_profile(&provider)?;
        let mut request = RegistrationRequest {
            provider: provider.clone(),
            issuer: issuer.clone(),
            provider_project: String::new(),
            create_project: profile.project_creation.is_some(),
            project_name: "Selfhost".into(),
            organization_id: String::new(),
            selfhost_url: url.clone(),
            id: id.clone(),
            name: name.clone(),
            admin_subjects: vec![],
            ca_certificate: String::new(),
        };
        if !request.create_project {
            request.provider_project = ask("Existing provider project ID", "")?;
        }
        if let Ok(account) = identity_setup::registration_account(&request, &credential).await {
            if let Some(subject) = account["suggested_subject"].as_str()
                && yes(
                    &format!("Allow your provider account ({subject}) to administer Selfhost?"),
                    false,
                )?
            {
                request.admin_subjects.push(subject.into());
            }
            request.organization_id = account["organization_id"].as_str().unwrap_or("").into();
        }
        if request.admin_subjects.is_empty() {
            request.admin_subjects = subject_list(&ask(
                "Exact administrator subject IDs, separated by commas",
                "",
            )?);
        }
        if yes("Configure advanced registration settings?", false)? {
            request.project_name = ask("Project name", &request.project_name)?;
            request.organization_id = ask(
                "Organization ID (blank uses token's organization)",
                &request.organization_id,
            )?;
            if yes("Use an existing provider project?", false)? {
                request.provider_project = ask("Project ID", "")?;
                request.create_project = false;
            }
            let ca = ask("Private CA PEM file (optional)", "")?;
            if !ca.is_empty() {
                request.ca_certificate = std::fs::read_to_string(ca)?;
            }
        }
        let mut plan = identity_setup::registration_plan(&prepared, &request)?;
        plan["administrators"] = json!(request.admin_subjects);
        show_plan(&plan);
        ensure!(
            yes(
                "Create this client and its dedicated project if shown?",
                false
            )?,
            "Setup cancelled before provider changes"
        );
        identity_setup::register(
            &prepared,
            request,
            plan["revision"].as_str().unwrap(),
            &credential,
        )
        .await?;
        prepared
            .login_config()?
            .context("Registration did not produce login settings")?
    } else {
        println!(
            "Register a Web OIDC client with authorization code and PKCE S256. Callback: {}/auth/callback",
            url.trim_end_matches('/')
        );
        if let Some(hint) = hint
            && let Some(instructions) = hint["instructions"].as_array()
        {
            for text in instructions.iter().filter_map(Value::as_str) {
                println!("{text}");
            }
        }
        let client_id = ask("OIDC client ID", "")?;
        let client_secret = secret("OIDC client secret (blank for a public client)")?;
        let admin_subjects = subject_list(&ask(
            "Exact administrator subject IDs, separated by commas",
            "",
        )?);
        let ca = ask("Private CA PEM file (optional)", "")?;
        let ca_certificate = if ca.is_empty() {
            String::new()
        } else {
            std::fs::read_to_string(ca)?
        };
        let mut config = LoginConfig {
            public_url: url.clone(),
            providers: vec![crate::auth::LoginProvider {
                id: id.clone(),
                name,
                issuer,
                client_id,
                client_secret,
                ca_certificate,
                admin_subjects,
            }],
        };
        config.validate()?;
        ensure!(
            yes("Is the exact callback registered for this client?", false)?,
            "Register the callback before completing setup"
        );
        prepared
            .save_login_config_checked(&mut config, Some(&crate::auth::login_revision(None)?))?;
        config
    };
    let code = encode(config)?;
    if same_host {
        let plan = handoff_plan(store, &code)?;
        show_plan(&plan);
        if yes(
            "Apply this login configuration to the dashboard here?",
            false,
        )? {
            handoff_apply(store, &code, plan["revision"].as_str().unwrap(), true)?;
            println!("Login configured. Test sign-in while keeping local recovery open.");
            return Ok(());
        }
    }
    println!(
        "Private setup code, valid for one hour. It contains the OIDC client secret, not your provider API token. Transfer it privately and do not paste it into chat or issue trackers."
    );
    println!("{code}");
    println!(
        "Paste it in Access > Use the CLI, or run this command on the dashboard host (using the dashboard's data directory) and paste it when prompted:\n  selfhost identity connect"
    );
    println!(
        "To issue a fresh code, rerun selfhost identity setup in this directory and reuse the prepared connection."
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> LoginConfig {
        LoginConfig {
            public_url: "http://localhost:8372".into(),
            providers: vec![crate::auth::LoginProvider {
                id: "home".into(),
                name: "Home".into(),
                issuer: "https://identity.example.test".into(),
                client_id: "synthetic-client".into(),
                client_secret: "synthetic-secret".into(),
                ca_certificate: String::new(),
                admin_subjects: vec!["synthetic-admin".into()],
            }],
        }
    }
    #[test]
    fn handoff_is_bounded_expiring_reviewed_and_single_use() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let code = encode(config()).unwrap();
        let plan = handoff_plan(&store, &code).unwrap();
        assert!(!plan.to_string().contains("synthetic-secret"));
        assert_eq!(plan["administrators"][0], "synthetic-admin");
        assert!(handoff_apply(&store, &code, "stale", true).is_err());
        assert!(handoff_apply(&store, &code, plan["revision"].as_str().unwrap(), false).is_err());
        handoff_apply(&store, &code, plan["revision"].as_str().unwrap(), true).unwrap();
        assert!(handoff_plan(&store, &code).is_err());
        assert!(decode("SHID1.zz").is_err());
        assert!(decode(&"a".repeat(MAX_CODE + 1)).is_err());
        let mut expired = decode(&code).unwrap();
        expired.expires_at = now() - 1;
        let bytes = serde_json::to_vec(&expired).unwrap();
        let code = format!(
            "SHID1.{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        assert!(decode(&code).is_err());
    }
}
