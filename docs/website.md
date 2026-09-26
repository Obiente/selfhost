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
