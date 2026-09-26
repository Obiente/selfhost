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

The private log contains the one-use local sign-in link. It expires after ten
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

Selfhost continues listening only on `127.0.0.1`. Configure an HTTPS reverse proxy
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
