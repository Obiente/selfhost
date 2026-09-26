# Selfhost

Set up an app once, return to the CLI when needed, or use an optional dashboard.
The package includes the native CLI, dashboard and service catalog. Node.js 22+
is required; Rust and a repository checkout are not.

```sh
npx selfhost --help
pnpm dlx selfhost --help
pnpx selfhost --help
```

Choose one launcher. Docker with Compose is needed when you start container
services. To generate an independent app in a new directory:

```sh
npx selfhost app --directory ./cloud init nextcloud
npx selfhost app --directory ./cloud start
```

Close Selfhost after the command finishes. The app keeps running. Its `compose.yaml`,
`.env` and app configuration files work with ordinary Docker Compose. You can
return with `selfhost app --directory ./cloud update` or use supported app actions
and identity connections without a dashboard or Selfhost account.

To install globally:

```sh
npm install --global selfhost
selfhost --help
```

Or use `pnpm add --global selfhost`. Run `selfhost tui` for guided terminal
management and `selfhost --help` for all commands. Append an exact package version
to pin a release, for example `npx selfhost@0.1.1 --help`.

If you want the dashboard, run `selfhost serve` and open the one-use local sign-in
link printed in your terminal.

Native executables are included for Windows, Linux and macOS on x64 and ARM64.
No install scripts or install-time binary downloads are used. The launcher checks
the selected binary's SHA-256 checksum and preserves arguments, working directory,
stdin/stdout/stderr and exit codes.

`SELFHOST_BINARY` can explicitly select a trusted native executable. A native
command already on `PATH` is a development fallback when no bundled target
exists. Script shims are rejected and corrupt bundled files fail closed.

From an existing identity-provider directory:

```sh
npx selfhost identity inspect . --selfhost-url http://localhost:8372
```

Inspection reads supported configuration without altering it. Follow the reviewed
setup flow to connect sign-in. Keep credentials out of command arguments and
shared logs.

Source and guides: [obiente/selfhost](https://github.com/obiente/selfhost).
Licensed AGPL-3.0-or-later. Your services remain portable through ordinary Docker
Compose, environment files and exported configuration.
