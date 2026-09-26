//! Reviewed updates for official Cargo installations. No client supplies a URL or executable path.
use crate::core::{Store, atomic_write, now, private_dir, token};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

const SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const EXE: &str = if cfg!(windows) {
    "selfhost.exe"
} else {
    "selfhost"
};
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateApproval {
    pub plan_id: String,
    pub revision: String,
    pub confirmation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateActivation {
    pub job_id: String,
    pub confirmation: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Installation {
    method: String,
    executable: PathBuf,
    hash: String,
    cargo_root: Option<PathBuf>,
    key: Option<String>,
    metadata: Option<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    id: String,
    revision: String,
    version: String,
    confirmation: String,
    expires_at: u64,
    installation: Installation,
    scope: Vec<String>,
    warnings: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Job {
    job_id: String,
    status: String,
    version: String,
    confirmation: String,
    plan: Plan,
    staged_hash: Option<String>,
    parent_pid: Option<u32>,
    restart_port: Option<u16>,
    #[serde(default)]
    restart_bind: Option<std::net::IpAddr>,
    #[serde(default)]
    restart_catalog: Option<PathBuf>,
    #[serde(default)]
    restart_pid: Option<u32>,
    #[serde(default)]
    restart_status: Option<String>,
    data_dir: PathBuf,
    error: Option<String>,
}

fn hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}
fn stable_version(value: &str) -> Result<(u64, u64, u64)> {
    let components: Vec<_> = value.split('.').collect();
    ensure!(
        components.len() == 3
            && components.iter().all(|s| !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_digit())
                && (s.len() == 1 || !s.starts_with('0'))),
        "Only exact stable release versions are supported"
    );
    Ok((
        components[0].parse()?,
        components[1].parse()?,
        components[2].parse()?,
    ))
}
fn bounded_json(path: &Path) -> Result<Value> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "Update metadata is too large"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn installation_at(executable: PathBuf, debug: bool) -> Result<Installation> {
    let executable = fs::canonicalize(executable)?;
    let mut found = Installation {
        hash: hash(&executable)?,
        executable: executable.clone(),
        method: if debug { "source" } else { "standalone" }.into(),
        cargo_root: None,
        key: None,
        metadata: None,
    };
    if debug {
        return Ok(found);
    }
    let bin = executable.parent().context("Executable has no parent")?;
    if bin.file_name().and_then(|s| s.to_str()) == Some("native")
        && let (Ok(package), Ok(manifest)) = (
            bounded_json(
                &bin.parent()
                    .context("Missing package root")?
                    .join("package.json"),
            ),
            bounded_json(&bin.join("manifest.json")),
        )
    {
        let bundled = manifest["binaries"].as_object().is_some_and(|entries| {
            entries.values().any(|entry| {
                entry["file"].as_str() == executable.file_name().and_then(|n| n.to_str())
                    && entry["sha256"].as_str() == Some(&found.hash)
            })
        });
        if package["name"] == "selfhost" && manifest["schema"] == 1 && bundled {
            found.method = "npm".into();
            return Ok(found);
        }
    }
    if bin.file_name().and_then(|s| s.to_str()) != Some("bin")
        || executable.file_name().and_then(|s| s.to_str()) != Some(EXE)
    {
        return Ok(found);
    }
    let root = bin.parent().context("Missing Cargo root")?;
    if let Ok(metadata) = bounded_json(&root.join(".crates2.json"))
        && let Some(installs) = metadata["installs"].as_object()
    {
        let candidates: Vec<_> = installs
            .iter()
            .filter(|(key, value)| {
                key.starts_with("selfhost ")
                    && value["bins"]
                        .as_array()
                        .is_some_and(|bins| bins.iter().any(|bin| bin == EXE))
            })
            .collect();
        if candidates.len() == 1 {
            let (key, entry) = candidates[0];
            let expected = format!("selfhost {VERSION} ({SOURCE})");
            if key == &expected
                && entry["profile"] == "release"
                && entry["bins"] == json!([EXE])
                && root.join(".crates.toml").is_file()
            {
                found.method = "cargo".into();
                found.cargo_root = Some(root.into());
                found.key = Some(key.clone());
                found.metadata = Some(entry.clone());
            } else {
                found.method = "source".into();
            }
        }
    }
    Ok(found)
}
fn installation() -> Result<Installation> {
    installation_at(std::env::current_exe()?, cfg!(debug_assertions))
}
fn instructions(method: &str, version: Option<&str>) -> Vec<String> {
    match method {
        "cargo"=>vec!["Review the update, build it in a private staging directory, then activate it. Activation restarts this dashboard only when requested from the dashboard.".into()],
        "npm"=>vec![version.map(|v| format!("Use an explicitly released launcher version: npx --yes --package=selfhost@{v} selfhost")).unwrap_or("Check the npm package for a released CLI launcher before changing your invocation.".into()),"This copy may be in an npx cache or a local package. Selfhost does not overwrite npm-managed binaries.".into()],
        "source"=>vec!["This is a development, source, or unverified Cargo build. Update your checkout, review its changes, and rebuild using your existing workflow.".into()],
        _=>vec!["This executable is not an identified official Cargo installation or verified npm bundle. Update it using its original installation method.".into()]
    }
}
fn lock(path: &Path) -> Result<File> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    f.try_lock_exclusive()
        .context("Another update operation is running")?;
    Ok(f)
}
fn save(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value)?)
}
fn id(value: &str) -> Result<()> {
    ensure!(
        value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid update identifier"
    );
    Ok(())
}
fn restart_log(job: &Job) -> PathBuf {
    job.data_dir
        .join("updates")
        .join(&job.job_id)
        .join("restart.log")
}
fn recovery_instructions(job: &Job) -> String {
    format!(
        "If this browser needs a new recovery sign-in link after restart, open the private log on the Selfhost host: {}. Keep its contents private; the log can contain a one-use sign-in link.",
        restart_log(job).display()
    )
}
fn job_view(job: &Job) -> Value {
    json!({"job_id":job.job_id,"status":job.status,"version":job.version,"restart_log":restart_log(job),"recovery_instructions":recovery_instructions(job),"confirmation":job.confirmation,"error":job.error,"restart_status":job.restart_status,"can_activate":job.status=="staged","can_recover":matches!(job.status.as_str(),"replacing"|"failed"|"rollback_required"),"recovery_confirmation":format!("RECOVER SELFHOST {}",job.plan.installation.key.as_deref().unwrap_or("unknown").split_whitespace().nth(1).unwrap_or("unknown"))})
}

impl Store {
    fn updates_dir(&self) -> Result<PathBuf> {
        let path = self.root.join("updates");
        private_dir(&path)?;
        Ok(path)
    }
    pub fn cached_update_status(&self) -> Result<Value> {
        let current = installation()?;
        let mut cached = bounded_json(&self.updates_dir()?.join("status.json"))
            .unwrap_or(json!({"latest_version":null,"checked_at":null,"check_error":null}));
        cached["installed_version"] = json!(VERSION);
        cached["install_method"] = json!(current.method);
        let newer = cached["latest_version"]
            .as_str()
            .and_then(|v| stable_version(v).ok())
            .zip(stable_version(VERSION).ok())
            .is_some_and(|(new, old)| new > old);
        cached["update_available"] = json!(newer);
        cached["can_stage"] =
            json!(newer && current.method == "cargo" && cached["check_error"].is_null());
        cached["instructions"] = json!(instructions(
            &current.method,
            cached["latest_version"].as_str()
        ));
        Ok(cached)
    }
    pub async fn update_status(&self) -> Result<Value> {
        self.update_status_with_timeout(Duration::from_secs(15))
            .await
    }
    pub async fn update_status_with_timeout(&self, timeout: Duration) -> Result<Value> {
        let current = installation()?;
        let check: Result<Option<String>> = async {
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("selfhost/", env!("CARGO_PKG_VERSION")))
                .build()?;
            let url = if current.method == "npm" {
                "https://registry.npmjs.org/selfhost/latest"
            } else {
                "https://crates.io/api/v1/crates/selfhost"
            };
            let response = client.get(url).send().await?.error_for_status()?;
            ensure!(
                response.content_length().unwrap_or(0) < 2 * 1024 * 1024,
                "Registry response is too large"
            );
            let mut response = response;
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(
                    bytes.len() + chunk.len() <= 2 * 1024 * 1024,
                    "Registry response is too large"
                );
                bytes.extend_from_slice(&chunk);
            }
            let document: Value = serde_json::from_slice(&bytes)?;
            let latest = if current.method == "npm" {
                // The historical package is not this launcher. Do not recommend it as an upgrade.
                ensure!(
                    document["repository"]["url"]
                        .as_str()
                        .is_some_and(|r| r == "git+https://github.com/obiente/selfhost.git"
                            || r == "https://github.com/obiente/selfhost")
                        && !document["bin"]["selfhost"].is_null(),
                    "The public npm package does not yet identify the Obiente CLI launcher"
                );
                document["version"].as_str()
            } else {
                ensure!(
                    document["crate"]["repository"] == "https://github.com/obiente/selfhost",
                    "Registry package repository does not match Obiente Selfhost"
                );
                document["crate"]["max_stable_version"].as_str()
            }
            .context("Registry did not report a stable version")?;
            stable_version(latest)?;
            Ok(Some(latest.into()))
        }
        .await;
        let value = match check {
            Ok(version) => json!({"latest_version":version,"checked_at":now(),"check_error":null}),
            Err(error) => {
                json!({"latest_version":null,"checked_at":now(),"check_error":error.to_string()})
            }
        };
        save(&self.updates_dir()?.join("status.json"), &value)?;
        self.cached_update_status()
    }
    pub async fn plan_update(&self) -> Result<Value> {
        let status = self.update_status().await?;
        ensure!(
            status["can_stage"] == true,
            "This installation has no supported newer release to stage"
        );
        let installation = installation()?;
        let version = status["latest_version"]
            .as_str()
            .context("Missing update version")?
            .to_owned();
        let revision = format!("{:x}", Sha256::digest(serde_json::to_vec(&installation)?));
        let plan=Plan{id:token(16)?,revision,confirmation:format!("UPDATE SELFHOST TO {version}"),version,expires_at:now()+900,installation,scope:vec!["Build the exact approved crates.io release using Cargo in private staging.".into(),"Activation replaces only this Cargo-installed Selfhost executable and its two Cargo installation records.".into()],warnings:vec!["Cargo runs the release's build scripts and uses your trusted Rust toolchain. Review the release before approval.".into(),"The SHA-256 check detects changes to staged files; it is not a publisher signature.".into(),"Activation briefly stops this dashboard. Services and their containers keep running. Back up Selfhost's data directory before updating; executable rollback does not reverse data migrations.".into()]};
        save(
            &self.updates_dir()?.join(format!("plan-{}.json", plan.id)),
            &plan,
        )?;
        Ok(
            json!({"id":plan.id,"revision":plan.revision,"version":plan.version,"confirmation":plan.confirmation,"expires_at":plan.expires_at,"scope":plan.scope,"warnings":plan.warnings}),
        )
    }
    pub async fn stage_update(&self, approval: UpdateApproval) -> Result<Value> {
        let root = self.updates_dir()?;
        id(&approval.plan_id)?;
        let data_dir = fs::canonicalize(&self.root)?;
        tokio::task::spawn_blocking(move || stage(&root, &data_dir, approval)).await?
    }
    pub fn activate_update(
        &self,
        approval: UpdateActivation,
        restart_address: Option<std::net::SocketAddr>,
    ) -> Result<Value> {
        id(&approval.job_id)?;
        let root = self.updates_dir()?;
        let _guard = lock(&root.join("operation.lock"))?;
        let path = root.join(&approval.job_id);
        let mut job: Job = serde_json::from_value(bounded_json(&path.join("job.json"))?)?;
        ensure!(
            job.status == "staged" && approval.confirmation == job.confirmation,
            "Review a staged update and type its exact activation confirmation"
        );
        validate_current(&job.plan.installation)?;
        ensure!(
            hash(&path.join("stage/bin").join(EXE))?
                == job
                    .staged_hash
                    .as_deref()
                    .context("Missing staged checksum")?,
            "Staged executable changed"
        );
        let helper = path.join(if cfg!(windows) {
            "update-helper.exe"
        } else {
            "update-helper"
        });
        fs::copy(std::env::current_exe()?, &helper)?;
        ensure!(
            hash(&helper)? == job.plan.installation.hash,
            "Updater helper verification failed"
        );
        job.parent_pid = Some(std::process::id());
        job.restart_port = restart_address.map(|address| address.port());
        job.restart_bind = restart_address.map(|address| address.ip());
        job.restart_catalog = restart_catalog()?;
        job.status = "activating".into();
        save(&path.join("job.json"), &job)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.join("activation.log"))?;
        let mut command = Command::new(&helper);
        command
            .arg("--data-dir")
            .arg(&job.data_dir)
            .args(["update-helper", "--job", &job.job_id])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        if let Err(error) = command.spawn() {
            job.status = "staged".into();
            job.error = Some(format!("Could not start the update helper: {error}"));
            save(&path.join("job.json"), &job)?;
            bail!("Could not start the update helper; the current executable is unchanged");
        }
        Ok(
            json!({"job_id":job.job_id,"status":"activating","shutdown_required":true,"message":"The update helper will wait for this process to exit before replacing the executable.","restart_log":restart_log(&job),"recovery_instructions":recovery_instructions(&job)}),
        )
    }
    pub fn update_jobs(&self) -> Result<Vec<Value>> {
        let mut jobs = Vec::new();
        for entry in fs::read_dir(self.updates_dir()?)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && id(&entry.file_name().to_string_lossy()).is_ok()
                && let Ok(value) = bounded_json(&entry.path().join("job.json"))
                && let Ok(job) = serde_json::from_value::<Job>(value)
            {
                let mut view = job_view(&job);
                view["can_recover"] = json!(
                    view["can_recover"] == true
                        && entry.path().join("cargo-v1.before").is_file()
                        && entry.path().join("cargo-v2.before").is_file()
                );
                jobs.push(view);
            }
        }
        Ok(jobs)
    }
    pub fn recover_update(
        &self,
        approval: UpdateActivation,
        restart_address: Option<std::net::SocketAddr>,
    ) -> Result<Value> {
        id(&approval.job_id)?;
        let root = self.updates_dir()?;
        let _guard = lock(&root.join("operation.lock"))?;
        let path = root.join(&approval.job_id);
        let mut job: Job = serde_json::from_value(bounded_json(&path.join("job.json"))?)?;
        ensure!(
            matches!(
                job.status.as_str(),
                "replacing" | "failed" | "rollback_required"
            ) && approval.confirmation
                == job_view(&job)["recovery_confirmation"]
                    .as_str()
                    .unwrap_or(""),
            "Review this job and type its exact recovery confirmation"
        );
        ensure!(
            path.join("cargo-v1.before").is_file() && path.join("cargo-v2.before").is_file(),
            "This job did not start replacing the installation; no recovery is needed"
        );
        let helper = path.join(if cfg!(windows) {
            "update-helper.exe"
        } else {
            "update-helper"
        });
        ensure!(
            hash(&helper)? == job.plan.installation.hash,
            "Original update helper is unavailable or changed; preserve the backups and recover manually"
        );
        job.parent_pid = Some(std::process::id());
        job.restart_port = restart_address.map(|address| address.port());
        job.restart_bind = restart_address.map(|address| address.ip());
        job.restart_catalog = restart_catalog()?;
        job.status = "recovering".into();
        save(&path.join("job.json"), &job)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.join("activation.log"))?;
        let mut command = Command::new(helper);
        command
            .arg("--data-dir")
            .arg(&job.data_dir)
            .args(["update-helper", "--job", &job.job_id])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        if let Err(error) = command.spawn() {
            job.status = "rollback_required".into();
            job.error = Some(error.to_string());
            save(&path.join("job.json"), &job)?;
            return Err(error.into());
        }
        Ok(
            json!({"job_id":job.job_id,"status":"recovering","shutdown_required":true,"restart_log":restart_log(&job),"recovery_instructions":recovery_instructions(&job)}),
        )
    }
}
fn validate_current(expected: &Installation) -> Result<()> {
    let current = installation()?;
    ensure!(
        current.method == "cargo"
            && current.executable == expected.executable
            && current.hash == expected.hash
            && current.key == expected.key
            && current.metadata == expected.metadata,
        "The running installation changed; review a new update plan"
    );
    Ok(())
}
fn restart_catalog() -> Result<Option<PathBuf>> {
    let mut args = std::env::args_os();
    while let Some(arg) = args.next() {
        if arg == "--catalog-dir" {
            return Ok(Some(fs::canonicalize(
                args.next().context("Missing catalog directory")?,
            )?));
        }
        if let Some(value) = arg.to_str().and_then(|s| s.strip_prefix("--catalog-dir=")) {
            return Ok(Some(fs::canonicalize(value)?));
        }
    }
    Ok(None)
}
fn cargo_executable() -> Result<PathBuf> {
    cargo_executable_on_path(&std::env::var_os("PATH").unwrap_or_default())
}
fn cargo_executable_on_path(search_path: &std::ffi::OsStr) -> Result<PathBuf> {
    let name = if cfg!(windows) { "cargo.exe" } else { "cargo" };
    for directory in std::env::split_paths(search_path) {
        if !directory.is_absolute() {
            continue;
        }
        let path = directory.join(name);
        if path.is_file() {
            // Rustup dispatches by argv[0]. Resolving its cargo symlink would
            // execute `rustup install` instead of `cargo install`. Opening the
            // original absolute path still verifies the native target bytes.
            native(&path)?;
            return Ok(path);
        }
    }
    bail!("A trusted native Cargo executable must be installed on your PATH")
}
fn native(path: &Path) -> Result<()> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 64];
    f.read_exact(&mut head)?;
    #[cfg(windows)]
    {
        ensure!(&head[..2] == b"MZ", "Expected a native Windows executable");
        let offset = u32::from_le_bytes(head[60..64].try_into()?);
        ensure!(
            offset as u64 + 6 <= f.metadata()?.len(),
            "Invalid PE header"
        );
        f.seek(SeekFrom::Start(offset as u64))?;
        let mut pe = [0u8; 4];
        f.read_exact(&mut pe)?;
        ensure!(&pe == b"PE\0\0", "Invalid native executable signature");
    }
    #[cfg(target_os = "linux")]
    ensure!(&head[..4] == b"\x7fELF", "Expected a native ELF executable");
    #[cfg(target_os = "macos")]
    ensure!(
        matches!(
            u32::from_be_bytes(head[..4].try_into()?),
            0xfeedface | 0xfeedfacf | 0xcefaedfe | 0xcffaedfe | 0xcafebabe | 0xbebafeca
        ),
        "Expected a native Mach-O executable"
    );
    Ok(())
}
fn stage(root: &Path, data_dir: &Path, approval: UpdateApproval) -> Result<Value> {
    let _guard = lock(&root.join("operation.lock"))?;
    let plan_path = root.join(format!("plan-{}.json", approval.plan_id));
    let plan: Plan = serde_json::from_value(bounded_json(&plan_path)?)?;
    ensure!(
        plan.id == approval.plan_id
            && plan.revision == approval.revision
            && plan.confirmation == approval.confirmation
            && plan.expires_at >= now(),
        "Update approval is incorrect or expired"
    );
    validate_current(&plan.installation)?;
    stable_version(&plan.version)?;
    let path = root.join(&plan.id);
    ensure!(
        !path.exists(),
        "This plan has already been staged; review its update job"
    );
    private_dir(&path)?;
    let mut job = Job {
        job_id: plan.id.clone(),
        version: plan.version.clone(),
        confirmation: format!("ACTIVATE SELFHOST {}", plan.version),
        plan,
        status: "building".into(),
        staged_hash: None,
        parent_pid: None,
        restart_port: None,
        restart_bind: None,
        restart_catalog: None,
        restart_pid: None,
        restart_status: None,
        data_dir: data_dir.into(),
        error: None,
    };
    save(&path.join("job.json"), &job)?;
    let result: Result<()> = (|| {
        let stage = path.join("stage");
        private_dir(&stage)?;
        let log = File::create(path.join("build.log"))?;
        let mut cmd = Command::new(cargo_executable()?);
        cmd.current_dir(&path)
            .args([
                "install",
                "selfhost",
                "--version",
                &job.version,
                "--locked",
                "--index",
                "https://github.com/rust-lang/crates.io-index",
                "--root",
            ])
            .arg(&stage)
            .arg("--target-dir")
            .arg(path.join("target"))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        if let Some(metadata) = &job.plan.installation.metadata {
            if let Some(target) = metadata["target"].as_str() {
                ensure!(
                    target
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
                    "Custom target-file installations require manual updates"
                );
                cmd.arg("--target").arg(target);
            }
            if metadata["all_features"] == true {
                cmd.arg("--all-features");
            }
            if metadata["no_default_features"] == true {
                cmd.arg("--no-default-features");
            }
            if let Some(features) = metadata["features"].as_array() {
                let features: Vec<_> = features.iter().filter_map(Value::as_str).collect();
                if !features.is_empty() {
                    cmd.arg("--features").arg(features.join(","));
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let status = cmd
            .status()
            .context("Could not launch Cargo for the approved update")?;
        ensure!(
            status.success(),
            "Cargo could not build the approved release ({status}). The current executable is unchanged. Build log: {}",
            path.join("build.log").display()
        );
        let executable = stage.join("bin").join(EXE);
        native(&executable)?;
        let output = Command::new(&executable).arg("--version").output()?;
        ensure!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim()
                    == format!("selfhost {}", job.version),
            "Staged executable reports an unexpected version"
        );
        let metadata = bounded_json(&stage.join(".crates2.json"))?;
        ensure!(
            !metadata["installs"][format!("selfhost {} ({SOURCE})", job.version)].is_null(),
            "Staged release is not recorded as coming from crates.io"
        );
        job.staged_hash = Some(hash(&executable)?);
        job.status = "staged".into();
        Ok(())
    })();
    if let Err(error) = result {
        job.status = "failed".into();
        job.error = Some(error.to_string());
        save(&path.join("job.json"), &job)?;
        return Err(error);
    }
    save(&path.join("job.json"), &job)?;
    Ok(job_view(&job))
}

// Open the parent process while it is still alive. Holding this handle prevents PID reuse on Windows.
#[cfg(windows)]
fn wait_parent(pid: u32) -> Result<()> {
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn GetLastError() -> u32;
    }
    unsafe {
        let handle = OpenProcess(0x00100000, 0, pid);
        if handle.is_null() {
            ensure!(
                GetLastError() == 87,
                "Cannot verify the previous Selfhost process has exited"
            );
            return Ok(());
        }
        let status = WaitForSingleObject(handle, 1_800_000);
        CloseHandle(handle);
        ensure!(
            status == 0,
            "The previous Selfhost process did not exit within thirty minutes"
        );
    }
    Ok(())
}
#[cfg(unix)]
fn wait_parent(pid: u32) -> Result<()> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    for _ in 0..18000 {
        if unsafe { kill(pid as i32, 0) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(3) {
                return Ok(());
            }
            bail!("Cannot verify the previous process has exited");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("The previous process did not exit within thirty minutes")
}
fn write_locked(file: &mut File, bytes: &[u8]) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    file.write_all(bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.sync_all()?;
    Ok(())
}
fn replace_with_backup(target: &Path, replacement: &Path, backup: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        unsafe extern "system" {
            fn ReplaceFileW(
                target: *const u16,
                replacement: *const u16,
                backup: *const u16,
                flags: u32,
                exclude: *mut std::ffi::c_void,
                reserved: *mut std::ffi::c_void,
            ) -> i32;
        }
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let target = wide(target);
        let replacement = wide(replacement);
        let backup = wide(backup);
        let result = unsafe {
            ReplaceFileW(
                target.as_ptr(),
                replacement.as_ptr(),
                backup.as_ptr(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        ensure!(
            result != 0,
            "Windows could not replace the executable: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    {
        fs::hard_link(target, backup)?;
        if let Err(error) = fs::rename(replacement, target) {
            let _ = fs::remove_file(backup);
            return Err(error.into());
        }
    }
    Ok(())
}
fn read_locked(file: &mut File) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "Cargo metadata too large");
    Ok(bytes)
}
fn recover_installation(path: &Path, job: &mut Job) -> Result<()> {
    let installation = &job.plan.installation;
    let root = installation
        .cargo_root
        .as_ref()
        .context("Missing original Cargo root")?;
    ensure!(
        installation.method == "cargo" && installation.executable == root.join("bin").join(EXE),
        "Recovery target does not match its original Cargo installation"
    );
    let _guard = lock(&root.join(".selfhost-update.lock"))?;
    let mut v1 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join(".crates.toml"))?;
    v1.try_lock_exclusive()?;
    let mut v2 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join(".crates2.json"))?;
    v2.try_lock_exclusive()?;
    let before1 = fs::read(path.join("cargo-v1.before"))?;
    let before2 = fs::read(path.join("cargo-v2.before"))?;
    for (current, before, after) in [
        (read_locked(&mut v1)?, &before1, path.join("cargo-v1.after")),
        (read_locked(&mut v2)?, &before2, path.join("cargo-v2.after")),
    ] {
        ensure!(
            current == *before || current == fs::read(after)?,
            "Cargo metadata changed independently. Automatic recovery would overwrite another installation; preserve the backups and recover manually"
        );
    }
    let target = &installation.executable;
    let backup = target.with_file_name(format!(".selfhost-{}.backup", job.job_id));
    let already_original = target.is_file() && hash(target)? == installation.hash;
    if !already_original {
        ensure!(
            hash(&backup)? == installation.hash,
            "Original executable backup changed or is missing"
        );
        if target.exists() {
            ensure!(
                hash(target)?
                    == job
                        .staged_hash
                        .as_deref()
                        .context("Missing staged checksum")?,
                "Executable changed independently; recovery stopped"
            );
            let retained = target.with_file_name(format!(".selfhost-{}.rejected", job.job_id));
            ensure!(
                !retained.exists(),
                "A previous recovery file already exists"
            );
            fs::rename(target, retained)?;
        }
        fs::rename(backup, target)?;
    }
    write_locked(&mut v1, &before1)?;
    write_locked(&mut v2, &before2)?;
    ensure!(
        hash(target)? == installation.hash,
        "Recovered executable verification failed"
    );
    job.status = "recovered".into();
    job.error = None;
    save(&path.join("job.json"), job)?;
    Ok(())
}
fn replace_installation(path: &Path, job: &mut Job) -> Result<()> {
    let installation = &job.plan.installation;
    let cargo_root = installation
        .cargo_root
        .as_ref()
        .context("Missing Cargo installation root")?;
    ensure!(
        installation.method == "cargo"
            && fs::canonicalize(cargo_root.join("bin").join(EXE))? == installation.executable,
        "Cargo target changed"
    );
    let _install_guard = lock(&cargo_root.join(".selfhost-update.lock"))?;
    // Cargo holds these exact two files, in this order, while changing its installation records.
    let mut v1 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(cargo_root.join(".crates.toml"))?;
    v1.try_lock_exclusive()
        .context("Cargo installation metadata is in use")?;
    let mut v2 = OpenOptions::new()
        .read(true)
        .write(true)
        .open(cargo_root.join(".crates2.json"))?;
    v2.try_lock_exclusive()
        .context("Cargo installation metadata is in use")?;
    let old1 = read_locked(&mut v1)?;
    let old2 = read_locked(&mut v2)?;
    let mut document1: toml::Value = toml::from_str(std::str::from_utf8(&old1)?)?;
    let mut document2: Value = serde_json::from_slice(&old2)?;
    let old_key = installation
        .key
        .as_ref()
        .context("Missing previous Cargo record")?;
    ensure!(
        document2["installs"][old_key]
            == *installation
                .metadata
                .as_ref()
                .context("Missing previous Cargo metadata")?
            && document1
                .get("v1")
                .and_then(|v| v.get(old_key))
                .and_then(toml::Value::as_array)
                .is_some_and(|bins| bins.len() == 1 && bins[0].as_str() == Some(EXE)),
        "Cargo installation records changed after review"
    );
    let new_key = format!("selfhost {} ({SOURCE})", job.version);
    let staged2 = bounded_json(&path.join("stage/.crates2.json"))?;
    let staged1: toml::Value =
        toml::from_str(&fs::read_to_string(path.join("stage/.crates.toml"))?)?;
    let new2 = staged2["installs"]
        .get(&new_key)
        .context("Missing staged Cargo record")?
        .clone();
    let new1 = staged1
        .get("v1")
        .and_then(|v| v.get(&new_key))
        .context("Missing staged Cargo v1 record")?
        .clone();
    ensure!(
        new2["bins"] == json!([EXE])
            && new1
                .as_array()
                .is_some_and(|bins| bins.len() == 1 && bins[0].as_str() == Some(EXE)),
        "Approved package must install only the Selfhost executable"
    );
    let installs = document2["installs"]
        .as_object_mut()
        .context("Invalid Cargo metadata")?;
    ensure!(
        !installs.contains_key(&new_key),
        "Approved version is already installed; inspect Cargo before retrying"
    );
    installs.remove(old_key);
    installs.insert(new_key.clone(), new2);
    let entries = document1
        .get_mut("v1")
        .and_then(toml::Value::as_table_mut)
        .context("Invalid Cargo v1 metadata")?;
    entries.remove(old_key);
    entries.insert(new_key, new1);
    let new1 = toml::to_string(&document1)?.into_bytes();
    let new2 = serde_json::to_vec(&document2)?;
    ensure!(
        hash(&installation.executable)? == installation.hash,
        "Installed executable changed after review"
    );
    let staged = path.join("stage/bin").join(EXE);
    ensure!(
        hash(&staged)?
            == job
                .staged_hash
                .as_deref()
                .context("Missing staged checksum")?,
        "Staged executable changed after review"
    );
    native(&staged)?;
    let backup = installation
        .executable
        .with_file_name(format!(".selfhost-{}.backup", job.job_id));
    let replacement = installation
        .executable
        .with_file_name(format!(".selfhost-{}.replacement", job.job_id));
    ensure!(
        !backup.exists() && !replacement.exists(),
        "Update recovery files already exist; inspect the previous attempt"
    );
    atomic_write(&path.join("cargo-v1.before"), &old1)?;
    atomic_write(&path.join("cargo-v2.before"), &old2)?;
    atomic_write(&path.join("cargo-v1.after"), &new1)?;
    atomic_write(&path.join("cargo-v2.after"), &new2)?;
    fs::copy(&staged, &replacement)?;
    ensure!(
        hash(&replacement)? == job.staged_hash.as_deref().unwrap(),
        "Copied replacement verification failed"
    );
    job.status = "replacing".into();
    save(&path.join("job.json"), job)?;
    replace_with_backup(&installation.executable, &replacement, &backup)?;
    let mutation: Result<()> = (|| {
        write_locked(&mut v1, &new1)?;
        write_locked(&mut v2, &new2)?;
        ensure!(
            hash(&installation.executable)? == job.staged_hash.as_deref().unwrap(),
            "Installed replacement verification failed"
        );
        Ok(())
    })();
    if let Err(error) = mutation {
        let rollback: Result<()> = (|| {
            if installation.executable.exists() {
                ensure!(
                    hash(&installation.executable)? == job.staged_hash.as_deref().unwrap(),
                    "Cannot roll back an independently modified executable"
                );
                fs::remove_file(&installation.executable)?;
            }
            fs::rename(&backup, &installation.executable)?;
            write_locked(&mut v1, &old1)?;
            write_locked(&mut v2, &old2)?;
            Ok(())
        })();
        if let Err(rollback_error) = rollback {
            job.status = "rollback_required".into();
            bail!(
                "Update failed: {error}. Recovery also failed: {rollback_error}. Keep the sibling executable backup and private Cargo metadata backups."
            );
        }
        bail!("Update failed and the executable and Cargo metadata were restored: {error}");
    }
    job.status = "completed".into();
    save(&path.join("job.json"), job)?;
    Ok(())
}
pub fn run_helper(data_dir: PathBuf, job_id: String) -> Result<()> {
    id(&job_id)?;
    let root = fs::canonicalize(data_dir)?.join("updates");
    let path = root.join(&job_id);
    let mut job: Job = serde_json::from_value(bounded_json(&path.join("job.json"))?)?;
    ensure!(
        job.job_id == job_id && matches!(job.status.as_str(), "activating" | "recovering"),
        "Update job is not awaiting activation or recovery"
    );
    ensure!(
        fs::canonicalize(&job.data_dir)?.join("updates") == root,
        "Update data directory changed"
    );
    let helper = fs::canonicalize(std::env::current_exe()?)?;
    ensure!(
        helper.parent() == Some(path.as_path()) && hash(&helper)? == job.plan.installation.hash,
        "Only the copied, verified update helper may activate this job"
    );
    let mut parent_exited = false;
    let result = (|| {
        wait_parent(job.parent_pid.context("Missing parent process")?)?;
        parent_exited = true;
        let _guard = lock(&root.join("operation.lock"))?;
        let fresh: Job = serde_json::from_value(bounded_json(&path.join("job.json"))?)?;
        ensure!(
            fresh.status == job.status && fresh.parent_pid == job.parent_pid,
            "Update job changed while waiting"
        );
        if job.status == "recovering" {
            recover_installation(&path, &mut job)
        } else {
            replace_installation(&path, &mut job)
        }
    })();
    if let Err(error) = result {
        if job.status != "rollback_required" {
            job.status = "failed".into();
        }
        job.error = Some(error.to_string());
        save(&path.join("job.json"), &job)?;
        if parent_exited
            && hash(&job.plan.installation.executable).ok().as_deref()
                == Some(&job.plan.installation.hash)
        {
            restart_dashboard(&path, &mut job)?;
        }
        return Err(error);
    }
    restart_dashboard(&path, &mut job)
}
fn restart_dashboard(path: &Path, job: &mut Job) -> Result<()> {
    if let Some(port) = job.restart_port {
        let restart_output = OpenOptions::new()
            .create(true)
            .append(true)
            .open(restart_log(job))?;
        let mut command = Command::new(&job.plan.installation.executable);
        if let Some(catalog) = &job.restart_catalog {
            command.arg("--catalog-dir").arg(catalog);
        }
        command
            .arg("--data-dir")
            .arg(&job.data_dir)
            .args(["serve", "--port", &port.to_string()]);
        if let Some(bind) = job.restart_bind {
            command.args(["--bind", &bind.to_string()]);
        }
        command
            .stdin(Stdio::null())
            .stdout(restart_output.try_clone()?)
            .stderr(restart_output);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        match command.spawn() {
            Ok(mut child) => {
                job.restart_pid = Some(child.id());
                std::thread::sleep(Duration::from_secs(1));
                match child.try_wait()? {
                    Some(status) => {
                        job.restart_status = Some("exited".into());
                        job.error = Some(format!(
                            "Dashboard exited after restart ({status}). Inspect the private restart.log; executable recovery does not undo data migrations."
                        ));
                    }
                    None => job.restart_status = Some("started_unverified".into()),
                }
            }
            Err(error) => {
                job.restart_status = Some("failed".into());
                job.error = Some(format!(
                    "Dashboard restart failed: {error}. Start Selfhost normally using the same data directory and port."
                ));
            }
        }
        save(&path.join("job.json"), job)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn cargo_symlink_keeps_rustup_dispatch_name() {
        let directory = tempfile::tempdir().unwrap();
        let launcher = directory.path().join("cargo");
        let target = fs::canonicalize(cargo_executable().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, &launcher).unwrap();
        let found = cargo_executable_on_path(directory.path().as_os_str()).unwrap();
        assert_eq!(
            found, launcher,
            "Do not resolve Cargo's multicall launcher name"
        );
        let output = Command::new(found).arg("--version").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("cargo "));
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, Job) {
        fixture_binary(&std::env::current_exe().unwrap())
    }
    fn fixture_binary(binary: &Path) -> (tempfile::TempDir, PathBuf, Job) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let cargo = root.join("cargo");
        let data = root.join("state");
        let job_id = "1234567890abcdef1234567890abcdef".to_owned();
        let path = data.join("updates").join(&job_id);
        fs::create_dir_all(cargo.join("bin")).unwrap();
        fs::create_dir_all(path.join("stage/bin")).unwrap();
        let exe = cargo.join("bin").join(EXE);
        fs::copy(binary, &exe).unwrap();
        let staged = path.join("stage/bin").join(EXE);
        fs::copy(&exe, &staged).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&staged)
            .unwrap()
            .write_all(b"synthetic replacement marker")
            .unwrap();
        let key = format!("selfhost {VERSION} ({SOURCE})");
        let new_key = format!("selfhost 99.0.0 ({SOURCE})");
        let metadata = json!({"bins":[EXE],"profile":"release","features":[],"all_features":false,"no_default_features":false});
        let v2 = json!({"installs":{key.clone():metadata.clone(),"unrelated 1.0.0 (synthetic)":{"bins":["unrelated"]}},"preserve":"extra"});
        save(&cargo.join(".crates2.json"), &v2).unwrap();
        let mut entries = toml::map::Map::new();
        entries.insert(
            key.clone(),
            toml::Value::Array(vec![toml::Value::String(EXE.into())]),
        );
        entries.insert(
            "unrelated 1.0.0 (synthetic)".into(),
            toml::Value::Array(vec![toml::Value::String("unrelated".into())]),
        );
        let mut doc = toml::map::Map::new();
        doc.insert("v1".into(), toml::Value::Table(entries));
        fs::write(
            cargo.join(".crates.toml"),
            toml::to_string(&toml::Value::Table(doc)).unwrap(),
        )
        .unwrap();
        save(
            &path.join("stage/.crates2.json"),
            &json!({"installs":{new_key.clone():metadata.clone()}}),
        )
        .unwrap();
        let mut entries = toml::map::Map::new();
        entries.insert(
            new_key,
            toml::Value::Array(vec![toml::Value::String(EXE.into())]),
        );
        let mut doc = toml::map::Map::new();
        doc.insert("v1".into(), toml::Value::Table(entries));
        fs::write(
            path.join("stage/.crates.toml"),
            toml::to_string(&toml::Value::Table(doc)).unwrap(),
        )
        .unwrap();
        let installation = Installation {
            method: "cargo".into(),
            hash: hash(&exe).unwrap(),
            executable: exe,
            cargo_root: Some(cargo),
            key: Some(key),
            metadata: Some(metadata),
        };
        let plan = Plan {
            id: job_id.clone(),
            revision: "synthetic".into(),
            version: "99.0.0".into(),
            confirmation: "synthetic".into(),
            expires_at: now() + 600,
            installation,
            scope: vec![],
            warnings: vec![],
        };
        let job = Job {
            job_id,
            status: "activating".into(),
            version: "99.0.0".into(),
            confirmation: "ACTIVATE SELFHOST 99.0.0".into(),
            plan,
            staged_hash: Some(hash(&staged).unwrap()),
            parent_pid: None,
            restart_port: None,
            restart_bind: None,
            restart_catalog: None,
            restart_pid: None,
            restart_status: None,
            data_dir: data,
            error: None,
        };
        (dir, path, job)
    }
    #[test]
    fn native_replacement_and_recovery_preserve_unrelated_cargo_records() {
        let (_dir, path, mut job) = fixture();
        let cargo = job.plan.installation.cargo_root.clone().unwrap();
        let old1 = fs::read(cargo.join(".crates.toml")).unwrap();
        let old2 = fs::read(cargo.join(".crates2.json")).unwrap();
        replace_installation(&path, &mut job).unwrap();
        assert_eq!(job.status, "completed");
        assert_eq!(
            hash(&job.plan.installation.executable).unwrap(),
            job.staged_hash.as_deref().unwrap()
        );
        let metadata = bounded_json(&cargo.join(".crates2.json")).unwrap();
        assert_eq!(metadata["preserve"], "extra");
        assert_eq!(
            metadata["installs"]["unrelated 1.0.0 (synthetic)"]["bins"],
            json!(["unrelated"])
        );
        job.status = "rollback_required".into();
        recover_installation(&path, &mut job).unwrap();
        assert_eq!(job.status, "recovered");
        assert_eq!(
            hash(&job.plan.installation.executable).unwrap(),
            job.plan.installation.hash
        );
        assert_eq!(fs::read(cargo.join(".crates.toml")).unwrap(), old1);
        assert_eq!(fs::read(cargo.join(".crates2.json")).unwrap(), old2);
    }
    #[test]
    fn changed_staging_and_independent_installation_fail_closed() {
        let (_dir, path, mut job) = fixture();
        let staged = path.join("stage/bin").join(EXE);
        OpenOptions::new()
            .append(true)
            .open(staged)
            .unwrap()
            .write_all(b"unexpected edit")
            .unwrap();
        assert!(replace_installation(&path, &mut job).is_err());
        assert_eq!(
            hash(&job.plan.installation.executable).unwrap(),
            job.plan.installation.hash
        );
        let (_dir, path, mut job) = fixture();
        replace_installation(&path, &mut job).unwrap();
        let cargo = job.plan.installation.cargo_root.clone().unwrap();
        let mut metadata = bounded_json(&cargo.join(".crates2.json")).unwrap();
        metadata["independent_install"] = json!(true);
        save(&cargo.join(".crates2.json"), &metadata).unwrap();
        assert!(recover_installation(&path, &mut job).is_err());
        assert_eq!(
            hash(&job.plan.installation.executable).unwrap(),
            job.staged_hash.as_deref().unwrap()
        );
        assert_eq!(
            bounded_json(&cargo.join(".crates2.json")).unwrap()["independent_install"],
            true
        );
    }
    #[test]
    #[ignore = "requires SELFHOST_UPDATE_TEST_BINARY pointing at a freshly built native Selfhost; creates only temporary installations and localhost dashboards"]
    fn native_helper_activation_and_failure_restart() {
        let binary = PathBuf::from(
            std::env::var_os("SELFHOST_UPDATE_TEST_BINARY")
                .expect("Set SELFHOST_UPDATE_TEST_BINARY"),
        );
        for metadata_busy in [false, true] {
            let (_dir, path, mut job) = fixture_binary(&binary);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            job.restart_port = Some(port);
            job.restart_bind = Some("127.0.0.2".parse().unwrap());
            let helper = path.join(if cfg!(windows) {
                "update-helper.exe"
            } else {
                "update-helper"
            });
            fs::copy(&binary, &helper).unwrap();
            #[cfg(windows)]
            let mut parent_command = {
                let mut command = Command::new("powershell.exe");
                command.args(["-NoProfile", "-Command", "Start-Sleep -Milliseconds 1500"]);
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
                command
            };
            #[cfg(unix)]
            let mut parent_command = {
                let mut command = Command::new("sleep");
                command.arg("2");
                command
            };
            let mut parent = parent_command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            job.parent_pid = Some(parent.id());
            save(&path.join("job.json"), &job).unwrap();
            let metadata_guard = if metadata_busy {
                let f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(
                        job.plan
                            .installation
                            .cargo_root
                            .as_ref()
                            .unwrap()
                            .join(".crates.toml"),
                    )
                    .unwrap();
                f.lock_exclusive().unwrap();
                Some(f)
            } else {
                None
            };
            let mut process = Command::new(&helper)
                .arg("--data-dir")
                .arg(&job.data_dir)
                .args(["update-helper", "--job", &job.job_id])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_millis(300));
            assert!(
                process.try_wait().unwrap().is_none(),
                "Helper must wait for its parent"
            );
            assert_eq!(
                hash(&job.plan.installation.executable).unwrap(),
                job.plan.installation.hash
            );
            parent.wait().unwrap();
            let result = process.wait().unwrap();
            assert_eq!(result.success(), !metadata_busy);
            drop(metadata_guard);
            let job: Job = serde_json::from_value(bounded_json(&path.join("job.json")).unwrap())
                .unwrap_or_else(|_| panic!("Missing job state"));
            let pid=job.restart_pid.expect("Both successful activation and verified-original failure must restart the dashboard");
            struct Stop(u32);
            impl Drop for Stop {
                fn drop(&mut self) {
                    #[cfg(windows)]
                    {
                        unsafe extern "system" {
                            fn OpenProcess(
                                access: u32,
                                inherit: i32,
                                pid: u32,
                            ) -> *mut std::ffi::c_void;
                            fn TerminateProcess(handle: *mut std::ffi::c_void, code: u32) -> i32;
                            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
                        }
                        unsafe {
                            let h = OpenProcess(1, 0, self.0);
                            if !h.is_null() {
                                TerminateProcess(h, 0);
                                CloseHandle(h);
                            }
                        }
                    }
                    #[cfg(unix)]
                    {
                        unsafe extern "C" {
                            fn kill(pid: i32, signal: i32) -> i32;
                        }
                        unsafe {
                            kill(self.0 as i32, 15);
                        }
                    }
                }
            }
            let stop = Stop(pid);
            let mut link_saved = false;
            for _ in 0..200 {
                if fs::read_to_string(restart_log(&job))
                    .unwrap_or_default()
                    .contains("sign-in link")
                {
                    link_saved = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            assert!(
                link_saved,
                "Restart recovery link must remain available in the private owner log"
            );
            assert!(
                fs::read_to_string(restart_log(&job))
                    .unwrap()
                    .contains(&format!("Selfhost listening on 127.0.0.2:{port}")),
                "Both replacement and recovery must preserve the explicit bind address"
            );
            assert_eq!(
                job.status,
                if metadata_busy { "failed" } else { "completed" }
            );
            assert_eq!(job.restart_status.as_deref(), Some("started_unverified"));
            assert_eq!(
                hash(&job.plan.installation.executable).unwrap(),
                if metadata_busy {
                    job.plan.installation.hash.clone()
                } else {
                    job.staged_hash.clone().unwrap()
                }
            );
            drop(stop);
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    #[test]
    fn exact_versions_and_identifiers() {
        assert!(stable_version("0.12.2").unwrap() > stable_version("0.2.19").unwrap());
        for bad in ["latest", "v1.2.3", "1.2.3;evil", "1.2.3-alpha", "01.2.3"] {
            assert!(stable_version(bad).is_err());
        }
        assert!(id("../../config").is_err());
    }
    #[test]
    fn development_binary_is_protected() {
        let found = installation_at(std::env::current_exe().unwrap(), true).unwrap();
        assert_eq!(found.method, "source");
        assert!(found.cargo_root.is_none());
    }
    #[test]
    fn native_script_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(EXE);
        fs::write(
            &path,
            b"#!/bin/sh\necho selfhost;xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        )
        .unwrap();
        assert!(native(&path).is_err());
    }
}
