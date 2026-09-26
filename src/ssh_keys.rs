//! Dedicated OpenSSH identities. Public metadata is safe to return; private keys never leave disk.
use crate::core::{Store, atomic_write, private_dir, run, token};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Docker,
    Proxmox,
    Proxy,
}
#[derive(Args, Deserialize)]
#[group(id = "ssh_key_create")]
#[serde(deny_unknown_fields)]
pub struct Create {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub host: String,
    #[arg(long)]
    pub user: String,
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    #[arg(long, value_enum, default_value = "docker")]
    pub purpose: Purpose,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub purpose: Purpose,
    pub public_key: String,
    pub fingerprint: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trust {
    pub public_key: String,
    pub fingerprint: String,
}
#[derive(Subcommand)]
pub enum SshCommand {
    /// Guide or resume a dedicated SSH connection, including host trust and authorization
    Setup {
        #[arg(long, value_enum)]
        purpose: Option<Purpose>,
    },
    /// List dedicated Selfhost SSH connections (public information only)
    List,
    /// Generate a dedicated Ed25519 automation key without a passphrase
    Create {
        #[command(flatten)]
        input: Create,
    },
    /// Show the public key, authorization command and connection instructions
    Show { id: String },
    /// Discover an untrusted server host key; verify its fingerprint separately
    Scan { id: String },
    /// Pin a server key after comparing its fingerprint through a trusted channel
    Trust {
        id: String,
        #[arg(long)]
        public_key: String,
        #[arg(long)]
        fingerprint: String,
    },
    /// Enable an OpenSSH alias on this machine, preserving the existing config
    Enable { id: String },
    /// Test pinned host trust and the dedicated key; does not change the server
    Test { id: String },
    /// Disable the alias and delete the local key after revoking remote access
    Remove {
        id: String,
        #[arg(long)]
        revoked: bool,
    },
}
fn safe_word(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn safe_host(s: &str) -> bool {
    safe_word(s) || s.parse::<std::net::IpAddr>().is_ok()
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn config_path(path: &Path) -> Result<String> {
    let s = path
        .to_str()
        .context("SSH paths must be Unicode")?
        .replace('\\', "/");
    ensure!(
        !s.contains(['\n', '\r', '\0', '"', '%', '$']),
        "SSH paths cannot contain quotes, percent signs, dollar signs or control characters"
    );
    Ok(format!("\"{s}\""))
}
fn regular(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Refusing a linked or non-file SSH path"
        );
    }
    Ok(())
}
fn directory(store: &Store) -> Result<PathBuf> {
    let p = store.root.join("ssh");
    if p.exists() {
        ensure!(
            !fs::symlink_metadata(&p)?.file_type().is_symlink(),
            "SSH directory cannot be a link"
        );
    }
    private_dir(&p)?;
    let absolute = fs::canonicalize(p)?;
    #[cfg(windows)]
    let absolute = {
        let text = absolute.to_str().context("SSH paths must be Unicode")?;
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            PathBuf::from(rest)
        } else {
            absolute
        }
    };
    Ok(absolute)
}
fn location(store: &Store, id: &str) -> Result<PathBuf> {
    ensure!(
        id.starts_with("sh-") && id.len() == 27 && id[3..].bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SSH connection ID"
    );
    let p = directory(store)?.join(id);
    ensure!(
        p.is_dir() && !fs::symlink_metadata(&p)?.file_type().is_symlink(),
        "SSH connection not found"
    );
    Ok(p)
}
fn profile_lock(dir: &Path) -> Result<fs::File> {
    let path = dir.join("lock");
    regular(&path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock_exclusive()
        .context("Another operation is using this SSH connection; retry shortly")?;
    Ok(file)
}
fn profile(store: &Store, id: &str) -> Result<Profile> {
    let p = location(store, id)?.join("profile.json");
    regular(&p)?;
    let v: Profile = serde_json::from_slice(&fs::read(p)?)?;
    ensure!(v.id == id, "SSH connection metadata mismatch");
    Ok(v)
}
fn key_text(value: &str) -> Result<String> {
    let parts: Vec<_> = value.split_whitespace().collect();
    ensure!(
        parts.len() == 2
            && parts[0] == "ssh-ed25519"
            && parts[1].len() <= 256
            && parts[1]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)),
        "Use an Ed25519 public key: ssh-ed25519 followed by its key data"
    );
    Ok(parts.join(" "))
}
async fn fingerprint(dir: &Path, key: &str) -> Result<String> {
    let file = tempfile::NamedTempFile::new_in(dir)?.into_temp_path();
    fs::write(&file, format!("{key}\n"))?;
    let mut c = Command::new("ssh-keygen");
    c.args(["-l", "-E", "sha256", "-f"]).arg(&file);
    let output = run(c, 5).await?;
    let fp = output
        .split_whitespace()
        .nth(1)
        .context("Missing SSH fingerprint")?;
    ensure!(fp.starts_with("SHA256:"), "Invalid SSH fingerprint");
    Ok(fp.into())
}
fn host_label(p: &Profile) -> String {
    if p.port == 22 {
        p.host.clone()
    } else {
        format!("[{}]:{}", p.host, p.port)
    }
}
fn config(p: &Profile, dir: &Path) -> Result<String> {
    Ok(format!(
        "Host {}\n  HostName {}\n  User {}\n  Port {}\n  IdentityFile {}\n  IdentitiesOnly yes\n  IdentityAgent none\n  BatchMode yes\n  StrictHostKeyChecking yes\n  UserKnownHostsFile {}\n  GlobalKnownHostsFile none\n  UpdateHostKeys no\n  ForwardAgent no\n  ForwardX11 no\n  ClearAllForwardings yes\n  RequestTTY no\n  ConnectTimeout 10\n",
        p.id,
        p.host,
        p.user,
        p.port,
        config_path(&dir.join("identity"))?,
        config_path(&dir.join("known_hosts"))?
    ))
}
fn block(p: &Profile, dir: &Path) -> Result<String> {
    Ok(format!(
        "# BEGIN SELFHOST {}\n{}Host *\n# END SELFHOST {}\n",
        p.id,
        config(p, dir)?,
        p.id
    ))
}
fn ssh_home() -> Result<PathBuf> {
    Ok(directories::BaseDirs::new()
        .context("Cannot locate the SSH account home directory")?
        .home_dir()
        .join(".ssh"))
}
fn enabled(p: &Profile, dir: &Path, home: &Path) -> Result<bool> {
    let path = home.join("config");
    regular(&path)?;
    let text = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    Ok(text.contains(&block(p, dir)?))
}
fn authorization(p: &Profile) -> String {
    let restrictions = match p.purpose {
        Purpose::Docker => "restrict,command=\"docker system dial-stdio\"",
        _ => "restrict",
    };
    format!("{restrictions} {} selfhost:{}", p.public_key, p.id)
}
fn install_script(p: &Profile) -> String {
    let line = quote(&authorization(p));
    format!(
        "umask 077\nmkdir -p \"$HOME/.ssh\"\nchmod 700 \"$HOME/.ssh\"\ntouch \"$HOME/.ssh/authorized_keys\"\nchmod 600 \"$HOME/.ssh/authorized_keys\"\ngrep -qxF -- {line} \"$HOME/.ssh/authorized_keys\" || printf '%s\\n' {line} >> \"$HOME/.ssh/authorized_keys\""
    )
}
impl Store {
    pub fn ssh_profiles(&self) -> Result<Vec<Value>> {
        let mut result = vec![];
        for entry in fs::read_dir(directory(self)?)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if id.starts_with("sh-") && entry.file_type()?.is_dir() {
                result.push(self.ssh_show(&id)?);
            }
        }
        Ok(result)
    }
    pub fn ssh_show(&self, id: &str) -> Result<Value> {
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        Ok(
            json!({"profile":p,"host_trusted":dir.join("known_hosts").is_file(),"tested":dir.join("tested").is_file(),"alias_enabled":enabled(&p,&dir,&ssh_home()?)?,"authorization":authorization(&p),"install_script":install_script(&p),"config":config(&p,&dir)?,"test_command":format!("selfhost ssh test {id}"),"revoke_instructions":format!("On {} as {}, remove the authorized_keys line ending in selfhost:{id}. Then remove this local connection.",p.host,p.user)}),
        )
    }
    pub async fn ssh_create(&self, input: Create) -> Result<Value> {
        ensure!(
            !input.name.trim().is_empty() && input.name.len() <= 64,
            "Choose a name up to 64 characters"
        );
        ensure!(
            safe_host(&input.host) && safe_word(&input.user) && input.port > 0,
            "Use a hostname/IP, SSH username and nonzero port without shell syntax"
        );
        let id = format!("sh-{}", token(12)?);
        let root = directory(self)?;
        let pending = tempfile::Builder::new()
            .prefix("pending-")
            .tempdir_in(&root)?;
        let dir = pending.path().to_path_buf();
        private_dir(&dir)?;
        let destination = root.join(&id);
        let mut c = Command::new("ssh-keygen");
        c.args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-C",
            &format!("selfhost:{id}"),
            "-f",
        ])
        .arg(dir.join("identity"));
        run(c,15).await.context("Could not generate a key. Install OpenSSH client tools on the machine running Selfhost")?;
        let raw = fs::read_to_string(dir.join("identity.pub"))?;
        let public_key = key_text(&raw.split_whitespace().take(2).collect::<Vec<_>>().join(" "))?;
        let fingerprint = fingerprint(&dir, &public_key).await?;
        let p = Profile {
            id,
            name: input.name.trim().into(),
            host: input.host,
            user: input.user,
            port: input.port,
            purpose: input.purpose,
            public_key,
            fingerprint,
        };
        atomic_write(&dir.join("config"), config(&p, &destination)?.as_bytes())?;
        atomic_write(&dir.join("profile.json"), &serde_json::to_vec_pretty(&p)?)?;
        fs::rename(&dir, &destination)?;
        self.ssh_show(&p.id)
    }
    pub async fn ssh_scan(&self, id: &str) -> Result<Value> {
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        let mut c = Command::new("ssh-keyscan");
        c.args([
            "-T",
            "5",
            "-t",
            "ed25519",
            "-p",
            &p.port.to_string(),
            &p.host,
        ]);
        let output = run(c, 8).await.context(
            "Could not read the server host key. Check its SSH address, port and firewall",
        )?;
        let mut keys = output
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let parts: Vec<_> = l.split_whitespace().collect();
                (parts.len() == 3).then(|| format!("{} {}", parts[1], parts[2]))
            })
            .collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        ensure!(
            keys.len() == 1,
            "The address returned no unique Ed25519 host key. Use an exact server address or enter its verified key manually"
        );
        let public_key = key_text(&keys[0])?;
        Ok(
            json!({"public_key":public_key,"fingerprint":fingerprint(&dir,&public_key).await?,"trusted":false}),
        )
    }
    pub async fn ssh_trust(&self, id: &str, input: Trust) -> Result<Value> {
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        let _lock = profile_lock(&dir)?;
        let public_key = key_text(&input.public_key)?;
        ensure!(
            fingerprint(&dir, &public_key).await? == input.fingerprint.trim(),
            "Fingerprint does not match this host key"
        );
        let path = dir.join("known_hosts");
        regular(&path)?;
        ensure!(
            !path.exists()
                || fs::read_to_string(&path)? == format!("{} {}\n", host_label(&p), public_key),
            "A different host key is already pinned. Create a new connection after verifying the server change"
        );
        atomic_write(
            &path,
            format!("{} {}\n", host_label(&p), public_key).as_bytes(),
        )?;
        self.ssh_show(id)
    }
    pub fn ssh_enable(&self, id: &str) -> Result<Value> {
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        let _lock = profile_lock(&dir)?;
        ensure!(
            dir.join("known_hosts").is_file(),
            "Verify and trust the server host key first"
        );
        ensure!(
            dir.join("tested").is_file(),
            "Test the dedicated key before enabling its alias"
        );
        edit_alias(&p, &dir, &ssh_home()?, false)?;
        self.ssh_show(id)
    }
    pub async fn ssh_test(&self, id: &str) -> Result<Value> {
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        let _lock = profile_lock(&dir)?;
        ensure!(
            dir.join("known_hosts").is_file(),
            "Verify and trust the server host key first"
        );
        let mut c = Command::new("ssh");
        c.arg("-F").arg(dir.join("config")).args(["-T", "--", id]);
        if p.purpose == Purpose::Docker {
            // The authorized key forces dial-stdio. Query a harmless Docker API endpoint.
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            c.arg("docker system dial-stdio")
                .kill_on_drop(true)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            c.creation_flags(0x08000000);
            let mut child = c.spawn()?;
            let mut stdin = child.stdin.take().unwrap();
            let mut stdout = child.stdout.take().unwrap();
            let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
                stdin
                    .write_all(b"GET /_ping HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n")
                    .await?;
                let mut bytes = vec![];
                (&mut stdout).take(8192).read_to_end(&mut bytes).await?;
                ensure!(
                    String::from_utf8_lossy(&bytes)
                        .lines()
                        .next()
                        .is_some_and(|l| l.contains(" 200 ")),
                    "SSH connected but Docker did not answer its health check"
                );
                Ok::<_, anyhow::Error>(())
            })
            .await;
            let _ = child.kill().await;
            result.context(
                "SSH/Docker test timed out; check the authorized key and Docker permissions",
            )??;
        } else {
            c.arg("printf selfhost-ssh-ok");
            ensure!(
                run(c, 15).await?.trim() == "selfhost-ssh-ok",
                "Unexpected SSH test response"
            );
        }
        atomic_write(&dir.join("tested"), b"verified")?;
        Ok(
            json!({"connected":true,"id":id,"message":"Pinned host key and dedicated SSH key verified"}),
        )
    }
    pub fn ssh_remove(&self, id: &str, revoked: bool) -> Result<Value> {
        ensure!(
            revoked,
            "Revoke this public key on the remote server first, then confirm with --revoked"
        );
        ensure!(
            !self.servers()?.iter().any(|s| s.endpoint == id),
            "This key is used by a server connection. Keep it until that connection is removed"
        );
        let networking = self.networking()?;
        ensure!(
            !networking["data"]["proxies"]
                .as_array()
                .is_some_and(|p| p.iter().any(|p| p["ssh_alias"] == id)),
            "This key is used by a proxy connection"
        );
        let p = profile(self, id)?;
        let dir = location(self, id)?;
        let _lock = profile_lock(&dir)?;
        edit_alias(&p, &dir, &ssh_home()?, true)?;
        drop(_lock);
        fs::remove_dir_all(dir)?;
        Ok(json!({"removed":true}))
    }
}
fn edit_alias(p: &Profile, dir: &Path, home: &Path, remove: bool) -> Result<()> {
    if !home.exists() {
        private_dir(home)?;
    }
    ensure!(
        !fs::symlink_metadata(home)?.file_type().is_symlink(),
        "SSH directory cannot be a link"
    );
    let lock_path = home.join(".selfhost-config.lock");
    regular(&lock_path)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock_exclusive()
        .context("Another SSH configuration change is in progress; retry shortly")?;
    let path = home.join("config");
    regular(&path)?;
    let before = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let managed = block(p, dir)?;
    ensure!(
        !before.contains(&format!("# BEGIN SELFHOST {}", p.id)) || before.contains(&managed),
        "This alias was edited outside Selfhost. Review it manually before changing it"
    );
    let after = if remove {
        before.replace(&managed, "")
    } else if before.contains(&managed) {
        return Ok(());
    } else {
        format!("{managed}{before}")
    };
    if after == before {
        return Ok(());
    }
    if !before.is_empty() {
        atomic_write(
            &dir.join(format!("ssh-config-backup-{}", token(6)?)),
            before.as_bytes(),
        )?;
    }
    atomic_write(&path, after.as_bytes())?;
    Ok(())
}
pub async fn execute(store: &Store, command: SshCommand) -> Result<()> {
    let result = match command {
        SshCommand::Setup { purpose } => {
            let purpose = match purpose {
                Some(purpose) => purpose,
                None => [Purpose::Docker, Purpose::Proxy, Purpose::Proxmox][crate::guided::choose(
                    "Connection purpose",
                    &[
                        "Docker host".into(),
                        "Reverse proxy".into(),
                        "Proxmox host".into(),
                    ],
                )?],
            };
            let alias = guided_setup(store, purpose).await?;
            println!("Connection ready: {alias}");
            return Ok(());
        }
        SshCommand::List => json!(store.ssh_profiles()?),
        SshCommand::Create { input } => store.ssh_create(input).await?,
        SshCommand::Show { id } => store.ssh_show(&id)?,
        SshCommand::Scan { id } => store.ssh_scan(&id).await?,
        SshCommand::Trust {
            id,
            public_key,
            fingerprint,
        } => {
            store
                .ssh_trust(
                    &id,
                    Trust {
                        public_key,
                        fingerprint,
                    },
                )
                .await?
        }
        SshCommand::Enable { id } => store.ssh_enable(&id)?,
        SshCommand::Test { id } => store.ssh_test(&id).await?,
        SshCommand::Remove { id, revoked } => store.ssh_remove(&id, revoked)?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// One resumable flow. Only the operator installs authorization on the remote host.
pub async fn guided_setup(store: &Store, purpose: Purpose) -> Result<String> {
    use crate::guided as prompt;
    prompt::interactive()?;
    let existing = store
        .ssh_profiles()?
        .into_iter()
        .filter(|v| {
            serde_json::from_value::<Profile>(v["profile"].clone())
                .is_ok_and(|p| p.purpose == purpose)
        })
        .collect::<Vec<_>>();
    let mut choices = vec!["Create a dedicated key".into()];
    choices.extend(existing.iter().map(|v| {
        format!(
            "Continue {} ({}@{})",
            v["profile"]["name"].as_str().unwrap_or_default(),
            v["profile"]["user"].as_str().unwrap_or_default(),
            v["profile"]["host"].as_str().unwrap_or_default()
        )
    }));
    let selected = if existing.is_empty() {
        0
    } else {
        prompt::choose("Dedicated SSH connection", &choices)?
    };
    let mut state = if selected == 0 {
        let name = prompt::input("Connection name", "")?;
        let host = prompt::input("SSH hostname or IP address", "")?;
        let user = prompt::input("SSH account", "")?;
        let port = prompt::input("SSH port", "22")?
            .parse::<u16>()
            .context("SSH port must be a number")?;
        prompt::review(
            &json!({"name":name,"host":host,"user":user,"port":port,"purpose":purpose,"key":"Dedicated Ed25519 key stored privately on this machine"}),
        );
        ensure!(
            prompt::confirm("Create this dedicated key?", false)?,
            "Cancelled"
        );
        store
            .ssh_create(Create {
                name,
                host,
                user,
                port,
                purpose,
            })
            .await?
    } else {
        existing[selected - 1].clone()
    };
    let id = state["profile"]["id"]
        .as_str()
        .context("Missing SSH identity")?
        .to_owned();
    println!("Progress is saved. Run selfhost ssh setup again to continue this connection.");
    if state["host_trusted"] != true {
        let key = match store.ssh_scan(&id).await {
            Ok(scan) => {
                println!(
                    "Server presented {}",
                    scan["fingerprint"].as_str().unwrap_or_default()
                );
                scan["public_key"]
                    .as_str()
                    .context("Missing server key")?
                    .to_owned()
            }
            Err(error) => {
                println!("Host key discovery failed: {error}");
                prompt::input("Verified server host public key (from its console)", "")?
            }
        };
        println!(
            "Use the server console or another trusted channel to obtain its host key fingerprint. A network scan alone does not verify the server."
        );
        println!(
            "On a standard OpenSSH server: ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256"
        );
        let fingerprint = prompt::input("Paste the independently verified SHA256 fingerprint", "")?;
        state = store
            .ssh_trust(
                &id,
                Trust {
                    public_key: key,
                    fingerprint,
                },
            )
            .await?;
        println!("Host key verified and pinned.");
    }
    if state["tested"] != true {
        let profile: Profile = serde_json::from_value(state["profile"].clone())?;
        println!(
            "Run this command on {} while signed in as {}:",
            profile.host, profile.user
        );
        println!(
            "\n{}\n",
            state["install_script"]
                .as_str()
                .context("Missing authorization command")?
        );
        match purpose {
            Purpose::Docker => println!(
                "This key is restricted to the Docker API. Docker access grants control over that Docker host."
            ),
            _ => println!(
                "This key disables forwarding and interactive terminals, but allows commands with this account's permissions. Use a dedicated account with only the required access."
            ),
        }
        ensure!(
            prompt::confirm(
                "Have you installed this authorization on that account?",
                false
            )?,
            "Setup paused; the private key and verified host are saved"
        );
    }
    loop {
        match store.ssh_test(&id).await {
            Ok(_) => break,
            Err(error) => {
                println!("Connection test failed: {error}");
                println!(
                    "Check the account authorization and network access. Your setup progress is saved."
                );
                ensure!(
                    prompt::confirm("Retry the connection test?", false)?,
                    "Setup paused; run selfhost ssh setup to resume"
                );
            }
        }
    }
    println!("Dedicated key and server connection verified.");
    if store.ssh_show(&id)?["alias_enabled"] != true {
        ensure!(
            prompt::confirm(
                "Enable this alias in this account's SSH configuration? Existing entries are preserved.",
                false
            )?,
            "Setup paused after successful test; resume to enable its alias"
        );
        store.ssh_enable(&id)?;
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Profile {
        Profile {
            id: format!("sh-{}", "a".repeat(24)),
            name: "Test connection".into(),
            host: "192.0.2.10".into(),
            user: "operator".into(),
            port: 2222,
            purpose: Purpose::Docker,
            public_key: "ssh-ed25519 SYNTHETIC".into(),
            fingerprint: "SHA256:test".into(),
        }
    }
    #[test]
    fn rejects_input_injection_and_limits_docker_authorization() {
        for s in ["-oBad", "host;id", "a\nb", "a b", "$(id)"] {
            assert!(!safe_host(s));
            assert!(!safe_word(s));
        }
        assert!(safe_host("2001:db8::10"));
        assert!(key_text("ssh-ed25519 key\ncommand=bad").is_err());
        let p = fixture();
        assert!(authorization(&p).starts_with("restrict,command=\"docker system dial-stdio\" "));
        let dir = tempfile::tempdir().unwrap();
        let c = config(&p, dir.path()).unwrap();
        assert!(c.contains("StrictHostKeyChecking yes"));
        assert!(c.contains("IdentityAgent none"));
        assert!(!c.contains("accept-new"));
    }
    #[test]
    fn alias_edits_are_scoped_idempotent_and_keep_existing_config() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        private_dir(&home).unwrap();
        let dir = temp.path().join("key");
        private_dir(&dir).unwrap();
        let before = "Host existing\n  HostName example.test\n  User operator\n";
        fs::write(home.join("config"), before).unwrap();
        let p = fixture();
        edit_alias(&p, &dir, &home, false).unwrap();
        edit_alias(&p, &dir, &home, false).unwrap();
        let current = fs::read_to_string(home.join("config")).unwrap();
        assert!(current.ends_with(before));
        assert_eq!(current.matches("# BEGIN SELFHOST").count(), 1);
        assert!(fs::read_dir(&dir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("ssh-config-backup-")
        }));
        edit_alias(&p, &dir, &home, true).unwrap();
        assert_eq!(fs::read_to_string(home.join("config")).unwrap(), before);
        edit_alias(&p, &dir, &home, false).unwrap();
        let edited = fs::read_to_string(home.join("config"))
            .unwrap()
            .replace("User operator", "User changed");
        fs::write(home.join("config"), &edited).unwrap();
        assert!(edit_alias(&p, &dir, &home, true).is_err());
        assert_eq!(fs::read_to_string(home.join("config")).unwrap(), edited);
    }
    #[tokio::test]
    async fn generated_key_is_private_and_host_trust_cannot_be_silently_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().join("workspace")).unwrap();
        let value = store
            .ssh_create(Create {
                name: "Test".into(),
                host: "192.0.2.10".into(),
                user: "operator".into(),
                port: 22,
                purpose: Purpose::Proxy,
            })
            .await
            .unwrap();
        let id = value["profile"]["id"].as_str().unwrap();
        assert!(!value.to_string().contains("PRIVATE KEY"));
        assert_eq!(value["host_trusted"], false);
        assert!(store.ssh_enable(id).is_err());
        let public_key = value["profile"]["public_key"].as_str().unwrap().to_string();
        let fingerprint = value["profile"]["fingerprint"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            store
                .ssh_trust(
                    id,
                    Trust {
                        public_key: public_key.clone(),
                        fingerprint: "SHA256:wrong".into()
                    }
                )
                .await
                .is_err()
        );
        assert!(
            store
                .ssh_trust(
                    id,
                    Trust {
                        public_key: public_key.clone(),
                        fingerprint: fingerprint.clone()
                    }
                )
                .await
                .unwrap()["host_trusted"]
                == true
        );
        assert!(store.ssh_enable(id).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(location(&store, id).unwrap().join("identity"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let second = store
            .ssh_create(Create {
                name: "Other".into(),
                host: "192.0.2.11".into(),
                user: "operator".into(),
                port: 22,
                purpose: Purpose::Proxy,
            })
            .await
            .unwrap();
        assert!(
            store
                .ssh_trust(
                    id,
                    Trust {
                        public_key: second["profile"]["public_key"].as_str().unwrap().into(),
                        fingerprint: second["profile"]["fingerprint"].as_str().unwrap().into()
                    }
                )
                .await
                .is_err()
        );
        assert!(store.ssh_remove(id, false).is_err());
        assert!(store.ssh_show("../../escape").is_err());
    }
}
