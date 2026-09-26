# Servers, clusters and moves

## Connections

Open **Infrastructure** or run `selfhost server-add` to save a connection. The guided CLI asks for the transport, offers inline dedicated SSH key setup or an existing alias, collects an optional app hostname and permissions, then checks inventory. Connections default to read-only. No request file is required.

| Connection              | Discovery                                                         | Project execution                                  |
| ----------------------- | ----------------------------------------------------------------- | -------------------------------------------------- |
| Local Docker            | Containers and Swarm membership                                   | Compose, stats, logs and recipe actions            |
| Docker over SSH         | Containers, Swarm nodes and services when connected to a manager  | Compose on the selected engine                     |
| Existing Docker context | Uses the endpoint and authentication already configured in Docker | Compose on that context                            |
| Proxmox over SSH        | Cluster nodes, QEMU VMs, LXCs and storage inventory               | Native offline guest migration within that cluster |

SSH aliases resolve through the user's OpenSSH configuration. Existing keys remain in OpenSSH. Selfhost can also create a dedicated key through its [SSH connection setup](ssh-keys.md). Verify host trust before connecting. Proxmox connections need permission to run `pvesh` on a cluster node. Docker inside a VM or LXC is a separate Docker endpoint using that guest's SSH alias.

Server groups organize connections; they do not create a cluster. Swarm inventory reflects the real orchestrator. Compose deployment does not create a Swarm service, reschedule a Swarm task, or deploy Kubernetes resources.

Read-only is enforced in the backend for Docker mutations and Proxmox migration. Editing a connection can change its name, group, app hostname and read-only setting. Its transport and endpoint remain immutable so an existing project's destination cannot silently change.

Projects store `server_id`. Old state defaults to the local server. Port allocation is scoped to the server. Every remote command explicitly selects its endpoint, including scheduled actions, custom actions, stats and logs. The local engine endpoint is resolved once from a local Docker context and pinned for that process. A remote current context is rejected for the local connection.

App links are shown for remote projects only when network access and an app hostname are configured. A loopback-bound remote app requires a tunnel. Dashboard API synchronization currently remains local-only to avoid sending credentials to an unverified remote endpoint.

## Docker data moves

Select a project and destination, then **Check move**. Undeployed projects can be reassigned without transferring data. A deployed project uses a persisted migration job:

1. Check source/destination engine identities, platform, read-only policy, existing destination resources and configured port conflicts.
2. Lock the project against edits, schedules and service actions, and save its configuration.
3. Transfer exact source images using Docker archives and verify image IDs.
4. Stop source services. Reject a forced-kill or OOM shutdown.
5. Archive each manifest-declared volume through stopped helper containers. Preserve ownership and permissions; compare SHA-256 content/metadata manifests after import.
6. Create destination containers with exact source image IDs, start only services that were running on the source, and wait for Docker/Compose health. Previously stopped services remain stopped. Images without a healthcheck are verified as running, not application-ready.
7. Switch project placement only after success. Retain source containers and volumes, stopped, for recovery. Restore configured image tags for subsequent intentional updates.

Archives are streamed to private local temporary storage, not returned through JSON. Each image or volume archive is capped at 20 GiB. Source and verification archives can coexist, so allow temporary space for both. Partial archives are retained after a failure for inspection. Successful transfer archives are removed.

The current adapter handles same-platform, standalone Docker engines and the catalog's single declared local volume per service. It rejects volume plugins, bind mounts, undeclared mounts and special files. External databases, distributed consistency, custom backup hooks, port remapping, DNS/proxy cutover, cross-platform image conversion and Swarm rescheduling need additional adapters. Docker itself rejects occupied destination ports during startup; preflight cannot reserve ports against other tools.

Failure leaves the project locked with its source placement intact. **Recover source** first stops the destination, removes migration-owned helper containers, and restarts only the source services that were previously running. Destination data is retained and must be deliberately removed before retrying. A crashed process leaves the same persisted guard and job available for recovery. Do not operate on the project's resources through another tool during transfer.

The commit boundary is deliberately conservative: if placement has already changed to the destination, source recovery refuses to restart it. Inspect the destination and migration journal before resolving an interruption at that boundary. A completed move has no automatic reverse-migration action.

## Proxmox guest moves

Choose **Move** on a VM or LXC, select another node, and check the plan. The current flow requires a stopped guest, an online target in the same cluster, matching storage IDs and network bridges, and no lock or known passthrough/bind-mount constraint. Proxmox performs its own final disk-capacity and compatibility checks.

Execution calls the native migration API through `pvesh`, saves the returned UPID task receipt locally, and polls task status. A submitted task is not reported as completed until Proxmox returns `status=stopped` and `exitstatus=OK`. Closing the dashboard does not cancel the native task. Reopen Infrastructure to retrieve its saved receipt.

Live migration, cross-cluster guest migration, storage/bridge remapping, migration cancellation and starting a guest after the move are not enabled by this flow. It does not delete or recreate guests itself.

## Verification

The opt-in `disposable_docker_move_and_recovery` test requires an explicitly created local `selfhost-migration-test` Docker context pointing to a separate disposable daemon. It creates its own synthetic project, tests real running and stopped service moves, triggers a special-file failure, verifies the persisted guard and source recovery, and removes its own project resources. It does not run as part of ordinary `cargo test`.

Normal tests cover state compatibility, SSH input validation, backend read-only enforcement, archive content/permission verification and Proxmox storage/network/passthrough checks. Live Proxmox migration still requires testing on a non-production guest.

## References

- [Docker contexts](https://docs.docker.com/engine/manage-resources/contexts/)
- [Docker SSH connections](https://docs.docker.com/engine/security/protect-access/)
- [Docker volumes](https://docs.docker.com/engine/storage/volumes/)
- [Proxmox administration guide](https://pve.proxmox.com/pve-docs/pve-admin-guide.pdf)
