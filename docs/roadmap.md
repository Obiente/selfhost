# Product direction

selfhost belongs under Obiente. It helps beginners start self-hosting while giving experienced operators direct access to their configuration. The local dashboard, terminal interface, and CLI are peers over the same core.

## First working slice

Project creation, declarative app recipes, local Compose execution, configuration editing, action history, configuration snapshots and restore, interval schedules, and declarative dashboard app-link sync. Homarr is the first dashboard recipe. No public exposure is required.

## Service recipe system

Implemented first native integration slice: typed configuration, revision checks, verified writes and restore, managed Nextcloud workflows, scheduled app actions, ZITADEL YAML editing, and declarative OIDC client creation with resumable app configuration. See [app integrations](app-integrations.md) for verification and limits. Next: effective runtime inspection, version negotiation, complete IDP deployment recipes, and browser-tested identity provisioning.

- Versioned manifests with schema validation and contributor documentation.
- Multi-container applications, dependency ordering, multiple persistent volumes, and typed user inputs.
- Explicit plans and project lockfiles before applying changes.
- Recipe migrations that preserve volume ownership, secrets, and existing user configuration.
- Upstream API schema tracking and contract fixtures for integrations.
- Contributor validation in CI, including container startup and persistence checks.

## Beginner workflows

- Start from the goal, such as a photo library, DNS filtering, or a personal dashboard.
- Show the storage, networking, and resource implications before installation.
- Guided onboarding and actionable failures with a recovery path.
- Import existing Compose projects without taking ownership implicitly.

## Homarr integration

- Detect supported API capabilities from the running instance.
- Configure apps and their integrations deterministically.
- Create boards and place apps and widgets through versioned recipes.
- Retry safely and reconcile existing resources without duplicates.
- Support the upcoming Homarr version through its published API contract.

## Reliable operations

- Consistent application-data backups, including database-specific quiesce/export steps.
- Verify archives and rehearse restore into an isolated project.
- Backup retention, remote backup targets, and storage estimates.
- Version discovery, update review, health verification, and rollback.
- Durable job recovery, maintenance windows, and an installable background service.
- Opt-in external notifications for failures and completed jobs.

## More hosts

- Implemented: SSH Docker and existing Docker contexts, Proxmox SSH inventory, server groups, per-project placement and read-only connections.
- Implemented: standalone Docker cold data transfer with verification/recovery and native offline same-cluster Proxmox migration requests. See infrastructure.md for verification and limits.
- Next: cross-cluster migration with storage/network mappings, Swarm workload placement, Kubernetes, and live guest migration.
- Explicit capability detection for Docker, Podman, Proxmox, and Incus adapters.
- Reusable declarative recipes with provider-specific implementations in the core.
- Authenticated remote dashboard access, separate from exposing hosted apps.
