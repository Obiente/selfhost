# Existing apps and deployment choices

## Connect an app you already run

Open **Existing apps** and choose **Link an app**. Select the app type, give it a name and enter its web address. HTTPS domains and HTTP loopback URLs are accepted. A URL link opens the app without changing its server.

Choose **Other app or website** for any service without a dedicated profile. That option provides a URL link; container inspection and commands require a profile defining the expected image repositories. A saved URL does not imply that Selfhost has checked the app's health or identity-provider configuration.

For status, resource usage and supported app commands, enable **Connect its Docker container**. Choose a server from Infrastructure and select a discovered matching container. Advanced users can still supply a full container ID. A Docker connection can point to a host, a Docker VM or a Docker LXC. Connect to the guest running Docker, not just its Proxmox host.

Selfhost reads only the selected container's ID, image identity, name and state. It does not import environment variables, mount paths, volumes or Compose files. The running image must match the selected profile's exact image repository. It saves the full container ID, so reusing the same container name cannot silently redirect commands to another container.

### Nextcloud AIO

Choose **Nextcloud AIO** and select the Nextcloud app container, usually `nextcloud-aio-nextcloud`. Do not select the master container. Status and installed-app listing are available with a read-only server connection.

The **Management permissions** dialog can enable specific operations such as updating Nextcloud apps. Both the server and that individual action must permit writes. Each write requires the exact app name again. AIO remains responsible for core upgrades, container lifecycle and its backups. Selfhost does not take over AIO's generated Compose stack.

If AIO recreates the Nextcloud container during an update, choose Reconnect and select the replacement after reviewing both identities. Management permissions are cleared and must be reviewed again. Selfhost never follows a new container automatically.

### ZITADEL

Choose **ZITADEL** to link its web address and optionally its Docker container. A ZITADEL installation managed by systemd can be linked by URL. Connecting Selfhost login is a separate step under **Access** and requires an OIDC client and permitted administrator subjects. Linking the app does not grant it authentication privileges.

### Unlink safely

Choose **Unlink** and type the exact app name. This deletes only Selfhost's reference. The external app, its containers, database, configuration and backups remain intact. A backup is unnecessary for unlinking because no app data changes.

Linked apps deliberately live outside managed projects. Project stop, update, migration, export and removal operations cannot reach them.

## Choose how a new app runs

Apps with multiple deployment profiles show a deployment selector. Nextcloud currently offers:

| Method              | Appropriate starting point                                        | Who manages the runtime                                            |
| ------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------ |
| Single Docker image | Localhost, a small installation, or a custom database arrangement | Selfhost with ordinary editable Compose                            |
| Nextcloud AIO       | An integrated stack with AIO backups and upgrades                 | Selfhost starts the AIO manager; AIO owns its companion containers |

### AIO requirements

Creating an AIO deployment requires acknowledging Docker engine access, its fixed resource names and its separately managed lifecycle. A read-only mount of a Docker socket still permits Docker API writes and effectively grants control over the engine host.

The generated AIO configuration uses the standard Linux Docker socket, binds its HTTPS administration port to loopback and configures a loopback Apache port for a reverse proxy. Use an SSH forward to reach administration on another server. Nextcloud itself needs a domain and HTTPS proxy during AIO setup; choose the single-image method for a localhost-only installation. Custom socket locations, rootless engines and SELinux may require reviewed host-specific changes and are not automatically inferred.

Before starting or refreshing a deployment, Selfhost checks its declared fixed container and volume names. An existing resource must belong to this exact Selfhost Compose project; otherwise startup is refused. Link an existing AIO installation instead of trying to install a second manager.

Stopping the Selfhost AIO project stops only the administration container. AIO's child services can remain running. Use AIO for its stack lifecycle and backup operations. Do not treat a Selfhost configuration snapshot or project export as an AIO data backup.

## CLI workflow

Use the same `--data-dir` as the dashboard when managing its connections.

```sh
selfhost existing link
selfhost existing reconnect
selfhost existing permissions CONNECTION_ID
selfhost existing list
selfhost existing inspect CONNECTION_ID
selfhost existing action CONNECTION_ID status
```

`existing link` asks for an app profile, name and URL, then offers container discovery on a saved Docker host. It displays only matching images for that profile. Linking starts without management permissions. `existing permissions` lets you choose the individual operations you want to allow and reviews the result before saving.

`existing reconnect` keeps the saved app name and URL, checks the replacement container's image against the linked profile, and binds the reviewed change to its immutable container identity. It refuses a stale plan or a container already linked elsewhere. Permission reset prevents previously approved writes from transferring silently to a different container.

No JSON file or copied revision is required for these interactive flows. Explicit `--file` inputs remain available for automation. Use the same `--data-dir` as the dashboard. Production server connections can remain read-only until you explicitly enable management.

## Contributor configuration

Existing app profiles are JSON files under `catalog/existing/`. Local overrides belong in `existing-profiles/` inside Selfhost's private data directory. Profiles declare exact image repositories and optional fixed command argument arrays. Arguments are passed directly to Docker exec without a shell. Write operations must set `write: true` and should explain backup requirements.

The profile is copied into each linked reference. Editing a catalog profile does not silently change commands or privileges for existing links. Review and relink to adopt a new profile.

Deployment profiles under `catalog/deployments/` are arrays mapping an app and method ID to either one recipe or one stack blueprint. Local overrides belong in `deployment-profiles/`. Stack blueprints declare inputs, requirements, fixed resources and Compose configuration. App-specific image names, container names, paths and commands stay in those files.

An explicit `x-selfhost.host_resources.docker_socket` declaration permits only `/var/run/docker.sock:/var/run/docker.sock:ro`. General absolute host bind paths remain rejected by portable setup validation. Exports are ordinary Compose, but a declared host resource must exist on the destination engine. Exports contain configuration and credentials, not application data.

## Source references

- [Nextcloud AIO Compose requirements](https://github.com/nextcloud/all-in-one/blob/main/compose.yaml)
- [Nextcloud AIO reverse proxy setup](https://github.com/nextcloud/all-in-one/blob/main/reverse-proxy.md)
- [Nextcloud OCC administration](https://docs.nextcloud.com/server/latest/admin_manual/occ_command.html)
- [ZITADEL readiness and health](https://zitadel.com/docs/apis/observability/health)
