# Terminal interface

Run `selfhost tui` to open the interactive terminal workspace. Use the same data directory as the dashboard and other CLI commands:

```sh
selfhost --data-dir ./selfhost-data tui
```

The interface calls Selfhost's application APIs directly. It does not construct shell commands from menu choices or ask you to type CLI commands. Your operating-system account controls access to the private workspace, so opening it grants administrator access to that workspace.

## Navigation

| Key                             | Action                                         |
| ------------------------------- | ---------------------------------------------- |
| Up / Down                       | Select a menu item or move through an editor   |
| Enter                           | Open the selected item or accept a text field  |
| Type in a menu                  | Filter its options                             |
| Backspace in a menu             | Remove the last filter character               |
| Space in a multiple-choice list | Toggle an option                               |
| Escape                          | Return or discard an unfinished form           |
| Ctrl+U in a text prompt         | Clear the field                                |
| Left / Right                    | Move the text cursor, or pan long output lines |
| Page Up / Page Down             | Scroll menus and output                        |
| Ctrl+S in the document editor   | Accept the edited document                     |

The terminal uses a separate screen and restores normal terminal mode when it exits. Selected rows have an arrow as well as color. Secret prompts mask input. Output is scrollable and filters terminal control sequences and direction overrides from application text.

Once an operation begins, the interface waits for its result before accepting another operation. Navigation or terminal rendering failures do not cancel an in-progress operation. A failed operation is reported without an automatic retry. Review its result and any recovery record before trying again.

## Workspace sections

### Projects and apps

Choose a project to start, stop, restart or refresh its services. You can review its generated Compose plan, save a configuration snapshot, change a generated project's name, image references and ports, edit environment variables and mounted files, or export an ordinary Compose archive.

Each service has resource usage, logs, lifecycle actions and actions declared by its recipe. Native integrations expose common settings first, with advanced settings available separately. Settings changes are previewed and applied with the backend's revision check. Native configuration backups can be previewed and restored. App onboarding asks for the fields supported by the selected setup mode, masks passwords and lets you choose existing service links or add your own by name and URL. Declared app workflows collect their typed inputs, including masked secrets. Lists are edited one item at a time; command arguments keep spaces and punctuation intact and can be reordered. Structured settings use named fields, lists and typed values without requiring JSON syntax.

App identity connections have creation plans and a saved-progress view. Resuming a connection uses its existing registration rather than silently creating another provider client. The dashboard-sync operation accepts a masked credential.

### Create a project or deployment

Choose an app to see its deployment methods, create a project containing several apps, write a custom Compose document or select a stack template. Declarative stack inputs appear as typed questions. AIO's Docker access and lifecycle requirements must be acknowledged individually.

Creation prepares a stopped project. Review it before starting it. A custom setup with complex Compose structure can be edited in the built-in document editor; environment variables and mounted files also have dedicated simple forms. Removing a service from the Compose document is rejected: use the reviewed removal flow instead.

### Existing apps

Link an app by URL, or select a connected Docker server and choose from matching containers. You can also enter an exact container name. The saved connection pins the container's identity. If it has been recreated, choose Reconnect, select its replacement and review its image and server. Reconnection clears management permissions so you can review them again. Read-only connection details, status and resource usage are separate from management permissions. Write operations require the individual action to be enabled, a writable server connection, backup acknowledgement and the exact app name.

Unlinking deletes only the Selfhost reference. It does not stop or delete the external application. See [Existing apps](existing-apps.md).

### Servers and moves

Add Docker SSH, Docker context or Proxmox SSH connections using named fields. Remote management does not require a local Docker engine; Docker SSH connections use your local Docker client and SSH access to the remote engine. A Proxmox host connection manages that host's guests; connect separately to a guest when managing Docker inside it. New connections are read-only. Enabling management requires the exact connection name.

You can inspect infrastructure, check Docker, edit connection settings, review a Docker workload move, assign an undeployed project, recover a saved move or review a stopped Proxmox VM/LXC migration. Proxmox task status can be queried by node and task ID. The same backend ownership, reachability, storage and read-only checks used by the dashboard apply.

### Databases

Host a shared database source or connect an existing database using a supported engine. Configure a project for a dedicated container, a shared isolated database or an external database. Test, rename or edit saved sources from the database menu. Editing tests the new connection before saving; leaving a replacement password blank preserves the existing password. Sources used by projects cannot change destination. Removal asks for the source name, refuses sources still in use and removes only the connection record. Hosted database projects and all database data remain. Credentials are masked. Changing a database binding does not migrate existing data automatically.

Supported databases can be provisioned, backed up, exported and restored. Restore requires the project name, uses the backend's safety backup and stops the app containers before replacing database contents. See [Setups and databases](setups-and-databases.md).

### Networking and reverse proxies

Add and edit proxy connections, including credentials, private CA certificates and provider-defined settings. Register a private network and its policy reference, review and apply a service route, or test upstream HTTPS from the proxy host.

Registering a network records an existing network. Peer enrollment, key distribution and firewall policy deployment remain tasks for that network's provider. See [Networking and login](networking-and-login.md).

### Identity and dashboard login

Connect an existing OIDC provider through named fields, inspect a provider configuration directory, apply its login manifest, register a client on a supported provider or edit an existing provider. The address-change flow accepts localhost or a domain URL and shows exact callback changes.

Automatic registration defaults to creating a provider project named Selfhost. Reusing a project and overriding the organization are advanced choices. A temporary credential can look up its human account for review; you must explicitly grant the suggested account access. Machine credentials require a human administrator's subject ID instead. The credential is masked and is not saved by the terminal interface.

Every login change goes through a revision-bound plan. You must confirm that all exact callbacks are configured at the provider. Keep the previous callback and a local recovery session until the new address has been tested. Advanced JSON editing is available for complete configurations and private CA details. See [Identity setup](identity-setup.md).

### Backups and removal recovery

Configuration snapshots, database archives, removal archives and interrupted moves are accessible from one section. Project and service removal always asks for scope and backup coverage before generating a plan. Review the exact containers, ownership evidence, exclusions and warnings, then type the required confirmation.

Removal preserves named volumes, bind mounts, networks and external database sources. A configuration snapshot is not a backup of app data or a container's writable layer. Recovery can restore a project record or reconcile an interrupted removal without starting or deleting runtime resources. See [Removal and recovery](removal.md).

### Activity and schedules

Browse activity, mark it as read and manage configuration snapshots, image refreshes or supported scheduled native workflows. Scheduled operations run while `selfhost serve` is running; the terminal interface itself is not a background scheduler.

### Selfhost updates

The workspace heading shows an available version from the last cached check. Opening menus does not contact a package registry. Use **Check for updates now** for a fresh check and installation-specific instructions.

Supported Cargo installations can review the exact release, build it in private staging, then activate the staged update with a separate typed confirmation. Activation exits the terminal interface and restores normal terminal mode so the helper can replace the executable. Reopen Selfhost after the helper finishes. Source builds, npm installations and other binaries show their appropriate update instructions instead of replacing themselves.

Update history includes failure details and recovery instructions. When safe executable recovery is available, it requires its own typed confirmation and exits Selfhost. Recovery restores the executable and Cargo installation records; it does not undo data migrations. Back up the Selfhost data directory before upgrading. See [Updates](updates.md).

## Advanced data and accessibility

JSON and Compose editors are reserved for structured documents that benefit from preserving their full representation. They stay inside the terminal and do not create temporary plaintext files or invoke an external editor. Review your terminal environment before opening documents containing credentials. Exported configuration and database archives remain sensitive files.

For screen-reader environments that work better with linear output than an alternate-screen terminal interface, every existing CLI command remains available. The terminal interface displays current stats and logs on demand; reopen the view to refresh them.

The interface exposes the currently implemented backend operations. It does not add unsupported capabilities, such as live Proxmox migration, automatic private-network enrollment, generic data migration for arbitrary custom Compose projects or a backup engine for every external app.
