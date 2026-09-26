# Keep the dashboard running

Selfhost can install a private executable copy and register it with your operating
system's user service manager. Closing the terminal then leaves the dashboard
running. The service uses the same workspace, SSH account and Docker access as
the account that installs it.

The default dashboard/API port is **8372**, from **SH** in ASCII: **S = 83** and
**H = 72**, joined as **8372**. Use `--port` for another port; existing explicitly
configured services keep their chosen port.

## Install and start

Use a release CLI installed through Cargo or npm. A temporary npx/pnpx invocation
also works: installation copies the native executable into your private data
directory, so the service does not depend on the package manager's cache.

```sh
selfhost dashboard install --dry-run
selfhost dashboard install --port 8372
selfhost dashboard start
selfhost dashboard logs
selfhost dashboard status
```

If you use `--data-dir` or `--catalog-dir`, pass the same options when installing.
Use the same `--data-dir` for subsequent management commands. **Access → Keep
Selfhost running** shows commands for the current workspace and port.

Installation does not start the service. Stop any foreground `selfhost serve`
using the same port before running `dashboard start`. The dry run prints the
service definition without registering it. Debug builds are not installable as
background services.

For loopback or wildcard listeners, the private log contains the one-use local
sign-in link. A listener bound to one private IP instead prints the configured
public sign-in address and local recovery instructions. Local links expire after ten
minutes. Use your configured identity provider for later sign-ins, or restart
the dashboard and read the new link locally. Do not share the log.

## Startup behavior

| Platform | Manager                                          | Startup                                                        |
| -------- | ------------------------------------------------ | -------------------------------------------------------------- |
| Linux    | systemd user service                             | At user sign-in; enable lingering for startup without a login  |
| Windows  | Scheduled Task, current account, least privilege | At user sign-in; no stored password or administrator elevation |
| macOS    | LaunchAgent                                      | At user sign-in                                                |

On a Linux server, enable lingering for the service account when you want it to
start at boot and remain after logout:

```sh
loginctl enable-linger
```

Your system may require administrator approval for that setting. Windows and
macOS installations are per-user sign-in services, not system services that run
before login. The operating-system account must already have the required Docker
and SSH permissions. Selfhost does not change group membership or sudo rules.

## Stop, restart and remove

```sh
selfhost dashboard stop
selfhost dashboard restart
selfhost dashboard uninstall
```

These commands affect the dashboard service, not application containers. Removing
the service preserves projects, credentials, backups, logs and its private binary.
Plan stops and restarts while no deployment, backup or migration is running: an
operating-system service stop can interrupt in-progress work. Schedules run only
while the dashboard is running.

To upgrade the private service copy, install the new CLI with your package manager,
stop and uninstall the old user service, then run `dashboard install` and
`dashboard start` with the same data directory. The copy is not replaced
automatically when an npm cache or separate Cargo installation changes.

If installation fails, inspect `dashboard status` and the native manager before
retrying. A prepared record is not a running service. Selfhost refuses to overwrite
an existing definition or task. Interrupted registration may need a manual review
of the exact task shown in the installation plan.

## Use a domain and your existing proxy

Selfhost listens on `127.0.0.1` by default. Configure an HTTPS reverse proxy
with your desired DNS name, valid TLS and the original public `Host` header.

```sh
selfhost dashboard domain https://selfhost.example.com --port 8372
```

This prints the backend address, exact OIDC callback and Caddy/Nginx configuration
examples. It does not write to your proxy or change DNS. If an identity provider
is already configured, it also returns a reviewed address-change revision.

For Caddy on the same host:

```text
selfhost.example.com {
  reverse_proxy 127.0.0.1:8372
}
```

For an existing Nginx TLS server block:

```nginx
location / {
  proxy_pass http://127.0.0.1:8372;
  proxy_set_header Host $http_host;
  proxy_set_header X-Forwarded-Proto https;
}
```

For Traefik or Nginx Proxy Manager, use the same HTTP loopback backend and preserve
the public Host header. Configure TLS at the proxy. A container's loopback address
is its own namespace, not the host's loopback.

### Proxy on another server, VM or LXC

#### First-time setup on a headless server

> **Available since 0.1.3:** `--setup` enables temporary private-IP setup.
> With older releases, use the SSH tunnel described below to access the loopback
> dashboard from your browser.

On a firewalled server, VM or LXC without a local browser, start a temporary setup
listener on its concrete private or VPN IP:

```sh
selfhost serve --bind 10.20.0.10 --setup
```

Replace `10.20.0.10` with an IP assigned to that machine. Open the full one-use
link printed in the server terminal on your browser's machine. The link expires
after ten minutes and the administrator session lasts thirty minutes. Restart
the same command if you need a fresh link. API access still requires sign-in;
knowing the IP alone does not grant access.

Setup uses **HTTP**. Restrict the firewall to your browser's machine and use a
trusted private network or an encrypted VPN such as WireGuard or NetBird. A
firewall limits reachability but does not encrypt traffic. Never expose this
setup listener to the internet. Wildcard addresses (`0.0.0.0` and `::`) are
rejected in setup mode: choose one exact IP for the browser address.

In **Access**, configure the HTTPS Selfhost address, identity provider and allowed
administrators. Point your proxy at the private IP and port, preserve the public
Host header, and test domain sign-in while the setup session remains open. Then
stop the foreground process and restart without `--setup`, or install the
background service below using the same data directory. Temporary setup is never
saved in background service settings. Activate dashboard updates after leaving
setup mode.

#### Persistent remote hosting

> **Available since 0.1.2:** `--bind` selects the listener address. Version 0.1.1
> uses a loopback listener; use the SSH tunnel below with that release.

For direct access from a remote proxy, bind the backend to its **private or VPN IP**.
`0.0.0.0` also works, but listens on every IPv4 interface, including public ones.
`::` selects the IPv6 wildcard; whether it also accepts IPv4 depends on the host.
Binding does not configure DNS, TLS, routing or firewall rules.

First configure an HTTPS Selfhost address, identity provider and explicitly allowed
administrators using temporary setup, the loopback dashboard or identity CLI.
Normal remote binding refuses to start without these settings. Verify domain
sign-in before switching the listener. Then, on the Selfhost machine:

```sh
selfhost serve --bind 10.20.0.10 --port 8372
```

Replace `10.20.0.10` with an address assigned to that machine, reachable from the
proxy. For persistent hosting, install the listener settings with the service:

```sh
selfhost dashboard install --bind 10.20.0.10 --port 8372
selfhost dashboard start
selfhost dashboard domain https://selfhost.example.com --bind 10.20.0.10 --port 8372
```

An already installed service must be stopped and uninstalled before reinstalling
with a different bind address. Its workspace is preserved. `dashboard status`
shows the saved bind address; web update restarts retain the selected address. Existing service settings without a bind field stay on
`127.0.0.1`. The domain command prints proxy examples; it does not change the
running listener. Give it a reachable IP, never `0.0.0.0` or `::` as an upstream.

Point the proxy at `http://10.20.0.10:8372`, preserve the public `Host` header, and
terminate browser HTTPS at the proxy. Restrict inbound backend traffic to the
proxy's IP using a host firewall and use WireGuard, NetBird or another authenticated
encrypted tunnel between hosts. The backend speaks HTTP; TLS at the public proxy
does not encrypt that second connection. Do not publish the backend port through
your router. `--bind` does not enforce a proxy source-IP allowlist for you.

Remote clients must sign in through the configured HTTPS origin. Direct private-IP
URLs and forged localhost/forwarded headers do not enable remote local recovery.
Only an explicitly started temporary `--setup` listener permits one-use sign-in
on its exact private-IP URL.
If a private-IP listener needs recovery, stop it and run `selfhost serve --bind
127.0.0.1` with the same data directory, then use the new local sign-in link.

#### Keep the backend on loopback instead

To open a headless server's loopback dashboard from your own browser, run this
on your browser's machine and keep the SSH connection open:

```sh
ssh -N -o ExitOnForwardFailure=yes -L 8372:127.0.0.1:8372 user@selfhost-host
```

Run `selfhost serve` on the server, then open its printed
`http://127.0.0.1:8372/#token=...` link in your local browser. Replace the SSH
target with your server's account and address. Keep the local forwarded port the
same as the Selfhost listener port so the browser's origin matches.

Create an authenticated tunnel from the proxy's network namespace to the Selfhost
host's loopback listener. For example, run this on the proxy host with a verified
SSH alias and key:

```sh
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 127.0.0.1:18372:127.0.0.1:8372 selfhost-host
```

Use `http://127.0.0.1:18372` as the proxy's backend. Supervise the tunnel separately
using your host's service manager. For a proxy container, the tunnel must be
reachable inside that container's network namespace. Do not expose an unencrypted
management backend on a public interface.

### Configure sign-in and apply the address

In **Access**, set the HTTPS Selfhost address and connect an identity provider.
Only explicitly allowed subjects can administer Selfhost. Forwarded headers do
not grant access; the backend checks the configured public origin.

When moving an existing login from localhost to a domain, add the new callback to
every provider first. Keep the old callback and local recovery session until the
new address works. Then apply the reviewed revision:

```sh
selfhost dashboard domain https://selfhost.example.com --port 8372 --revision REVIEWED_REVISION --confirm-callbacks
```

The same review is available in Access. Previous settings are backed up and old
provider sessions are invalidated. The domain command cannot enable remote access
without an identity provider. See [identity setup](identity-setup.md) for details.
