# Remove an app or project

Open a project's removal action to choose what should happen. Removal always has a review step and requires the exact displayed app or project name.

## Choose the scope

- **Archive in Selfhost** removes the selection from active management. Containers keep running. Project files, volumes, images, networks and external databases are retained. App-specific schedules are removed when archiving one app; all project schedules are removed when archiving a project.
- **Remove containers** stops and deletes only the exact containers listed in the review. Persistent volumes, bind mounts, networks, images and database sources are retained. Data written only to a container's writable layer is lost when that container is deleted.

Selfhost does not run Docker prune, remove volumes, drop shared databases, or remove externally linked apps through these actions. Unlinking an existing app removes its reference only.

Deleting an existing service from the Compose/setup editor is rejected. Use this
reviewed removal flow first; the editor and public setup API cannot silently
remove service membership from active management.

Read-only servers allow local archiving but not container removal. Runtime removal is blocked for projects that host a shared database source. An app whose recipe declares an external lifecycle, such as Nextcloud AIO, must use its own administration controls for runtime removal; Selfhost can archive its record. Removing a single app is blocked when other Compose services explicitly depend on it or when the project has a managed database connection. Resolve the connection or review removal of the whole project instead. Dependencies inside application-specific settings cannot all be detected automatically; review these connections before removal.

## Choose a backup

The default is a **configuration snapshot**. It contains the project definition, environment variables and managed configuration files. It does not contain application files in volumes, database contents, or container writable layers.

For a provisioned supported PostgreSQL connection, **configuration and database** also creates a verified archive of that project's connected database. It does not include application files or other databases. Use each application's own backup process when a complete application recovery copy is needed.

**Continue without a new backup** requires a separate acknowledgement. Existing snapshots, backups and project files are still retained. A private removal record is kept for recovery; that record is not a substitute for an independent data backup.

If a selected backup fails, Selfhost stops before removing containers or changing active management. Plans expire after ten minutes. Configuration changes, server changes, or a changed container inventory invalidate the review.

## Recover an archived project

The archive list retains whole-project removal records. Restore requires the exact project name. Restoring reattaches the retained project configuration to Selfhost and leaves schedules disabled. It does not start containers or deploy the project. Conflicting project IDs and known port allocations are rejected.

An individually removed app can be recovered by restoring its configuration snapshot from the existing project's snapshot controls. Review the full snapshot before restoring because it contains the entire project's configuration. Starting the project is a separate action.

If removal is interrupted, its archive may show `runtime_pending` or `state_pending`. Further project mutations are blocked. Use **Reconcile removal** with the exact project name to inspect its runtime and recover the pre-removal configuration with schedules disabled. Reconciliation does not start, stop or delete containers. Containers already removed are not recreated. Review the recovered project before starting anything again.

If the Docker server is unavailable, runtime reconciliation waits until it can inspect that server. A partially completed removal is not automatically retried.

## Command line

Use the same Selfhost data directory for planning and applying. First review the plan:

```sh
selfhost removal plan PROJECT_ID
selfhost removal plan PROJECT_ID --service SERVICE_ID --mode remove_containers --backup configuration
```

The response includes a plan ID, revision, exact confirmation and removal scope. Only apply the values from that review:

```sh
selfhost removal apply PROJECT_ID --plan-id PLAN_ID --revision REVISION --confirm "Exact displayed name" --acknowledge-preserved-data
```

If the plan selected `--backup none`, the apply command also requires `--acknowledge-no-backup`. To include the supported connected PostgreSQL database, plan with `--backup configuration_and_database`.

```sh
selfhost removal archives
selfhost removal restore ARCHIVE_ID --confirm "Exact project name"
selfhost removal reconcile ARCHIVE_ID --confirm "Exact project name"
```

These commands restore or recover configuration only. They do not start containers.

## API

All removal endpoints require an authenticated administrator and the same origin checks as other mutations.

1. `POST /api/projects/{id}/removal/plan` with `service` (optional), `mode` (`archive` or `remove_containers`), and `backup` (`configuration`, `configuration_and_database`, or `none`).
2. Read the response's scope, warnings, exact container list and required confirmation.
3. `POST /api/projects/{id}/removal/apply` with `plan_id`, `revision`, `confirmation`, `acknowledge_preserved_data: true`, and `acknowledge_no_backup: true` when no backup was selected.

List archives with `GET /api/removal/archives`. Restore whole-project configuration using `POST /api/removal/archives/{id}/restore`; reconcile an interrupted removal with `POST /api/removal/archives/{id}/reconcile`. Both take `{"confirmation":"Exact project name"}`.

Plans and recovery records stay in the private Selfhost data directory. They are not designed as a long-term external backup. Preserve that directory separately alongside application data backups.

Container removal first requests a stop with a 60-second grace period, then removes the exact stopped container IDs without force or volume flags. A container that restarts outside Selfhost causes removal to fail safely. See [Docker's container removal reference](https://docs.docker.com/reference/cli/docker/container/rm/).
