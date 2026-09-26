# Dashboard guide

The left navigation switches between projects, existing apps, servers, networking,
access, catalog, databases, schedules, snapshots and activity. The account button
shows the identity you're using and lets you sign out.

## Projects and services

A project groups a deployment and its configuration. Select one to see its
services. A service view has resource statistics, logs and actions. Actions are
defined by the app's configuration, so different apps expose different controls.

- **Start project** applies the saved configuration and starts its containers.
- **Project settings** changes ports, images and network access.
- **Environment, files & database** opens the portable setup editor.
- **Save snapshot** saves configuration and generated credentials, not volume data.
- **Export setup** downloads ordinary Compose, environment and configuration files.

Review changed settings before starting a project. A changed host port or domain
can also require changes to proxies and identity-provider callbacks.

## Choose a deployment method

Apps with multiple supported methods offer a deployment picker. Nextcloud can use
the single-image recipe or the AIO manager. AIO has special Docker permissions and
fixed resources, so its requirements must be acknowledged before project creation.
If you already run AIO, use **Existing apps** instead of creating a second manager.

## Existing apps

An existing-app connection is separate from a managed project. A URL connection
gives you a place to open the app; a verified Docker connection adds status,
statistics and supported actions. Write actions need explicit permission and
confirmation. See [the existing-app guide](existing-apps.md).

## Access

Choose **Connect existing** for an identity provider you already run, or expand
**Host an identity provider** to create a new deployment. The manual flow shows
common settings first. Advanced settings include private CA certificates and
additional providers. The directory CLI path can inspect your provider setup.
See [identity setup](identity-setup.md).

## Servers and networking

**Infrastructure** registers servers and inspects clusters, VMs and containers.
Use read-only connections when you only want visibility. **Networking** connects
reverse proxies and records private network paths. Read-only restrictions are
enforced by the backend, not just by disabling buttons.

## Removing something

Use **Remove project** or **Remove this app** to open a review. Choose whether to
archive its Selfhost record or stop and remove its owned containers, then choose
a backup option. Confirm the exact name and review the data that will be retained.
No general Docker prune is used.

Open **Projects → View archived projects** to restore a removed record. If an
operation was interrupted, use **Reconcile removal** and inspect the result before
starting anything. See [removal and recovery](removal.md).

## Keyboard and accessibility

Use Tab to move through controls and Enter or Space to activate them. The first
keyboard link skips navigation. Dialogs move focus inside, keep keyboard focus
within their controls, and restore focus when closed. Escape closes ordinary
dialogs; an operation already in progress may require completion first.

Reduced-motion preferences are respected. Forms use visible labels, selected
controls expose their state, and status/error messages are announced. Automated
checks and keyboard testing are useful safeguards, but do not replace testing
with different browsers and assistive technologies.
