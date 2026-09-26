# App versions and catalogue maintenance

An app version is an explicit container image tag or digest. Installing a new
Selfhost release does not change an existing app's image or frozen recipe.
Checking upstream releases also leaves deployments unchanged.

## Choose an app version

Recipe version choices can include a validation record describing what was tested.
The default image remains available even when the recipe has no separate version
record. A tested image does not imply that every architecture, database or identity
integration has been tested; read its notes.

For an independent directory, an explicit image can be selected at creation:

```sh
selfhost app --directory ./notifications init gotify --image gotify/server:3.1.1
```

A custom tag or full `sha256` digest must belong to the same image repository.
Unrecorded alternatives require `--allow-untested`. This acknowledges that native
configuration fields, setup APIs and upgrade paths may differ. It does not bypass
image syntax, repository or configuration validation. Use a custom Compose setup
when intentionally selecting a different image repository.

For an existing standalone app:

```sh
selfhost app --directory ./notifications versions gotify
selfhost app --directory ./notifications version-plan gotify gotify/server:YOUR_VERSION --allow-untested
selfhost app --directory ./notifications version-apply gotify gotify/server:YOUR_VERSION --allow-untested --revision REVIEWED_REVISION
```

The plan shows the old and selected images. Apply checks its revision and saves
the image choice while preserving Compose settings, environment, files and volume
declarations. It does not pull an image or change running containers. When ready,
run `selfhost app --directory ./notifications start` or ordinary Docker Compose to
recreate from the saved configuration. Take an application-data backup first and
check the app's upgrade instructions. Restoring an older image is not necessarily
a valid downgrade after database migrations.

Deployment methods and multi-service stacks may declare their own image inputs.
Use those method-specific choices; a generic image flag does not safely replace
all images in a stack.

## Contributor version profiles

Optional files under `catalog/versions/APP_ID.json` declare tested images and an
official upstream GitHub release source. Contributor catalog directories use the
same layout. A profile contains:

```json
{
  "schema": 1,
  "tested": [
    {
      "image": "example/app:2.4.1",
      "label": "2.4.1",
      "notes": "Describe the exact startup, actions, data and platforms verified."
    }
  ],
  "source": {
    "repository": "example/app",
    "tag_prefix": "v",
    "image_prefix": "",
    "stream": "2.4",
    "automatic": false,
    "smoke_path": "/health",
    "smoke_status": 200
  }
}
```

The update stream permits only stable patch versions within the explicit
`major.minor` pair. Pre-releases, rolling tags, unsupported tag formats and major
or minor changes need manual review. Set `automatic` only when the recipe has a
working disposable upgrade smoke path. Discovery can be enabled while automated
candidate preparation remains disabled.

## Check candidates locally

Install the repository's Node dependencies and authenticate the GitHub CLI for
public metadata access if required, then run:

```sh
node scripts/catalog-maintenance.mjs
node --test scripts/catalog-maintenance.test.mjs
```

Discovery uses the official [GitHub releases API](https://docs.github.com/en/rest/releases/releases#list-releases)
through `gh api`, reads at most 30 recent releases per source, bounds output and
request time, spaces sources one second apart and retries transient server errors
at most twice. Rate limits stop that source until a later run. Results are written
to ignored `.local/catalog-candidates.json`; no recipe or deployment is changed.
The dashboard/CLI backend uses the same declared release source with bounded
responses and a five-minute success cache.

To prepare updates locally, build the CLI and explicitly opt into disposable
Docker fixtures:

```sh
node scripts/catalog-maintenance.mjs --prepare --allow-docker-smoke --binary target/debug/selfhost
```

Preparation inspects the candidate image manifest, validates generated Compose,
starts the current recipe in a temporary project, writes a marker into its data
volume, upgrades that isolated project and verifies health and marker retention.
Declared non-confirming recipe actions are also exercised. The temporary project's
containers and volumes are removed afterward. No existing user project is selected.
Only passing candidates change the recipe image and add precise test notes to the
version profile. Release notes mentioning migrations, schema changes, breaking
changes or manual steps are withheld for manual review.

These gates are deliberately limited. A retained marker is not a complete database
migration test. Absence of migration keywords is not evidence that migrations are
unnecessary. Semantic versioning cannot guarantee application compatibility.

## Prepared GitHub workflow

`.github/workflows/catalog-updates.yml` prepares weekly draft pull requests on the
default branch after the same gates. Manual dispatch defaults to discovery only;
enable `prepare_pr` to request gated preparation. An existing open catalogue review
branch prevents a duplicate proposal. Reports are workflow artifacts, not committed
diagnostics. The workflow does not auto-merge, publish Selfhost or update installed
applications.

Maintainers must review upstream release notes, migration and rollback requirements,
profile compatibility and additional platforms before merging. Update stream
changes and new automation profiles are ordinary reviewed source changes. The
workflow is prepared in the repository; it has not been run or enabled remotely by
these local changes.
