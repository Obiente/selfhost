//! Read-only diagnostics on the machine hosting Selfhost. Never reads client SSH configuration.
use anyhow::{Result, ensure};
use clap::Args;
use serde::Serialize;
use serde_json::Value;
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

#[derive(Args, Default)]
pub struct Options {
    /// SSH hostname or alias reachable from your computer (defaults to the SSH session's server IP)
    #[arg(long)]
    pub ssh_host: Option<String>,
    #[arg(long)]
    pub ssh_user: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub ssh_port: Option<u16>,
    /// Client address for evaluating server-side SSH Match rules outside an SSH session
    #[arg(long)]
    pub client_ip: Option<IpAddr>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug)]
pub struct Remote {
    client: IpAddr,
    server: IpAddr,
    port: u16,
    user: String,
}
impl Remote {
    fn parse(connection: &str, user: &str) -> Option<Self> {
        let parts: Vec<_> = connection.split_whitespace().collect();
        if parts.len() != 4 || !safe_user(user) {
            return None;
        }
        let client_port: u16 = parts[1].parse().ok()?;
        let port = parts[3].parse().ok()?;
        if client_port == 0 || port == 0 {
            return None;
        }
        Some(Self {
            client: parts[0].parse().ok()?,
            server: parts[2].parse().ok()?,
            port,
            user: user.into(),
        })
    }
    pub fn detect() -> Option<Self> {
        let user = std::env::var("SUDO_USER")
            .ok()
            .filter(|u| safe_user(u))
            .or_else(|| std::env::var("USER").ok())?;
        Self::parse(&std::env::var("SSH_CONNECTION").ok()?, &user)
    }
}
fn safe_user(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn safe_host(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-[]".contains(&b))
}

#[derive(Serialize)]
pub struct Check {
    pub name: &'static str,
    pub status: &'static str,
    pub message: String,
}
fn check(name: &'static str, status: &'static str, message: impl Into<String>) -> Check {
    Check {
        name,
        status,
        message: message.into(),
    }
}
#[derive(Serialize)]
pub struct Report {
    pub bind: IpAddr,
    pub port: u16,
    pub remote_session: bool,
    pub tunnel_command: String,
    pub tunnel_url: String,
    pub tunnel_requires_loopback_restart: bool,
    pub checks: Vec<Check>,
}

impl Report {
    fn new(bind: IpAddr, port: u16, remote: Option<&Remote>, options: &Options) -> Result<Self> {
        let host = options
            .ssh_host
            .clone()
            .or_else(|| remote.map(|r| r.server.to_string()))
            .unwrap_or_else(|| "YOUR_SSH_HOST".into());
        let user = options
            .ssh_user
            .clone()
            .or_else(|| remote.map(|r| r.user.clone()))
            .unwrap_or_else(|| "YOUR_SSH_USER".into());
        ensure!(
            safe_host(&host) && safe_user(&user),
            "Use an SSH hostname/IP/alias and username without shell metacharacters"
        );
        ensure!(port != 0, "Use the actual listener port for diagnostics");
        let ssh_port = options
            .ssh_port
            .or_else(|| remote.map(|r| r.port))
            .unwrap_or(22);
        let loopback = if bind.is_loopback() {
            bind
        } else if bind.is_ipv6() {
            "::1".parse()?
        } else {
            "127.0.0.1".parse()?
        };
        let address = SocketAddr::new(loopback, port);
        Ok(Self {
            bind,
            port,
            remote_session: remote.is_some(),
            tunnel_command: format!(
                "ssh -N -o ClearAllForwardings=no -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -p {ssh_port} -L '{address}:{address}' {user}@{host}"
            ),
            tunnel_url: format!("http://{address}/"),
            tunnel_requires_loopback_restart: !bind.is_loopback() && !bind.is_unspecified(),
            checks: Vec::new(),
        })
    }
    pub fn print(&self) {
        println!("Dashboard access checks:");
        for c in &self.checks {
            println!("  [{}] {}: {}", c.status, c.name, c.message);
        }
        if self.tunnel_requires_loopback_restart {
            println!(
                "For SSH tunnel access, first stop this listener and run on the server with the same data directory:"
            );
            println!(
                "  selfhost serve --bind {} --port {}",
                if self.bind.is_ipv6() {
                    "::1"
                } else {
                    "127.0.0.1"
                },
                self.port
            );
        }
        println!("Run this tunnel command on your computer and leave it running:");
        println!("  {}", self.tunnel_command);
        println!(
            "Then open {} with the #token=... from the server's one-use sign-in link.",
            self.tunnel_url
        );
        println!(
            "If the local port is occupied, choose another --port on the server and use that same port in the tunnel and browser."
        );
        println!(
            "Host checks cannot verify your computer's routing, Proxmox/cloud firewalls, or SSH key/certificate forwarding restrictions. No settings were changed."
        );
    }
}

fn executable(name: &str) -> Option<PathBuf> {
    [
        "/usr/sbin",
        "/sbin",
        "/usr/bin",
        "/bin",
        "/usr/local/sbin",
        "/usr/local/bin",
    ]
    .iter()
    .map(|dir| PathBuf::from(dir).join(name))
    .find(|p| p.is_file())
}
async fn probe(name: &str, args: &[String]) -> Option<String> {
    let mut command = tokio::process::Command::new(executable(name)?);
    command.args(args).env("LC_ALL", "C");
    crate::core::run(command, 3).await.ok()
}

fn ssh_policy(text: &str, destination: SocketAddr) -> Check {
    let setting = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name} ")))
    };
    if setting("disableforwarding") == Some("yes")
        || matches!(setting("allowtcpforwarding"), Some("no" | "remote"))
    {
        return check(
            "SSH forwarding",
            "blocked",
            format!(
                "The server disables local TCP forwarding. Ask its administrator for a rule scoped to your SSH account and source address: AllowTcpForwarding local and PermitOpen {destination}."
            ),
        );
    }
    let target_ip = destination.ip().to_string();
    let target_port = destination.port().to_string();
    if let Some(allowed) = setting("permitopen") {
        let matches_target = allowed.split_whitespace().any(|entry| {
            if entry == "any" {
                return true;
            }
            let Some((host, port)) = entry.rsplit_once(':') else {
                return false;
            };
            (host == "*" || host.trim_matches(['[', ']']) == target_ip)
                && (port == "*" || port == target_port)
        });
        if !matches_target {
            return check(
                "SSH forwarding",
                "blocked",
                format!(
                    "The server's PermitOpen policy does not allow {destination}. Ask its administrator to allow this exact dashboard destination."
                ),
            );
        }
    } else {
        return check(
            "SSH forwarding",
            "unknown",
            "Could not determine the server's permitted forwarding destinations.",
        );
    }
    if matches!(setting("allowtcpforwarding"), Some("yes" | "all" | "local")) {
        check(
            "SSH forwarding",
            "observed",
            format!(
                "Effective daemon policy permits local forwarding to {destination}. Key/certificate restrictions and hostname-based Match rules still need a real tunnel test."
            ),
        )
    } else {
        check(
            "SSH forwarding",
            "unknown",
            "Could not determine server-side TCP forwarding policy.",
        )
    }
}

fn nft_policy(text: &str, port: u16) -> Option<Check> {
    let document: Value = serde_json::from_str(text).ok()?;
    let objects = document["nftables"].as_array()?;
    let restricted: Vec<_> = objects
        .iter()
        .filter_map(|o| o.get("chain"))
        .filter(|c| matches!(c["hook"].as_str(), Some("input" | "output")) && c["policy"] == "drop")
        .filter_map(|c| c["hook"].as_str())
        .collect();
    if restricted.is_empty() {
        Some(check(
            "Host firewall",
            "unknown",
            "nftables rules were readable; no default-drop input/output chain was found. Explicit rules, other firewall layers and upstream filtering can still block access.",
        ))
    } else {
        Some(check(
            "Host firewall",
            "possible blocker",
            format!(
                "nftables has default-drop {} policy. Direct access needs a matching TCP {port} input allowance; an SSH tunnel needs the server's connection to its local dashboard permitted. Review loopback input/output rules. This is not an end-to-end reachability test.",
                restricted.join("/")
            ),
        ))
    }
}
async fn firewall(port: u16) -> Check {
    if let Some(output) = probe("nft", &["-j".into(), "list".into(), "ruleset".into()]).await
        && let Some(result) = nft_policy(&output, port)
    {
        return result;
    }
    if let Some(output) = probe("iptables", &["-S".into()]).await {
        let drops = output
            .lines()
            .any(|line| matches!(line, "-P INPUT DROP" | "-P OUTPUT DROP"));
        return check(
            "Host firewall",
            if drops { "possible blocker" } else { "unknown" },
            if drops {
                format!(
                    "IPv4 iptables has a default-drop INPUT or OUTPUT policy. Review TCP {port} and loopback allowances; IPv6 and upstream filtering remain unverified."
                )
            } else {
                "IPv4 iptables rules were readable. Explicit rules, IPv6 and upstream firewalls remain unverified.".into()
            },
        );
    }
    check(
        "Host firewall",
        "unknown",
        "Firewall rules are unavailable or unreadable with this account. An administrator can inspect nftables/iptables (including rules managed by UFW or firewalld); Proxmox/cloud filtering must be checked separately.",
    )
}

pub async fn diagnose(
    bind: IpAddr,
    port: u16,
    options: &Options,
    listener_open: bool,
    login: Option<&crate::auth::LoginConfig>,
) -> Result<Report> {
    let remote = Remote::detect();
    let mut report = Report::new(bind, port, remote.as_ref(), options)?;
    let local_ip = if bind.is_unspecified() {
        if bind.is_ipv6() {
            "::1".parse()?
        } else {
            "127.0.0.1".parse()?
        }
    } else {
        bind
    };
    let tunnel_ip = if report.tunnel_requires_loopback_restart {
        if bind.is_ipv6() {
            "::1".parse()?
        } else {
            "127.0.0.1".parse()?
        }
    } else {
        local_ip
    };
    if let Some(address) = crate::server::recovery_address(tunnel_ip, port, login) {
        report.tunnel_url = format!("http://{address}/");
    } else {
        // A custom 127/8 address can conflict with a configured HTTPS login origin.
        // Use the standard loopback for recovery, just as the server instructs.
        let recovery_bind: IpAddr = if bind.is_ipv6() {
            "::1".parse()?
        } else {
            "127.0.0.1".parse()?
        };
        let recovery_report = Report::new(recovery_bind, port, remote.as_ref(), options)?;
        report.tunnel_command = recovery_report.tunnel_command;
        report.tunnel_url = format!(
            "http://{}/",
            crate::server::recovery_address(recovery_bind, port, login)
                .expect("standard loopback has a recovery address")
        );
        report.tunnel_requires_loopback_restart = true;
    }
    let destination = SocketAddr::new(local_ip, port);
    let reachable = listener_open
        || matches!(
            tokio::time::timeout(
                Duration::from_secs(2),
                tokio::net::TcpStream::connect(destination)
            )
            .await,
            Ok(Ok(_))
        );
    report.checks.push(check("Listener", if reachable { "observed" } else { "blocked" }, if reachable {
        format!("TCP {destination} is listening locally. {}", if bind.is_loopback() { "A remote browser cannot connect to this loopback address directly; use the SSH tunnel below." } else { "Local listening does not prove that another machine can reach this port." })
    } else { format!("No TCP connection to {destination} succeeded within two seconds. Check the running process, its --bind/--port, and local firewall rules.") }));
    let user = options
        .ssh_user
        .as_deref()
        .or_else(|| remote.as_ref().map(|r| r.user.as_str()));
    let client = options
        .client_ip
        .or_else(|| remote.as_ref().map(|r| r.client));
    let server = remote
        .as_ref()
        .map(|r| r.server)
        .or_else(|| options.ssh_host.as_ref().and_then(|host| host.parse().ok()))
        .or_else(|| (!bind.is_unspecified() && !bind.is_loopback()).then_some(bind));
    // SSH_CONNECTION describes the daemon endpoint even behind port translation.
    // --ssh-port only changes the suggested client command during an SSH session.
    let port_ssh = remote
        .as_ref()
        .map(|r| r.port)
        .or(options.ssh_port)
        .unwrap_or(22);
    let ssh = async {
        if let (Some(user), Some(client), Some(server)) = (user, client, server) {
            let spec =
                format!("user={user},addr={client},host={client},laddr={server},lport={port_ssh}");
            if let Some(output) = probe("sshd", &["-T".into(), "-C".into(), spec]).await {
                let tunnel_ip = if report.tunnel_requires_loopback_restart {
                    if bind.is_ipv6() {
                        "::1".parse().unwrap()
                    } else {
                        "127.0.0.1".parse().unwrap()
                    }
                } else {
                    local_ip
                };
                return ssh_policy(&output, SocketAddr::new(tunnel_ip, port));
            }
        }
        check(
            "SSH forwarding",
            "unknown",
            "Could not inspect effective server SSH policy. Run from the SSH session with an account able to read sshd configuration and host keys. No elevation is attempted.",
        )
    };
    let (ssh, firewall) = tokio::join!(ssh, firewall(port));
    report.checks.extend([ssh, firewall]);
    if let Some(remote) = remote {
        report.checks.push(check("SSH transport", "observed", format!("This SSH session reached {}:{} from {}. That does not prove direct browser access to TCP {port}.", remote.server, remote.port, remote.client)));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tunnel_instructions_preserve_ports_and_never_inject_shell_text() {
        let remote = Remote::parse("192.0.2.20 55000 192.0.2.10 2222", "operator").unwrap();
        let report = Report::new(
            "127.0.0.1".parse().unwrap(),
            8372,
            Some(&remote),
            &Options::default(),
        )
        .unwrap();
        assert!(
            report
                .tunnel_command
                .contains("-p 2222 -L '127.0.0.1:8372:127.0.0.1:8372' operator@192.0.2.10")
        );
        assert_eq!(report.tunnel_url, "http://127.0.0.1:8372/");
        assert!(!report.tunnel_requires_loopback_restart);
        assert!(
            Report::new(
                "192.0.2.10".parse().unwrap(),
                8372,
                Some(&remote),
                &Options::default()
            )
            .unwrap()
            .tunnel_requires_loopback_restart
        );
        let ipv6 = Report::new(
            "::1".parse().unwrap(),
            8372,
            Some(&remote),
            &Options::default(),
        )
        .unwrap();
        assert!(ipv6.tunnel_command.contains("'[::1]:8372:[::1]:8372'"));
        for host in [
            "--proxy-command=evil",
            "host;evil",
            "$(evil)",
            "host\nother",
        ] {
            assert!(
                Report::new(
                    "127.0.0.1".parse().unwrap(),
                    8372,
                    Some(&remote),
                    &Options {
                        ssh_host: Some(host.into()),
                        ..Options::default()
                    }
                )
                .is_err()
            );
        }
        assert!(Remote::parse("192.0.2.20 55000 192.0.2.10 2222", "root;evil").is_none());
        assert!(Remote::parse("garbage", "root").is_none());
    }
    #[test]
    fn ssh_policy_separates_blockers_from_unverified_key_restrictions() {
        let target = "127.0.0.1:8372".parse().unwrap();
        for policy in [
            "allowtcpforwarding no\npermitopen any",
            "allowtcpforwarding remote\npermitopen any",
            "disableforwarding yes\nallowtcpforwarding yes\npermitopen any",
            "allowtcpforwarding local\npermitopen 127.0.0.1:22",
            "allowtcpforwarding local\npermitopen none",
        ] {
            assert_eq!(ssh_policy(policy, target).status, "blocked");
        }
        for permit in [
            "any",
            "127.0.0.1:8372",
            "*:8372",
            "127.0.0.1:*",
            "127.0.0.1:22 127.0.0.1:8372",
        ] {
            assert_eq!(
                ssh_policy(
                    &format!("allowtcpforwarding local\npermitopen {permit}"),
                    target
                )
                .status,
                "observed"
            );
        }
        assert_eq!(ssh_policy("", target).status, "unknown");
    }
    #[tokio::test]
    async fn diagnostics_use_recovery_origin_and_report_closed_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let login = crate::auth::LoginConfig {
            public_url: format!("https://{address}"),
            ..Default::default()
        };
        let report = diagnose(
            address.ip(),
            address.port(),
            &Options::default(),
            false,
            Some(&login),
        )
        .await
        .unwrap();
        assert_eq!(
            report.tunnel_url,
            format!("http://localhost:{}/", address.port())
        );
        assert_eq!(report.checks[0].status, "observed");
        drop(listener);
        let closed = diagnose(
            address.ip(),
            address.port(),
            &Options::default(),
            false,
            None,
        )
        .await
        .unwrap();
        assert_eq!(closed.checks[0].status, "blocked");
        let login = crate::auth::LoginConfig {
            public_url: format!("https://127.0.0.2:{}", address.port()),
            ..Default::default()
        };
        let report = diagnose(
            "127.0.0.2".parse().unwrap(),
            address.port(),
            &Options::default(),
            true,
            Some(&login),
        )
        .await
        .unwrap();
        assert!(report.tunnel_requires_loopback_restart);
        assert_eq!(
            report.tunnel_url,
            format!("http://127.0.0.1:{}/", address.port())
        );
        assert!(!report.tunnel_command.contains("127.0.0.2"));
    }
    #[test]
    fn firewall_policy_never_claims_end_to_end_connectivity() {
        let restricted = r#"{"nftables":[{"chain":{"hook":"input","policy":"drop"}},{"chain":{"hook":"output","policy":"drop"}}]}"#;
        assert_eq!(
            nft_policy(restricted, 8372).unwrap().status,
            "possible blocker"
        );
        assert_eq!(
            nft_policy(r#"{"nftables":[]}"#, 8372).unwrap().status,
            "unknown"
        );
        assert!(nft_policy("permission denied", 8372).is_none());
    }
}
