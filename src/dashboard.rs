//! Per-user dashboard supervision. Never manages application containers.
use crate::core::{Store, atomic_write, private_dir};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Subcommand)]
pub enum DashboardCommand {
    /// Install a private copy as a user service (starts at sign-in); does not start it now
    Install {
        #[arg(long, default_value_t = 8372)]
        port: u16,
        /// Show service definitions without installing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Start the installed dashboard in the background
    Start,
    /// Stop only this workspace's background dashboard
    Stop,
    /// Restart the user service and issue a new local sign-in link
    Restart,
    /// Inspect the user service and its private log location
    Status,
    /// Remove the user service, preserving projects, configuration and logs
    Uninstall,
    /// Read private dashboard logs, including the local sign-in link
    Logs {
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=1000))]
        lines: u16,
    },
    /// Review a domain and proxy instructions; apply an existing login configuration with its revision
    Domain {
        url: String,
        #[arg(long, default_value_t = 8372)]
        port: u16,
        #[arg(long)]
        revision: Option<String>,
        #[arg(long)]
        confirm_callbacks: bool,
    },
    #[command(hide = true)]
    Run {
        #[arg(long)]
        port: u16,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    port: u16,
    catalog: Option<PathBuf>,
    state: String,
}

fn directory(store: &Store) -> PathBuf {
    store.root.join("dashboard-service")
}
fn settings(store: &Store) -> Result<Option<Settings>> {
    let file = directory(store).join("service.json");
    if !file.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(file)?)?))
}
fn id(store: &Store) -> Result<String> {
    Ok(format!(
        "selfhost-{:x}",
        Sha256::digest(fs::canonicalize(&store.root)?.to_string_lossy().as_bytes())
    )[..25]
        .into())
}
fn text(path: &Path) -> Result<String> {
    let value = path.to_str().context("Service paths must be valid UTF-8")?;
    ensure!(
        !value.chars().any(char::is_control),
        "Service paths cannot contain control characters"
    );
    Ok(value.into())
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn systemd(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$")
    )
}
fn ps(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn windows_argument(value: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        result.push_str(&"\\".repeat(if c == '"' { slashes * 2 + 1 } else { slashes }));
        result.push(c);
        slashes = 0;
    }
    result.push_str(&"\\".repeat(slashes * 2));
    result.push('"');
    result
}
#[cfg(windows)]
struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl WindowsJob {
    fn new() -> Result<Self> {
        use windows_sys::Win32::System::JobObjects::*;
        // This unnamed, non-inheritable job owns only the service's backend child.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        ensure!(
            !job.is_null(),
            "Cannot create service process group: {}",
            std::io::Error::last_os_error()
        );
        let guard = Self(job);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )
        };
        ensure!(
            ok != 0,
            "Cannot configure service process group: {}",
            std::io::Error::last_os_error()
        );
        Ok(guard)
    }
    fn assign(&self, child: &std::process::Child) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        let ok = unsafe {
            windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
                self.0,
                child.as_raw_handle(),
            )
        };
        ensure!(
            ok != 0,
            "Cannot attach dashboard to its process group: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
fn executable(store: &Store) -> PathBuf {
    directory(store).join(if cfg!(windows) {
        "selfhost.exe"
    } else {
        "selfhost"
    })
}
fn arguments(store: &Store, config: &Settings) -> Result<Vec<String>> {
    let mut args = vec!["--data-dir".into(), text(&fs::canonicalize(&store.root)?)?];
    if let Some(catalog) = &config.catalog {
        args.extend(["--catalog-dir".into(), text(catalog)?]);
    }
    args.extend([
        "dashboard".into(),
        "run".into(),
        "--port".into(),
        config.port.to_string(),
    ]);
    Ok(args)
}
fn definition(
    platform: &str,
    name: &str,
    binary: &str,
    args: &[String],
    user: &str,
) -> Result<String> {
    match platform {
        "linux" => Ok(format!(
            "[Unit]\nDescription=Selfhost dashboard\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={} {}\nRestart=on-failure\nRestartSec=5\nUMask=0077\nTimeoutStopSec=300\n\n[Install]\nWantedBy=default.target\n",
            systemd(binary),
            args.iter()
                .map(|a| systemd(a))
                .collect::<Vec<_>>()
                .join(" ")
        )),
        "macos" => Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>org.obiente.{name}</string><key>ProgramArguments</key><array>{}</array><key>RunAtLoad</key><true/><key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict><key>ThrottleInterval</key><integer>5</integer><key>Umask</key><integer>63</integer></dict></plist>\n",
            std::iter::once(binary)
                .chain(args.iter().map(String::as_str))
                .map(|a| format!("<string>{}</string>", xml(a)))
                .collect::<String>()
        )),
        "windows" => {
            let action = args
                .iter()
                .map(|a| windows_argument(a))
                .collect::<Vec<_>>()
                .join(" ");
            Ok(format!(
                "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\"><Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger></Triggers><Principals><Principal id=\"Selfhost\"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings><Actions Context=\"Selfhost\"><Exec><Command>{}</Command><Arguments>{}</Arguments></Exec></Actions></Task>\n",
                xml(user),
                xml(user),
                xml(binary),
                xml(&action)
            ))
        }
        _ => anyhow::bail!("Persistent dashboard services support Linux, macOS and Windows"),
    }
}
fn output(program: &str, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .output()
        .with_context(|| format!("Unable to run {program}"))
}
fn checked(program: &str, args: &[&str]) -> Result<String> {
    let result = output(program, args)?;
    ensure!(
        result.status.success(),
        "{program} failed: {} {}",
        String::from_utf8_lossy(&result.stdout).trim(),
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&result.stdout).trim().into())
}
fn unit_path(store: &Store) -> Result<PathBuf> {
    let name = id(store)?;
    let home = directories::BaseDirs::new().context("Cannot find home directory")?;
    Ok(match std::env::consts::OS {
        "linux" => home
            .config_dir()
            .join("systemd/user")
            .join(format!("{name}.service")),
        "macos" => home
            .home_dir()
            .join("Library/LaunchAgents")
            .join(format!("org.obiente.{name}.plist")),
        _ => directory(store).join("task.xml"),
    })
}
fn manager(store: &Store, action: &str) -> Result<Value> {
    let name = id(store)?;
    let path = text(&unit_path(store)?)?;
    let raw = match std::env::consts::OS {
        "linux" => output("systemctl", &["--user", action, &format!("{name}.service")])?,
        "macos" => {
            let uid = checked("id", &["-u"])?;
            let target = format!("gui/{uid}/org.obiente.{name}");
            match action {
                "start" => output("launchctl", &["bootstrap", &format!("gui/{uid}"), &path])?,
                "stop" => output("launchctl", &["bootout", &target])?,
                _ => output("launchctl", &["print", &target])?,
            }
        }
        "windows" => output(
            "schtasks.exe",
            &[
                match action {
                    "start" => "/Run",
                    "stop" => "/End",
                    _ => "/Query",
                },
                "/TN",
                &name,
            ],
        )?,
        _ => anyhow::bail!("Unsupported service manager"),
    };
    if action != "status" {
        ensure!(
            raw.status.success(),
            "Service manager: {} {}",
            String::from_utf8_lossy(&raw.stdout).trim(),
            String::from_utf8_lossy(&raw.stderr).trim()
        );
    }
    Ok(
        json!({"success":raw.status.success(),"output":String::from_utf8_lossy(&raw.stdout).trim(),"error":String::from_utf8_lossy(&raw.stderr).trim()}),
    )
}
pub fn status(store: &Store) -> Result<Value> {
    let config = settings(store)?;
    Ok(
        json!({"installed":config.as_ref().is_some_and(|s|s.state=="installed"),"state":config.as_ref().map(|s|&s.state),"port":config.as_ref().map(|s|s.port),"startup":"User sign-in. Linux can use loginctl enable-linger for startup without a login.","log":directory(store).join("dashboard.log"),"manager":if config.is_some(){manager(store,"status")?}else{json!(null)}}),
    )
}
pub fn summary(store: &Store) -> Result<Value> {
    let config = settings(store)?;
    let root = text(&fs::canonicalize(&store.root)?)?;
    let quoted = if cfg!(windows) {
        ps(&root)
    } else {
        format!("'{}'", root.replace('\'', "'\\''"))
    };
    Ok(
        json!({"installed":config.as_ref().is_some_and(|s|s.state=="installed"),"platform":std::env::consts::OS,"command_prefix":format!("selfhost --data-dir {quoted}"),"supervised":std::env::var("SELFHOST_DASHBOARD_SERVICE").as_deref()==Ok("1")}),
    )
}
fn install(store: &Store, catalog: Option<&Path>, port: u16, dry_run: bool) -> Result<Value> {
    ensure!(port > 0, "Choose a fixed nonzero port");
    let config = Settings {
        port,
        catalog: catalog.map(fs::canonicalize).transpose()?,
        state: "prepared".into(),
    };
    let user = if cfg!(windows) {
        checked(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
            ],
        )?
    } else {
        String::new()
    };
    let spec = definition(
        std::env::consts::OS,
        &id(store)?,
        &text(&std::path::absolute(executable(store))?)?,
        &arguments(store, &config)?,
        &user,
    )?;
    if dry_run {
        return Ok(
            json!({"definition":spec,"path":unit_path(store)?,"port":port,"startup":"User sign-in","copies_executable":true,"starts_now":false}),
        );
    }
    ensure!(
        !cfg!(debug_assertions),
        "Install a release CLI first (Cargo or npm); development binaries cannot be installed as services"
    );
    ensure!(
        settings(store)?.is_none(),
        "A service record already exists. Inspect dashboard status; uninstall it before replacing the service copy"
    );
    let unit = unit_path(store)?;
    ensure!(
        !unit.exists(),
        "A service definition already exists at this path; refusing to overwrite it"
    );
    if cfg!(windows) {
        ensure!(
            manager(store, "status")?["success"] != true,
            "A task already uses this workspace's service name; inspect it before installing"
        );
    }
    let dir = directory(store);
    private_dir(&dir)?;
    let mut config = config;
    atomic_write(
        &dir.join("service.json"),
        &serde_json::to_vec_pretty(&config)?,
    )?;
    atomic_write(&executable(store), &fs::read(std::env::current_exe()?)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(executable(store), fs::Permissions::from_mode(0o700))?;
    }
    fs::create_dir_all(unit.parent().context("Missing service directory")?)?;
    let definition_bytes = if cfg!(windows) {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(spec.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    } else {
        spec.into_bytes()
    };
    atomic_write(&unit, &definition_bytes)?;
    match std::env::consts::OS {
        "linux" => {
            checked("systemctl", &["--user", "daemon-reload"])?;
            checked(
                "systemctl",
                &["--user", "enable", &format!("{}.service", id(store)?)],
            )?;
        }
        "windows" => {
            checked(
                "schtasks.exe",
                &["/Create", "/TN", &id(store)?, "/XML", &text(&unit)?],
            )?;
        }
        "macos" => {}
        _ => unreachable!(),
    }
    config.state = "installed".into();
    atomic_write(
        &dir.join("service.json"),
        &serde_json::to_vec_pretty(&config)?,
    )?;
    Ok(
        json!({"installed":true,"starts_now":false,"next":"Run selfhost dashboard start, then selfhost dashboard logs for your local sign-in link","port":port}),
    )
}
fn uninstall(store: &Store) -> Result<Value> {
    let config = settings(store)?.context("No dashboard service is installed")?;
    if config.state == "installed" {
        stop(store)?;
    }
    let name = id(store)?;
    match std::env::consts::OS {
        "linux" => {
            if config.state == "installed" {
                checked(
                    "systemctl",
                    &["--user", "disable", &format!("{name}.service")],
                )?;
            }
        }
        "windows" => {
            if config.state == "installed" && manager(store, "status")?["success"] == true {
                checked("schtasks.exe", &["/Delete", "/TN", &name, "/F"])?;
            }
        }
        "macos" => {}
        _ => anyhow::bail!("Unsupported service manager"),
    }
    let unit = unit_path(store)?;
    if unit.exists() {
        fs::remove_file(unit)?;
    }
    if cfg!(target_os = "linux") {
        checked("systemctl", &["--user", "daemon-reload"])?;
    }
    // Keep the executable too: a running process or interrupted manager stop must not lose it.
    fs::remove_file(directory(store).join("service.json"))?;
    Ok(
        json!({"uninstalled":true,"port":config.port,"preserved":"Projects, credentials, logs and the private service binary remain in the data directory"}),
    )
}
fn stop(store: &Store) -> Result<Value> {
    let config = settings(store)?.context("No dashboard service installed")?;
    let result = manager(store, "stop");
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], config.port));
    for _ in 0..100 {
        if std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(100))
            .is_err()
        {
            return Ok(result.unwrap_or_else(
                |_| json!({"success":true,"message":"The dashboard listener is stopped"}),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    anyhow::bail!(
        "The dashboard port is still listening. Inspect the service manager before retrying; no replacement was started"
    )
}
fn domain(
    store: &Store,
    value: &str,
    port: u16,
    revision: Option<&str>,
    confirm: bool,
) -> Result<Value> {
    let url = crate::auth::identity_url(value)?;
    ensure!(
        url.scheme() == "https" && url.path() == "/" && port > 0,
        "Use an HTTPS origin without a path and a fixed backend port"
    );
    let origin = url.origin().ascii_serialization();
    let config = store.login_config()?;
    if let Some(mut config) = config {
        config.public_url = origin.clone();
        if let Some(revision) = revision {
            return crate::identity_setup::apply(store, config, revision, confirm);
        }
        let mut plan = crate::identity_setup::plan(store, &config)?;
        plan["proxy"] = proxy_help(&url, port);
        return Ok(plan);
    }
    ensure!(
        revision.is_none(),
        "Connect an identity provider before applying a public domain"
    );
    Ok(
        json!({"public_url":origin,"callback":format!("{origin}/auth/callback"),"proxy":proxy_help(&url,port),"next":"In Access, connect an identity provider using this Selfhost address. Remote access requires an explicitly authorized identity."}),
    )
}
fn proxy_help(url: &reqwest::Url, port: u16) -> Value {
    json!({"upstream":format!("http://127.0.0.1:{port}"),"preserve_host":url.authority(),"caddy":format!("{} {{\n  reverse_proxy 127.0.0.1:{port}\n}}",url.authority()),"nginx":format!("location / {{\n  proxy_pass http://127.0.0.1:{port};\n  proxy_set_header Host $http_host;\n  proxy_set_header X-Forwarded-Proto https;\n}}"),"notes":["Configure DNS and a valid TLS certificate on your proxy. Keep the original public Host header.","Selfhost stays bound to loopback. For a proxy on another host or in an LXC/container, use a secured tunnel to this backend. The proxy's own localhost is a different machine/namespace.","Forwarded headers cannot grant access. Configure the exact Selfhost public origin and an identity provider before remote sign-in."]})
}
pub async fn execute(
    store: &Store,
    catalog: Option<&Path>,
    command: DashboardCommand,
) -> Result<()> {
    let result = match command {
        DashboardCommand::Install { port, dry_run } => install(store, catalog, port, dry_run)?,
        DashboardCommand::Start => {
            ensure!(
                settings(store)?.is_some(),
                "Install the dashboard service first"
            );
            manager(store, "start")?
        }
        DashboardCommand::Stop => {
            ensure!(settings(store)?.is_some(), "No dashboard service installed");
            stop(store)?
        }
        DashboardCommand::Restart => {
            ensure!(settings(store)?.is_some(), "No dashboard service installed");
            stop(store)?;
            manager(store, "start")?
        }
        DashboardCommand::Status => status(store)?,
        DashboardCommand::Uninstall => uninstall(store)?,
        DashboardCommand::Logs { lines } => {
            let mut file = fs::File::open(directory(store).join("dashboard.log"))
                .context("No service log yet; start the installed dashboard first")?;
            let start = file.metadata()?.len().saturating_sub(512 * 1024);
            file.seek(SeekFrom::Start(start))?;
            let mut bytes = Vec::new();
            file.take(512 * 1024).read_to_end(&mut bytes)?;
            let content = String::from_utf8_lossy(&bytes);
            let tail = content
                .lines()
                .rev()
                .take(lines as usize)
                .collect::<Vec<_>>();
            for line in tail.into_iter().rev() {
                println!("{line}");
            }
            return Ok(());
        }
        DashboardCommand::Domain {
            url,
            port,
            revision,
            confirm_callbacks,
        } => domain(store, &url, port, revision.as_deref(), confirm_callbacks)?,
        DashboardCommand::Run { port } => {
            let dir = directory(store);
            private_dir(&dir)?;
            let log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("dashboard.log"))?;
            let mut args = vec!["--data-dir".into(), text(&store.root)?];
            if let Some(catalog) = catalog {
                args.extend(["--catalog-dir".into(), text(catalog)?]);
            }
            args.extend(["serve".into(), "--port".into(), port.to_string()]);
            let mut child = Command::new(std::env::current_exe()?);
            child
                .args(args)
                .env("SELFHOST_DASHBOARD_SERVICE", "1")
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                child.creation_flags(0x08000000);
            }
            #[cfg(windows)]
            let job = WindowsJob::new()?;
            let mut child = child.spawn()?;
            #[cfg(windows)]
            if let Err(error) = job.assign(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            let code = child.wait()?;
            ensure!(code.success(), "Dashboard exited with {code}");
            return Ok(());
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_definitions_escape_literal_arguments() {
        let args = vec![
            "--data-dir".into(),
            "/home/example/a b/$literal%value's".into(),
        ];
        let linux = definition("linux", "test", "/bin/selfhost", &args, "").unwrap();
        assert!(linux.contains("$$literal%%value's"));
        assert!(linux.contains("Restart=on-failure"));
        let windows = definition(
            "windows",
            "test",
            "C:\\App's\\selfhost.exe",
            &args,
            "S-1-5-21-123",
        )
        .unwrap();
        assert!(windows.contains("App&apos;s"));
        assert!(windows.contains("LeastPrivilege"));
        let mac = definition("macos", "test", "/bin/selfhost", &["a&b<c".into()], "").unwrap();
        assert!(mac.contains("a&amp;b&lt;c"));
    }
    #[test]
    fn domain_plan_never_saves_unconfigured_remote_access() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        assert!(domain(&store, "http://example.test", 8372, None, false).is_err());
        let plan = domain(&store, "https://example.test", 8372, None, false).unwrap();
        assert_eq!(plan["callback"], "https://example.test/auth/callback");
        assert!(store.login_config().unwrap().is_none());
    }
}
