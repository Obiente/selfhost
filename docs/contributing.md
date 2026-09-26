# Contribute to Selfhost

Selfhost's catalog and integrations are configuration-driven. A contributor can
add a recipe, deployment method or declared action without adding an app-name
branch to the CLI.

## Run the project

```sh
npm ci --prefix ui
npm run build --prefix ui
cargo run -- serve
```

Use a separate `--data-dir` for experiments. Test changes with disposable services,
never production resources.

## Choose the right configuration

| What you are adding                        | Location                          |
| ------------------------------------------ | --------------------------------- |
| Single-service recipe                      | `catalog/*.toml`                  |
| Native app settings and actions            | `catalog/integrations/`           |
| Multi-service stack                        | `catalog/stacks/`                 |
| Deployment-method choices                  | `catalog/deployments/`            |
| Existing-app image recognition and actions | `catalog/existing/`               |
| Identity directory recognition             | `catalog/identity-directories/`   |
| Identity client registration               | `catalog/identity-registration/`  |
| Reverse-proxy provider                     | `catalog/infrastructure/proxies/` |

Check neighboring files for the actual schema and use the current CLI's validation
commands. Keep configuration specific to Selfhost. Do not copy an external catalog
into the dashboard or link installation flows to another platform's templates.

## Recipe expectations

- Use explicit images, ports, storage and health checks.
- Keep secrets generated or supplied privately, never committed defaults.
- Describe Docker socket access, reserved names and other host permissions.
- Separate read operations from actions that change a service.
- Define input types and bounds for native configuration and actions.
- Preserve standard Compose/env/native formats so the user can leave Selfhost.
- Test initial setup, recreation, persistent data, wrong-target rejection and
  relevant read-only restrictions.

Do not claim that an integration has been tested against a provider or deployment
method unless that exact path has been exercised.

## Vue interface

Use reusable components under `ui/src/components` and composables under
`ui/src/composables`. Reka UI provides interaction primitives; Selfhost owns their
styling. Use `BaseDialog` for ordinary dialogs and an AlertDialog for consequential
confirmation. Prefer visible labels, native form controls and clear empty states.

All dashboard API requests should use `sessionFetch` or the supplied API prop.
Do not send session credentials to app URLs. Do not store complete bearer tokens
in browser storage.

## Check your changes

```sh
npm run build --prefix ui
cargo test
cargo clippy --all-targets -- -D warnings
npm run build --prefix site
```

Use actual browser, keyboard and narrow-screen checks for UI flows. Keep screenshots,
diagnostics, credentials and private paths out of commits. Share only deliberately
reviewed product screenshots. The project uses AGPL-3.0-or-later.
