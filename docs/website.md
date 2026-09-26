# Website and documentation development

The public website and guides use VitePress and Vue. The documentation source is
the repository's `docs/` directory, so technical guides have one maintained copy.
The homepage, navigation and theme live under `site/.vitepress/`.

Every catalogue entry has a generated `/apps/APP_ID` guide. Its deployment selector,
inputs, setup fields, native settings and actions come from the same contributor
profiles used by the CLI. Each selected deployment lists only the profiles attached
to its services. Readers can switch between dashboard and standalone CLI instructions,
and choose their CLI installation method using the documentation selector.

Add app-specific advice in `catalog/guides/APP_ID.json` using arrays named `setup`,
`integration_notes` and `limitations`. Keep credentials and installation-specific
values out of these public files. JSON profiles are executable configuration;
the public guide generator exports an explicit whitelist of documentation fields.

App icons use each recipe's `icon`; a stack can declare its own `icon`. Check every
URL with `node scripts/check-catalogue-icons.mjs`. The website and dashboard share
an image component that falls back to the bundled Selfhost mark on load failure.

```sh
npm ci
cd site
npm ci
npm run dev
npm run build
npm run preview
```

The development server binds to loopback. VitePress preview listens on all interfaces;
use it only on a trusted development machine. The build produces static
files under `site/.vitepress/dist`. Host those files with a static web server and
HTTPS. The website does not need access to the management API or its credentials.

Before deployment, choose the public hostname and any base path, verify navigation
and search, then review the generated files. A successful local build is not a
public deployment. No tracking, analytics or external account is required.

## Deploy on Obiente Cloud

Create a Git-backed application with these settings:

| Setting                                            | Value                                                    |
| -------------------------------------------------- | -------------------------------------------------------- |
| Repository                                         | `Obiente/selfhost`                                       |
| Branch                                             | `main`                                                   |
| Build type                                         | Dockerfile                                               |
| Build context                                      | Repository root (`.` or `/`, depending on the interface) |
| Dockerfile path                                    | `site/Dockerfile`                                        |
| Container port                                     | `8080`                                                   |
| Domain                                             | `selfhost.obiente.org`                                   |
| HTTPS                                              | Enable at the Obiente Cloud proxy                        |
| Health check                                       | HTTP `GET /` on port `8080`                              |
| Environment variables, secrets, persistent volumes | None required                                            |

Point the domain's DNS record at your Obiente Cloud ingress, then deploy. The proxy
terminates HTTPS and forwards HTTP to the container's port 8080. The site is built
with Node.js and served by Caddy as a non-root user. The running container contains
only the static website and needs no Docker socket, database or Selfhost API access.

The build context must include the repository root: catalogue generation reads
`catalog/`, shared Vue components and the root build dependencies. The Dockerfile's
ignore file excludes local data, existing builds and blog drafts. Only the generated
website is copied into the runtime image. `/blog/` remains unavailable.

Caddy resolves clean URLs such as `/getting-started` and `/apps/nextcloud` to their
generated HTML pages, including on a direct visit or refresh. Unknown URLs return
the generated 404 page with HTTP status 404. Hashed assets can be cached for a year.

For a local container check:

```sh
docker build -f site/Dockerfile -t selfhost-site .
docker run --rm -p 127.0.0.1:8080:8080 selfhost-site
```

After deployment, verify HTTPS, a direct visit to `/apps/nextcloud`, search and the
CLI selector. Confirm `/blog/` and an unknown URL return 404. The sitemap uses
`https://selfhost.obiente.org`; update `site/.vitepress/config.mts` if that changes.
This configuration expects the site at the domain root.

## Public catalogue

The [full catalogue](/catalogue) is generated before website development and builds
from `catalog/*.toml` and the deployment, integration, onboarding, existing-app,
stack and version profiles. There is no separate list to maintain. Search, category
filters and support tags use this generated data. New contributor recipes appear
on the next build.

Tags describe declared Selfhost capabilities. Tested-version notes are separate
and must name the checks actually performed; adding an image or an integration
does not imply that every feature has been tested. The generator exports public
metadata only, without environment values, credentials or executable setup steps.

Run `node --test scripts/catalogue-data.test.mjs` to check catalogue completeness
and capability mapping. Generated JSON under `site/.vitepress/generated/` is ignored.

## Command reference

Build the CLI, then generate its command reference:

```sh
cargo build
node scripts/generate-cli-reference.mjs
```

The generator uses the local build by default. `SELFHOST_BINARY` can select another
executable. Regenerate the reference when commands or options change.

## CLI method selector

Every guide offers an installed CLI, npx, pnpm dlx or pnpx selector. Vue shares the preference
across pages and remembers it locally. Markdown command blocks, inline commands
and generated usage lines adapt together; copied code uses the visible method.
JSON configuration, paths and installation commands remain literal. With
JavaScript disabled, the static pages show installed-CLI examples.

Command transformations live in `site/.vitepress/cli-markdown.mjs` and are tested
with `node --test site/.vitepress/cli-markdown.test.mjs`.
