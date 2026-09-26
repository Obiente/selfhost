# Guided setup

The guided commands in version 0.1.4 ask for the information they need, show
the changes, and carry out the approved steps. You do not need to create request
files, find internal IDs or copy review revisions between commands.

Start with:

```sh
selfhost guide
```

Choose an app, existing service, server, database, proxy or recurring task. The
CLI uses the same application profiles and validation as the dashboard. Secret
prompts hide input. A review does not grant permission until you confirm it.

## Go directly to a task

| Task                                               | Command                                                     |
| -------------------------------------------------- | ----------------------------------------------------------- |
| Choose an app and deployment method                | `selfhost deploy`                                           |
| Install a multi-service stack                      | `selfhost stack-create`                                     |
| Set up an independent app directory                | `selfhost app --directory ./my-app init APP`                |
| Connect a server                                   | `selfhost server-add`                                       |
| Discover and link an existing app                  | `selfhost existing link`                                    |
| Reconnect an app after its container was replaced  | `selfhost existing reconnect`                               |
| Set up a project's database                        | `selfhost database-setup`                                   |
| Connect, edit or remove a database source          | `selfhost database-add`, `database-edit`, `database-remove` |
| Connect or edit a reverse proxy                    | `selfhost proxy-add`, `proxy-edit`                          |
| Publish an app through a saved proxy               | `selfhost route-setup`                                      |
| Register an existing private network               | `selfhost network-add`                                      |
| Create a recurring task                            | `selfhost task create`                                      |
| Install and start the background dashboard         | `selfhost dashboard setup`                                  |
| Connect Selfhost login from the provider directory | `selfhost identity setup`                                   |

App installation asks about supported database choices before the first start,
then offers first-run setup and identity integration where its profile supports
them. You may stop after creating the ordinary configuration files.

## Return to an app

For a managed project:

```sh
selfhost app-setup PROJECT SERVICE
selfhost app-connect PROJECT SERVICE
selfhost app-config PROJECT SERVICE --edit
selfhost app-action PROJECT SERVICE ACTION
selfhost app-sync PROJECT SERVICE
selfhost app-update PROJECT
```

Independent Compose directories use the equivalent `selfhost app --directory DIR`
commands. They do not need a dashboard or a persistent Selfhost process. See the
[standalone guide](standalone.md).

Database credentials, host trust and administrator access are reviewed explicitly.
A container replacement clears earlier management permissions so a different
workload cannot silently inherit them. Declining an operation before its approval
does not perform that operation. If a later step fails after an approved change,
the CLI reports what remains saved so you can resume or recover.

## Automation and advanced configuration

Explicit request files and separate plan/apply commands remain available for
scripts. Their revision checks protect against changes since review. Guided
commands carry these revisions internally and fail if the reviewed state changes.

Use typed settings and list controls for supported app configuration. Compose and
native configuration editors remain available when you intentionally need full
control. Your files and running applications remain usable without Selfhost.

Provider capabilities still matter: automatic client registration needs a matching
provider profile. Registering a private network records an existing network; it
does not install peers or change firewall rules. Replacing stored database
credentials verifies credentials already changed at the database; it does not
silently change a database account password.
