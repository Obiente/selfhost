# Tasks and triggers

Tasks are optional. They can keep a configured dashboard's links current or repeat an eligible app action. They do not require a permanently running Selfhost dashboard: run them from the CLI whenever you want, or invoke `selfhost task tick` from your own scheduler.

While `selfhost serve` is running, it checks for due tasks. A dashboard link task compares current projects and linked existing applications with its private onboarding receipt. Newly added sources are picked up on its next interval. This is periodic reconciliation, not an instant event delivery system.

## Keep dashboard links current

Complete [app onboarding](./app-onboarding.md) first. Only profiles that declare a `sync` mode appear as link destinations. Homarr supports this flow.

In the dashboard or TUI, choose the destination and source scope, review the proposed links, and enable the task. Empty source ID lists mean **all current and future** sources of that selected kind. Select explicit IDs to restrict access to a fixed set.

From the CLI, list available destinations:

```sh
selfhost task destinations
```

Save a request as `dashboard-links.json`, substituting the destination project ID:

```json
{
  "name": "Keep dashboard links current",
  "kind": "dashboard_links",
  "destination": {
    "project_id": "p-example",
    "service": "homarr"
  },
  "managed": true,
  "existing": true,
  "project_ids": [],
  "existing_ids": [],
  "interval_seconds": 300
}
```

Review and create it using the revision returned by the plan:

```sh
selfhost task plan dashboard-links.json
selfhost task create dashboard-links.json --revision REVIEWED_REVISION
selfhost task list
selfhost task run TASK_ID
```

Creation saves an enabled rule and schedules its first reconciliation. If Selfhost's server is running, it may execute that rule on its next scheduler pass. It uses the private API credential and board created during onboarding; no token belongs in a task request.

### Source addresses

Managed services use their server's browser hostname and published port. Remote services bound only to loopback and remote addresses recorded as `localhost` are skipped. Linked existing apps use their recorded HTTP or HTTPS address. URLs with embedded credentials, query strings, or fragments are excluded.

Localhost links are useful when browsing from the same computer. Private network addresses require network access from the user's browser. Selfhost does not automatically expose services publicly or configure proxies as a side effect of adding a link. Check the reviewed addresses before enabling broad source scopes.

### What reconciliation changes

- Adds missing URLs, up to 100 per run; later intervals handle the rest.
- Deduplicates against completed onboarding links, including a URL's equivalent trailing slash.
- Leaves existing board items, names and layout alone.
- Does not remove links when a source project is deleted. Remove stale links in the destination app.
- Does not inspect, start or modify source applications. A link can therefore be added before its source app is started.

## App action tasks

Catalogue authors define actions in an app's native TOML recipe. An action with `confirm = false` can be selected for recurring execution after review. Actions requiring interactive confirmation, arbitrary shell commands and lifecycle operations such as update or delete are not eligible.

For example, a recipe can declare a read-only health or configuration check:

```toml
[[actions]]
id = "check-configuration"
label = "Check configuration"
description = "Validate this application's configuration without changing it."
command = ["app", "config", "check"]
timeout_seconds = 30
confirm = false
```

Use the app's actual executable and arguments. This example is a schema illustration, not a command supported by every image. Mark mutating or sensitive commands with `confirm = true`; keep those interactive.

An action task request looks like:

```json
{
  "name": "Validate application configuration",
  "kind": "service_action",
  "destination": {
    "project_id": "p-example",
    "service": "example"
  },
  "action": "check-configuration",
  "interval_seconds": 3600
}
```

Use the same `task plan` and `task create` workflow. `task destinations` lists eligible actions. The task records success or failure without retaining command output, which can contain application secrets.

## Pause, retry and remove

```sh
selfhost task disable TASK_ID
selfhost task enable TASK_ID
selfhost task tick
selfhost task remove TASK_ID --confirmation "Keep dashboard links current"
```

The last 100 runs are retained per task. Removing a rule stops scheduling; it preserves app links and onboarding receipts. It does not delete projects or application data.

A task pins the reviewed destination's container, image, app profile, server policy and private onboarding identity. Updating or recreating the destination container requires a new task review. A source changing or disappearing does not transfer authorization to a different destination.

Failures before dispatch disable the rule as **blocked**. Resolve the cause, then enable it to retry. Read-only servers, removed projects and unfinished migrations cannot be task destinations.

Before a remote write, Selfhost persists a dispatch record. If a request fails, times out, or Selfhost exits before saving completion, its outcome can be **uncertain**. It is not retried automatically, and enabling the rule cannot bypass that protection. Inspect the destination and its private onboarding receipt before recovery; preserve that receipt. Removing and recreating a task will not bypass an unfinished onboarding operation.

## CLI independence

Use an external scheduler to call `selfhost task tick` against the same `--data-dir` if you do not run the dashboard. A nonblocking process lock prevents overlapping task runners and rule changes. Tasks are workspace features; portable [standalone app directories](./standalone.md) continue to work with ordinary Docker Compose, with no task runner required.

Automatic catalogue version checks are separate from these user tasks. A version check never authorizes deployment changes or automatic app upgrades.
