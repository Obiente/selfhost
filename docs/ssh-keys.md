# Dedicated SSH keys

> Available since 0.1.4.

Use **Infrastructure → Dedicated SSH connections** for Docker and Proxmox hosts,
or **Networking → Proxies → Dedicated SSH connections** for a proxy host, VM or
LXC. Keys are created on the machine running Selfhost, under its protected data
directory. The dashboard and CLI must use the same operating-system account and
`--data-dir` to share connections.

## Guided dashboard setup

1. Enter a connection name, hostname/IP, SSH account, port and purpose.
2. Read the server's host key. Compare its SHA-256 fingerprint through the server
   console or another trusted channel before trusting it. A scan alone does not
   establish trust. The console command is
   `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256`.
3. Copy the generated authorization command and run it as the selected SSH account
   on the destination. It adds the public key once and preserves existing keys.
4. Test the dedicated key. Then explicitly enable the alias on the Selfhost host.
   Selfhost adds a scoped entry to that account's OpenSSH config and keeps a private
   backup of its previous contents. Existing entries remain intact.
5. Choose **Use this connection** to fill the server or proxy form with the alias.

The test uses the pinned host key and dedicated identity. Host-key changes fail
closed. To replace a host key, verify the change independently and create a new
connection. An enabled alias also works with ordinary OpenSSH and Docker commands.

## CLI setup

```sh
selfhost ssh setup
```

Choose Docker, Proxmox or proxy access. The wizard creates a dedicated key, guides host-key verification and authorization, tests the connection, then offers to enable its alias. It never accepts a scanned host key without the independently verified fingerprint.

`selfhost server-add` and `selfhost proxy-add` include this same wizard, so you can complete the whole connection in one flow. There is no need to stop and collect IDs from other commands. Already-created keys are offered when you resume after a failed connection test or interrupted setup.

One command must be run on the destination account to authorize its public key. Selfhost shows that command and asks you to confirm it is installed. It does not assume an existing account password or authorize itself remotely. The private key stays on the Selfhost host.

OpenSSH client utilities (`ssh`, `ssh-keygen`, `ssh-keyscan`) must be installed on the Selfhost host. Remote Docker operations also require the Docker CLI there; a local Docker engine is unnecessary.

For automation, the explicit `ssh create`, `scan`, `trust`, `test` and `enable` commands remain available. Run `selfhost ssh --help` for their options.

## Permissions and lifetime

Keys use Ed25519 and have no passphrase so unattended operations can use them.
Private keys are never returned by the API. Keep the Selfhost account, data
directory and backups private. Users who control that account can use its keys.

- **Docker:** the authorized key forces `docker system dial-stdio` and disables
  terminal allocation and forwarding. Docker API access can control the host.
  Selfhost's read-only setting restricts its own actions; it is not a Docker or
  SSH permission boundary.
- **Proxy and Proxmox:** remote commands run with the selected account's privileges.
  The key disables terminal allocation, SSH forwarding and agent forwarding.
  Give the account only the command/file permissions it needs. Do not add broad
  passwordless sudo rules as a shortcut.

Enabling an alias modifies the SSH configuration of the account running Selfhost,
not the browser user's computer. Global SSH configuration can still add identity
files or other behavior to ordinary alias usage. Review broad `Host *` settings
when using additional identities; the dedicated connection test uses its own
configuration file. Selfhost does not change the destination's SSH daemon policy,
firewall, account membership or sudo configuration.

The generated key, public authorization line and config are ordinary OpenSSH
artifacts. Keep them to continue using the connection without Selfhost. If you
move the Selfhost data directory, update identity and known-hosts paths in the
alias accordingly.

To revoke access, remove the exact `authorized_keys` line ending in
`selfhost:CONNECTION_ID` on the destination. Then remove the local identity:

```sh
selfhost ssh remove CONNECTION_ID --revoked
```

Removal refuses keys still referenced by a saved server or proxy. Deleting a local
key alone cannot revoke copies or sessions on another machine. Keep the existing
connection until any replacement has been tested.
