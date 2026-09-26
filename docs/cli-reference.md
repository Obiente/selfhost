# Command reference

Generated from the current source executable. Run `selfhost COMMAND --help` to check your installed version.

## selfhost

```text
A home for the services you run

Usage: selfhost [OPTIONS] [COMMAND]

Commands:
  task                    Review and run automatic app tasks and service-link triggers
  app                     Set up and reuse apps in ordinary Compose directories, without a dashboard
  dashboard               Keep the dashboard running and configure access through a reverse proxy
  update                  Check for and install a reviewed Selfhost update
  deployments             List contributor-defined deployment methods
  deploy                  Create a stopped project using a reviewed deployment JSON file
  identity                Connect an identity provider or inspect its existing configuration directory
  existing                Link, inspect and grant specific actions on independently managed apps
  removal                 Review removal, choose backups and restore archived project records
  stacks                  List installable multi-service stacks
  stack-create            Create a portable stack from an installation JSON file, without starting it
  networking              List connected proxies, networks and provider capabilities
  proxy-add               Register a proxy from a private JSON file
  proxy-edit              Update a saved proxy connection from a private JSON file containing its ID
  network-add             Register an existing private network and its access-policy reference
  route-plan              Preview native proxy configuration for a route JSON file
  route-apply             Apply a reviewed route on a writable proxy
  route-probe             Test upstream HTTPS from the proxy host
  login-config            Configure Selfhost's own OIDC login from a private JSON file; omit file to show redacted settings
  app-backups             List saved native app configuration changes
  app-restore             Preview a native configuration restore, then apply with the returned revision
  app-setup               Inspect available automatic application setup and saved progress
  app-setup-plan          Preview declarative first-run setup or existing API-key connection
  app-setup-apply         Apply reviewed app setup; secrets may be supplied through environment variables
  app-connect-account     Find the authenticated human account before choosing an app administrator
  app-connect-plan        Review registration of an OIDC client from a connection JSON file
  app-connect             Create the reviewed client and configure the app; read the token from an environment variable
  app-connection          Read connection progress, or resume configuring its existing client
  app-config              Read the app-specific integration and typed settings
  app-plan                Preview typed settings from a JSON object of field ids and values
  app-apply               Apply a reviewed settings JSON file with the preview revision
  app-action              Run a declared app-management workflow; input JSON may contain secrets
  database-backup-export  Copy a verified database archive for use with its engine's native tools
  database-backup         Create a native database archive in the private project store
  database-backups        List database archives for a project
  database-restore        Restore a project archive after stopping its app containers; saves a safety backup
  databases               List database sources (credentials are omitted)
  database-engines        List database engines and their default connection ports
  database-add            Add a database source from a private JSON file
  database-start          Start a managed shared database server
  database-attach         Attach a dedicated, external or shared database to a project
  database-provision      Create this project's isolated database and role on its shared source
  create-setup            Create a custom setup from ordinary Compose YAML or JSON (does not start it)
  setup                   Read or replace a setup JSON document, including Compose, environment and files
  env                     Edit environment variables; values are read from stdin, never command arguments
  config                  Edit a mounted configuration file
  export                  Export standalone Compose, environment and config files (includes secrets, excludes volume data)
  serve                   Open the dashboard server (loopback by default)
  tui                     Browse and control projects in the terminal
  catalog                 Validate and list the service catalog
  versions                List recipe versions, optionally check its upstream release source
  version-plan            Review a change to the desired app image without changing running services
  version-apply           Save a reviewed app version; recreate the service separately when ready
  create                  Create a project without starting containers
  list                    List projects
  plan                    Print a project's Compose plan (custom inline values may contain secrets)
  run                     Start, stop, restart, refresh images, or snapshot a project
  schedule                Schedule configuration snapshots or image refreshes while serve is running
  restore                 Restore desired configuration from a snapshot; use start to apply it
  sync                    Add project apps to a dashboard using its manifest's API recipe
  doctor                  Check the Docker connection
  stats                   Show a service's current CPU, memory, network and disk I/O
  logs                    Show the last 200 lines of service output
  service                 Run start, stop, restart, update, or an action from the service recipe
  help                    Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
  -V, --version                    Print version
```

## task

```text
Review and run automatic app tasks and service-link triggers

Usage: selfhost task [OPTIONS] <COMMAND>

Commands:
  list          List task status and bounded history
  destinations  List available destination capabilities
  plan          Review an opt-in task request JSON file
  create        Save a reviewed task; no remote writes until its first run
  run           Run an enabled task once, without the dashboard
  tick          Run all enabled due tasks once, without the dashboard
  enable        Enable or retry a task blocked before dispatch
  disable
  remove        Remove scheduling, preserving app links and onboarding receipts
  help          Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task list

```text
List task status and bounded history

Usage: selfhost task list [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task destinations

```text
List available destination capabilities

Usage: selfhost task destinations [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task plan

```text
Review an opt-in task request JSON file

Usage: selfhost task plan [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task create

```text
Save a reviewed task; no remote writes until its first run

Usage: selfhost task create [OPTIONS] --revision <REVISION> <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task run

```text
Run an enabled task once, without the dashboard

Usage: selfhost task run [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task tick

```text
Run all enabled due tasks once, without the dashboard

Usage: selfhost task tick [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task enable

```text
Enable or retry a task blocked before dispatch

Usage: selfhost task enable [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task disable

```text
Usage: selfhost task disable [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### task remove

```text
Remove scheduling, preserving app links and onboarding receipts

Usage: selfhost task remove [OPTIONS] --confirmation <CONFIRMATION> <ID>

Arguments:
  <ID>

Options:
      --confirmation <CONFIRMATION>
      --data-dir <DATA_DIR>          Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>    Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                         Print help
```

## app

```text
Set up and reuse apps in ordinary Compose directories, without a dashboard

Usage: selfhost app [OPTIONS] <COMMAND>

Commands:
  init             Write a new private Compose directory; no dashboard, registration or Docker required
  start            Start the directory's current Compose configuration
  stop             Stop containers and retain every volume
  restart          Restart the directory's containers
  update           Snapshot configuration, pull configured tags and recreate as needed; does not back up app data
  status           Show Docker Compose status
  versions         List the recorded recipe's version choices
  version-plan     Review an image change without touching running containers
  version-apply    Save a reviewed image change; run start separately when ready to recreate
  logs             Show recent logs for a service
  actions          Show frozen recipe actions and integration workflows without running them
  action           Run a declared recipe action or app workflow
  config           Read supported native settings with secret fields omitted
  plan             Preview a JSON map of typed native app settings
  apply            Apply a reviewed native settings file, preserving user-owned Compose and env files
  setup            Inspect available automatic app setup and saved progress
  setup-plan       Preview first-run setup or an existing API-key connection
  setup-apply      Apply reviewed app setup without a dashboard or daemon
  connect-account  Find the authenticated human account before choosing an app administrator
  sync-plan        Review an explicit JSON array of new dashboard links using its saved API key
  sync             Add reviewed links to the saved private board without a daemon
  connect-plan     Preview an app's OIDC client registration from a JSON request
  connect          Create a reviewed OIDC client and configure the app
  connection       Show the connection receipt, or resume configuring its existing client
  backups          List native configuration backups for a service
  restore          Preview a native configuration restore; apply only with the returned revision
  help             Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app init

```text
Write a new private Compose directory; no dashboard, registration or Docker required

Usage: selfhost app init [OPTIONS] <APP>

Arguments:
  <APP>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --stack                      Use an installable stack ID instead of a single-container recipe
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --method <METHOD>            Contributor-defined deployment method, such as nextcloud's aio
      --inputs <INPUTS>            JSON map of stack input values
      --ack <ACK>                  Explicitly acknowledge a deployment requirement (repeat for each ID)
      --start                      Start after writing the files; otherwise review them first
      --image <IMAGE>              Explicit image tag/digest from the recipe repository
      --allow-untested             Acknowledge that a custom image has not been validated with this recipe
  -h, --help                       Print help
```

### app start

```text
Start the directory's current Compose configuration

Usage: selfhost app start [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app stop

```text
Stop containers and retain every volume

Usage: selfhost app stop [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app restart

```text
Restart the directory's containers

Usage: selfhost app restart [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app update

```text
Snapshot configuration, pull configured tags and recreate as needed; does not back up app data

Usage: selfhost app update [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app status

```text
Show Docker Compose status

Usage: selfhost app status [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app versions

```text
List the recorded recipe's version choices

Usage: selfhost app versions [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app version-plan

```text
Review an image change without touching running containers

Usage: selfhost app version-plan [OPTIONS] <SERVICE> <IMAGE>

Arguments:
  <SERVICE>
  <IMAGE>

Options:
      --allow-untested
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app version-apply

```text
Save a reviewed image change; run start separately when ready to recreate

Usage: selfhost app version-apply [OPTIONS] --revision <REVISION> <SERVICE> <IMAGE>

Arguments:
  <SERVICE>
  <IMAGE>

Options:
      --allow-untested
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --revision <REVISION>
  -h, --help                       Print help
```

### app logs

```text
Show recent logs for a service

Usage: selfhost app logs [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app actions

```text
Show frozen recipe actions and integration workflows without running them

Usage: selfhost app actions [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app action

```text
Run a declared recipe action or app workflow

Usage: selfhost app action [OPTIONS] <SERVICE> <ACTION>

Arguments:
  <SERVICE>
  <ACTION>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --inputs <INPUTS>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app config

```text
Read supported native settings with secret fields omitted

Usage: selfhost app config [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app plan

```text
Preview a JSON map of typed native app settings

Usage: selfhost app plan [OPTIONS] <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app apply

```text
Apply a reviewed native settings file, preserving user-owned Compose and env files

Usage: selfhost app apply [OPTIONS] --revision <REVISION> <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app setup

```text
Inspect available automatic app setup and saved progress

Usage: selfhost app setup [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app setup-plan

```text
Preview first-run setup or an existing API-key connection

Usage: selfhost app setup-plan [OPTIONS] <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app setup-apply

```text
Apply reviewed app setup without a dashboard or daemon

Usage: selfhost app setup-apply [OPTIONS] --revision <REVISION> <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app connect-account

```text
Find the authenticated human account before choosing an app administrator

Usage: selfhost app connect-account [OPTIONS] <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --token-env <TOKEN_ENV>      [default: SELFHOST_IDP_TOKEN]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app sync-plan

```text
Review an explicit JSON array of new dashboard links using its saved API key

Usage: selfhost app sync-plan [OPTIONS] <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app sync

```text
Add reviewed links to the saved private board without a daemon

Usage: selfhost app sync [OPTIONS] --revision <REVISION> <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app connect-plan

```text
Preview an app's OIDC client registration from a JSON request

Usage: selfhost app connect-plan [OPTIONS] <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app connect

```text
Create a reviewed OIDC client and configure the app

Usage: selfhost app connect [OPTIONS] --revision <REVISION> <SERVICE> <FILE>

Arguments:
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --token-env <TOKEN_ENV>      [default: SELFHOST_IDP_TOKEN]
  -h, --help                       Print help
```

### app connection

```text
Show the connection receipt, or resume configuring its existing client

Usage: selfhost app connection [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --resume
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app backups

```text
List native configuration backups for a service

Usage: selfhost app backups [OPTIONS] <SERVICE>

Arguments:
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### app restore

```text
Preview a native configuration restore; apply only with the returned revision

Usage: selfhost app restore [OPTIONS] <SERVICE> <BACKUP>

Arguments:
  <SERVICE>
  <BACKUP>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## dashboard

```text
Keep the dashboard running and configure access through a reverse proxy

Usage: selfhost dashboard [OPTIONS] <COMMAND>

Commands:
  install    Install a private copy as a user service (starts at sign-in); does not start it now
  start      Start the installed dashboard in the background
  stop       Stop only this workspace's background dashboard
  restart    Restart the user service and issue a new local sign-in link
  status     Inspect the user service and its private log location
  uninstall  Remove the user service, preserving projects, configuration and logs
  logs       Read private dashboard logs, including the local sign-in link
  domain     Review a domain and proxy instructions; apply an existing login configuration with its revision
  help       Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard install

```text
Install a private copy as a user service (starts at sign-in); does not start it now

Usage: selfhost dashboard install [OPTIONS]

Options:
      --bind <BIND>                Listener IP; use a private/VPN address for a remote proxy [default: 127.0.0.1]
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --port <PORT>                [default: 8372]
      --dry-run                    Show service definitions without installing anything
  -h, --help                       Print help
```

### dashboard start

```text
Start the installed dashboard in the background

Usage: selfhost dashboard start [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard stop

```text
Stop only this workspace's background dashboard

Usage: selfhost dashboard stop [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard restart

```text
Restart the user service and issue a new local sign-in link

Usage: selfhost dashboard restart [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard status

```text
Inspect the user service and its private log location

Usage: selfhost dashboard status [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard uninstall

```text
Remove the user service, preserving projects, configuration and logs

Usage: selfhost dashboard uninstall [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard logs

```text
Read private dashboard logs, including the local sign-in link

Usage: selfhost dashboard logs [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --lines <LINES>              [default: 50]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### dashboard domain

```text
Review a domain and proxy instructions; apply an existing login configuration with its revision

Usage: selfhost dashboard domain [OPTIONS] <URL>

Arguments:
  <URL>

Options:
      --bind <BIND>                Listener IP used in proxy examples; wildcard listeners need a reachable host IP instead [default: 127.0.0.1]
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --port <PORT>                [default: 8372]
      --revision <REVISION>
      --confirm-callbacks
  -h, --help                       Print help
```

## update

```text
Check for and install a reviewed Selfhost update

Usage: selfhost update [OPTIONS] [COMMAND]

Commands:
  check     Check the registry and show installation-specific instructions
  plan      Review the exact version, scope and update requirements
  stage     Build and verify a previously reviewed update without replacing this executable
  activate  Activate a verified update after this process exits
  recover   Recover an interrupted update using its verified previous executable
  jobs      Show staged updates and recovery information
  help      Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### update check

```text
Check the registry and show installation-specific instructions

Usage: selfhost update check [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### update plan

```text
Review the exact version, scope and update requirements

Usage: selfhost update plan [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### update stage

```text
Build and verify a previously reviewed update without replacing this executable

Usage: selfhost update stage [OPTIONS] --plan-id <PLAN_ID> --revision <REVISION> --confirm <CONFIRM>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --plan-id <PLAN_ID>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --revision <REVISION>
      --confirm <CONFIRM>
  -h, --help                       Print help
```

### update activate

```text
Activate a verified update after this process exits

Usage: selfhost update activate [OPTIONS] --job <JOB> --confirm <CONFIRM>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --job <JOB>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --confirm <CONFIRM>
  -h, --help                       Print help
```

### update recover

```text
Recover an interrupted update using its verified previous executable

Usage: selfhost update recover [OPTIONS] --job <JOB> --confirm <CONFIRM>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --job <JOB>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --confirm <CONFIRM>
  -h, --help                       Print help
```

### update jobs

```text
Show staged updates and recovery information

Usage: selfhost update jobs [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## deployments

```text
List contributor-defined deployment methods

Usage: selfhost deployments [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## deploy

```text
Create a stopped project using a reviewed deployment JSON file

Usage: selfhost deploy [OPTIONS] --file <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## identity

```text
Connect an identity provider or inspect its existing configuration directory

Usage: selfhost identity [OPTIONS] <COMMAND>

Commands:
  inspect           Read provider hints without executing scripts or printing credentials
  plan              Review a login manifest and its exact callback URLs
  apply             Save reviewed settings after provider callbacks are configured
  register-plan     Review automatic client registration on a supported provider
  register-account  Look up the credential's account for explicit administrator selection
  register          Register the reviewed client; provider credential is read from the environment
  help              Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### identity inspect

```text
Read provider hints without executing scripts or printing credentials

Usage: selfhost identity inspect [OPTIONS] [DIRECTORY]

Arguments:
  [DIRECTORY]  [default: .]

Options:
      --data-dir <DATA_DIR>          Directory for projects, secrets, and configuration snapshots
      --selfhost-url <SELFHOST_URL>  [default: http://localhost:8372]
      --catalog-dir <CATALOG_DIR>    Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                         Print help
```

### identity plan

```text
Review a login manifest and its exact callback URLs

Usage: selfhost identity plan [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --directory <DIRECTORY>      [default: .]
  -h, --help                       Print help
```

### identity apply

```text
Save reviewed settings after provider callbacks are configured

Usage: selfhost identity apply [OPTIONS] --revision <REVISION>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --directory <DIRECTORY>      [default: .]
      --revision <REVISION>
      --confirm-callbacks
  -h, --help                       Print help
```

### identity register-plan

```text
Review automatic client registration on a supported provider

Usage: selfhost identity register-plan [OPTIONS] --file <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### identity register-account

```text
Look up the credential's account for explicit administrator selection

Usage: selfhost identity register-account [OPTIONS] --file <FILE>

Options:
      --data-dir <DATA_DIR>
          Directory for projects, secrets, and configuration snapshots
      --file <FILE>

      --catalog-dir <CATALOG_DIR>
          Additional TOML service manifests; matching ids override recipes for new projects
      --credential-env <CREDENTIAL_ENV>
          [default: SELFHOST_IDP_TOKEN]
  -h, --help
          Print help
```

### identity register

```text
Register the reviewed client; provider credential is read from the environment

Usage: selfhost identity register [OPTIONS] --file <FILE> --revision <REVISION>

Options:
      --data-dir <DATA_DIR>
          Directory for projects, secrets, and configuration snapshots
      --file <FILE>

      --catalog-dir <CATALOG_DIR>
          Additional TOML service manifests; matching ids override recipes for new projects
      --revision <REVISION>

      --credential-env <CREDENTIAL_ENV>
          [default: SELFHOST_IDP_TOKEN]
  -h, --help
          Print help
```

## existing

```text
Link, inspect and grant specific actions on independently managed apps

Usage: selfhost existing [OPTIONS] <COMMAND>

Commands:
  list         List independently managed connections
  profiles     List supported connection profiles and their declared actions
  link         Save a read-only app connection from a JSON file
  inspect      Inspect the pinned container without making changes
  stats        Read resource usage for the pinned container
  permissions  Explicitly allow selected write actions using a reviewed JSON file
  action       Run a declared action; write actions require the exact saved app name
  unlink       Remove the connection record, preserving the actual app and all its data
  help         Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing list

```text
List independently managed connections

Usage: selfhost existing list [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing profiles

```text
List supported connection profiles and their declared actions

Usage: selfhost existing profiles [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing link

```text
Save a read-only app connection from a JSON file

Usage: selfhost existing link [OPTIONS] --file <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing inspect

```text
Inspect the pinned container without making changes

Usage: selfhost existing inspect [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing stats

```text
Read resource usage for the pinned container

Usage: selfhost existing stats [OPTIONS] <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing permissions

```text
Explicitly allow selected write actions using a reviewed JSON file

Usage: selfhost existing permissions [OPTIONS] --file <FILE> <ID>

Arguments:
  <ID>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing action

```text
Run a declared action; write actions require the exact saved app name

Usage: selfhost existing action [OPTIONS] <ID> <ACTION>

Arguments:
  <ID>
  <ACTION>

Options:
      --confirm <CONFIRM>          [default: ""]
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### existing unlink

```text
Remove the connection record, preserving the actual app and all its data

Usage: selfhost existing unlink [OPTIONS] --confirm <CONFIRM> <ID>

Arguments:
  <ID>

Options:
      --confirm <CONFIRM>
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## removal

```text
Review removal, choose backups and restore archived project records

Usage: selfhost removal [OPTIONS] <COMMAND>

Commands:
  plan       Review exact scope, resource ownership and backup coverage
  apply      Apply a fresh removal plan with explicit confirmations
  archives   List archived records and backup references
  reconcile  Reconcile an interrupted removal without starting or deleting containers
  restore    Restore an archived project record without starting containers
  help       Print this message or the help of the given subcommand(s)

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### removal plan

```text
Review exact scope, resource ownership and backup coverage

Usage: selfhost removal plan [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --service <SERVICE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --mode <MODE>                [default: archive] [possible values: archive, remove_containers]
      --backup <BACKUP>            [default: configuration] [possible values: configuration, configuration_and_database, none]
  -h, --help                       Print help
```

### removal apply

```text
Apply a fresh removal plan with explicit confirmations

Usage: selfhost removal apply [OPTIONS] --plan-id <PLAN_ID> --revision <REVISION> --confirm <CONFIRM> <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>         Directory for projects, secrets, and configuration snapshots
      --plan-id <PLAN_ID>
      --catalog-dir <CATALOG_DIR>   Additional TOML service manifests; matching ids override recipes for new projects
      --revision <REVISION>
      --confirm <CONFIRM>
      --acknowledge-preserved-data
      --acknowledge-no-backup
  -h, --help                        Print help
```

### removal archives

```text
List archived records and backup references

Usage: selfhost removal archives [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### removal reconcile

```text
Reconcile an interrupted removal without starting or deleting containers

Usage: selfhost removal reconcile [OPTIONS] --confirm <CONFIRM> <ARCHIVE>

Arguments:
  <ARCHIVE>

Options:
      --confirm <CONFIRM>
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

### removal restore

```text
Restore an archived project record without starting containers

Usage: selfhost removal restore [OPTIONS] --confirm <CONFIRM> <ARCHIVE>

Arguments:
  <ARCHIVE>

Options:
      --confirm <CONFIRM>
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## stacks

```text
List installable multi-service stacks

Usage: selfhost stacks [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## stack-create

```text
Create a portable stack from an installation JSON file, without starting it

Usage: selfhost stack-create [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## networking

```text
List connected proxies, networks and provider capabilities

Usage: selfhost networking [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## proxy-add

```text
Register a proxy from a private JSON file

Usage: selfhost proxy-add [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## proxy-edit

```text
Update a saved proxy connection from a private JSON file containing its ID

Usage: selfhost proxy-edit [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## network-add

```text
Register an existing private network and its access-policy reference

Usage: selfhost network-add [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## route-plan

```text
Preview native proxy configuration for a route JSON file

Usage: selfhost route-plan [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## route-apply

```text
Apply a reviewed route on a writable proxy

Usage: selfhost route-apply [OPTIONS] --revision <REVISION> <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## route-probe

```text
Test upstream HTTPS from the proxy host

Usage: selfhost route-probe [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## login-config

```text
Configure Selfhost's own OIDC login from a private JSON file; omit file to show redacted settings

Usage: selfhost login-config [OPTIONS] [FILE]

Arguments:
  [FILE]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-backups

```text
List saved native app configuration changes

Usage: selfhost app-backups [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-restore

```text
Preview a native configuration restore, then apply with the returned revision

Usage: selfhost app-restore [OPTIONS] <PROJECT> <SERVICE> <BACKUP>

Arguments:
  <PROJECT>
  <SERVICE>
  <BACKUP>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-setup

```text
Inspect available automatic application setup and saved progress

Usage: selfhost app-setup [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-setup-plan

```text
Preview declarative first-run setup or existing API-key connection

Usage: selfhost app-setup-plan [OPTIONS] <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-setup-apply

```text
Apply reviewed app setup; secrets may be supplied through environment variables

Usage: selfhost app-setup-apply [OPTIONS] --revision <REVISION> <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-connect-account

```text
Find the authenticated human account before choosing an app administrator

Usage: selfhost app-connect-account [OPTIONS] <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --token-env <TOKEN_ENV>      [default: SELFHOST_IDP_TOKEN]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-connect-plan

```text
Review registration of an OIDC client from a connection JSON file

Usage: selfhost app-connect-plan [OPTIONS] <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-connect

```text
Create the reviewed client and configure the app; read the token from an environment variable

Usage: selfhost app-connect [OPTIONS] --revision <REVISION> <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --token-env <TOKEN_ENV>      [default: SELFHOST_IDP_TOKEN]
  -h, --help                       Print help
```

## app-connection

```text
Read connection progress, or resume configuring its existing client

Usage: selfhost app-connection [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --resume
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-config

```text
Read the app-specific integration and typed settings

Usage: selfhost app-config [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-plan

```text
Preview typed settings from a JSON object of field ids and values

Usage: selfhost app-plan [OPTIONS] <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-apply

```text
Apply a reviewed settings JSON file with the preview revision

Usage: selfhost app-apply [OPTIONS] --revision <REVISION> <PROJECT> <SERVICE> <FILE>

Arguments:
  <PROJECT>
  <SERVICE>
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## app-action

```text
Run a declared app-management workflow; input JSON may contain secrets

Usage: selfhost app-action [OPTIONS] <PROJECT> <SERVICE> <ACTION>

Arguments:
  <PROJECT>
  <SERVICE>
  <ACTION>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --inputs <INPUTS>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-backup-export

```text
Copy a verified database archive for use with its engine's native tools

Usage: selfhost database-backup-export [OPTIONS] <PROJECT> <BACKUP> <OUTPUT>

Arguments:
  <PROJECT>
  <BACKUP>
  <OUTPUT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-backup

```text
Create a native database archive in the private project store

Usage: selfhost database-backup [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-backups

```text
List database archives for a project

Usage: selfhost database-backups [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-restore

```text
Restore a project archive after stopping its app containers; saves a safety backup

Usage: selfhost database-restore [OPTIONS] --confirm-project <CONFIRM_PROJECT> <PROJECT> <BACKUP>

Arguments:
  <PROJECT>
  <BACKUP>

Options:
      --confirm-project <CONFIRM_PROJECT>

      --data-dir <DATA_DIR>
          Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>
          Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help
          Print help
```

## databases

```text
List database sources (credentials are omitted)

Usage: selfhost databases [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-engines

```text
List database engines and their default connection ports

Usage: selfhost database-engines [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-add

```text
Add a database source from a private JSON file

Usage: selfhost database-add [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-start

```text
Start a managed shared database server

Usage: selfhost database-start [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## database-attach

```text
Attach a dedicated, external or shared database to a project

Usage: selfhost database-attach [OPTIONS] <PROJECT> <MODE>

Arguments:
  <PROJECT>
  <MODE>     [possible values: dedicated, external, shared]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --source <SOURCE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --engine <ENGINE>            Engine supported by the app recipe; defaults to its source or recipe
  -h, --help                       Print help
```

## database-provision

```text
Create this project's isolated database and role on its shared source

Usage: selfhost database-provision [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## create-setup

```text
Create a custom setup from ordinary Compose YAML or JSON (does not start it)

Usage: selfhost create-setup [OPTIONS] --compose <COMPOSE> <NAME>

Arguments:
  <NAME>

Options:
      --compose <COMPOSE>
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --environment <ENVIRONMENT>  JSON map of environment variable names to values
      --files <FILES>              Directory containing config files referenced as ./files/NAME
      --server <SERVER>            [default: local]
  -h, --help                       Print help
```

## setup

```text
Read or replace a setup JSON document, including Compose, environment and files

Usage: selfhost setup [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --reveal                     Explicitly include secret values in output
  -h, --help                       Print help
```

## env

```text
Edit environment variables; values are read from stdin, never command arguments

Usage: selfhost env [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --set <SET>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --unset <UNSET>
      --reveal
  -h, --help                       Print help
```

## config

```text
Edit a mounted configuration file

Usage: selfhost config [OPTIONS] <PROJECT> <NAME>

Arguments:
  <PROJECT>
  <NAME>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --file <FILE>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --remove
  -h, --help                       Print help
```

## export

```text
Export standalone Compose, environment and config files (includes secrets, excludes volume data)

Usage: selfhost export [OPTIONS] <PROJECT> <OUTPUT>

Arguments:
  <PROJECT>
  <OUTPUT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## serve

```text
Open the dashboard server (loopback by default)

Usage: selfhost serve [OPTIONS]

Options:
      --bind <BIND>                Listener IP; remote access requires a configured HTTPS origin and identity provider [default: 127.0.0.1]
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --port <PORT>                [default: 8372]
      --setup                      Temporarily allow one-use sign-in on a concrete private/VPN IP without an identity provider
  -h, --help                       Print help
```

## tui

```text
Browse and control projects in the terminal

Usage: selfhost tui [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## catalog

```text
Validate and list the service catalog

Usage: selfhost catalog [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## versions

```text
List recipe versions, optionally check its upstream release source

Usage: selfhost versions [OPTIONS] <APP>

Arguments:
  <APP>

Options:
      --check
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## version-plan

```text
Review a change to the desired app image without changing running services

Usage: selfhost version-plan [OPTIONS] <PROJECT> <SERVICE> <IMAGE>

Arguments:
  <PROJECT>
  <SERVICE>
  <IMAGE>

Options:
      --allow-untested
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## version-apply

```text
Save a reviewed app version; recreate the service separately when ready

Usage: selfhost version-apply [OPTIONS] --revision <REVISION> <PROJECT> <SERVICE> <IMAGE>

Arguments:
  <PROJECT>
  <SERVICE>
  <IMAGE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --revision <REVISION>
      --allow-untested
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## create

```text
Create a project without starting containers

Usage: selfhost create [OPTIONS] --apps <APPS> <NAME>

Arguments:
  <NAME>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --server <SERVER>            [default: local]
      --apps <APPS>
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --image <IMAGES>             Select an image as APP=IMAGE (repeat for multiple apps)
      --allow-untested             Acknowledge compatibility review for versions outside the recipe's tested set
  -h, --help                       Print help
```

## list

```text
List projects

Usage: selfhost list [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## plan

```text
Print a project's Compose plan (custom inline values may contain secrets)

Usage: selfhost plan [OPTIONS] <PROJECT>

Arguments:
  <PROJECT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## run

```text
Start, stop, restart, refresh images, or snapshot a project

Usage: selfhost run [OPTIONS] <PROJECT> <ACTION>

Arguments:
  <PROJECT>
  <ACTION>   [possible values: start, stop, restart, refresh, snapshot]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## schedule

```text
Schedule configuration snapshots or image refreshes while serve is running

Usage: selfhost schedule [OPTIONS] <PROJECT> <ACTION>

Arguments:
  <PROJECT>
  <ACTION>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --hours <HOURS>              [default: 24]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
      --disabled
  -h, --help                       Print help
```

## restore

```text
Restore desired configuration from a snapshot; use start to apply it

Usage: selfhost restore [OPTIONS] <SNAPSHOT>

Arguments:
  <SNAPSHOT>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## sync

```text
Add project apps to a dashboard using its manifest's API recipe

Usage: selfhost sync [OPTIONS] <PROJECT> <TARGET>

Arguments:
  <PROJECT>
  <TARGET>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --key-env <KEY_ENV>          [default: SELFHOST_DASHBOARD_API_KEY]
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## doctor

```text
Check the Docker connection

Usage: selfhost doctor [OPTIONS]

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## stats

```text
Show a service's current CPU, memory, network and disk I/O

Usage: selfhost stats [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## logs

```text
Show the last 200 lines of service output

Usage: selfhost logs [OPTIONS] <PROJECT> <SERVICE>

Arguments:
  <PROJECT>
  <SERVICE>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```

## service

```text
Run start, stop, restart, update, or an action from the service recipe

Usage: selfhost service [OPTIONS] <PROJECT> <SERVICE> <ACTION>

Arguments:
  <PROJECT>
  <SERVICE>
  <ACTION>

Options:
      --data-dir <DATA_DIR>        Directory for projects, secrets, and configuration snapshots
      --catalog-dir <CATALOG_DIR>  Additional TOML service manifests; matching ids override recipes for new projects
  -h, --help                       Print help
```
