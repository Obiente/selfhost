# Connect your identity provider

Open **Access** in the dashboard. **Connect an existing identity provider** comes first. Use **Host a new identity provider** if you want Selfhost to create a separate identity project instead.

## Use an existing client

1. Choose Selfhost's address. `http://localhost:8372` or `http://127.0.0.1:8372` works on your computer. Use the port printed by your running dashboard. A domain address requires HTTPS.
2. In your provider, create or select a Web OpenID Connect application using authorization code and PKCE S256. Register the exact displayed callback, for example `http://localhost:8372/auth/callback`. Avoid wildcard callbacks.
3. Copy the issuer URL, client ID and optional client secret into Selfhost. Name the connection as you want it to appear on the sign-in button.
4. Enter the exact OIDC subject IDs of users allowed to administer Selfhost. Subject IDs are stable provider identifiers, not email addresses. For ZITADEL and a default Keycloak realm, use the exact user ID. For authentik, check the provider's subject mode.
5. Review the connection, confirm the callback is registered, and save. Test sign-in while keeping your local recovery session open.

Advanced settings contain the connection ID and private CA certificates. HTTPS verification remains enabled. An empty client secret preserves an existing secret only when issuer, client ID and connection ID are unchanged.

All allowed users currently have full administrator access to the connected workspace. There is no automatic first-user administrator enrollment.

## Let Selfhost create the client

Choose **Set up automatically**. The available providers come from registration profiles; the initial profile supports ZITADEL's Application v2 API.

Supply your issuer and a temporary provider API token. Selfhost creates its own project named **Selfhost** by default and discovers the token's organization. ZITADEL requires `project.create` and `project.app.write` permissions. In advanced settings you can instead reuse an existing project ID, which only needs permission to create its application, or choose a different organization.

If you already linked a supported provider under **Existing apps**, select it with **Use linked provider** to fill in its address. Linking an app for management does not grant anyone permission to sign in to Selfhost; complete and review this separate sign-in setup.

Choose **Find my account** to read the token's current user, then explicitly select the displayed human account as a Selfhost administrator. Machine accounts are never suggested as human administrators. Alternatively enter exact administrator subject IDs in advanced settings. Review the project, client, administrator IDs and exact callback before applying.

Selfhost creates the dedicated project and one Web OIDC client, saves its returned client credentials privately and adds it to your login settings. The API token is used for these requests and is not saved. Loopback HTTP callbacks enable the client's development mode. No users, existing clients, or provider configuration files are changed.

A registration intent is saved before each creation API call. If the connection fails or the response cannot be understood, retries stop. Inspect the recorded project and application IDs at your provider before recovery. If client creation is explicitly rejected, correct the credential or its permissions and retry the same reviewed request: Selfhost reuses the recorded project. It never blindly deletes a created project. The private data directory contains `identity-registrations/<connection-id>.json` and, when received, `<connection-id>-response.json`. These can contain client credentials. Never publish them.

## Use the CLI in the provider directory

The commands are the same whether Selfhost was installed through Cargo or invoked through an available npm launcher. Check that your installed version contains `identity` commands; source changes do not publish new registry packages.

```sh
selfhost identity inspect . --selfhost-url http://localhost:8372
```

Inspection reads Compose documents, `.env`, and declared native provider files as data. ZITADEL's `zitadel.yaml` and `zitadel.yml` can supply an issuer from explicit external domain, port and security settings. Inspection identifies supported image names, reports safe issuer hints and provider instructions, and redacts secrets. Supported providers include a registration template with the remaining required fields. It does not run shell scripts, expand variables, traverse outside the directory, or modify the provider.

### Import a prepared connection

Download a template from **Access > Use the CLI**, or create `selfhost-login.json`:

```json
{
  "public_url": "http://localhost:8372",
  "providers": [
    {
      "id": "home",
      "name": "Home identity",
      "issuer": "https://identity.example.com",
      "client_id": "your-registered-client-id",
      "client_secret": "",
      "admin_subjects": ["your-exact-user-subject-id"]
    }
  ]
}
```

Templates downloaded from the dashboard omit secrets. Add any required secret privately on the machine where the CLI runs.

```sh
selfhost identity plan --directory .
selfhost identity apply --directory . --revision <reviewed-revision> --confirm-callbacks
```

Pass `--data-dir <your-private-data-directory>` before `identity` if your dashboard uses a custom data directory. Importing into a different machine's data directory does not configure a remote dashboard. You can also upload the prepared file in the dashboard and review it there.

### Register a client through the CLI

Create a private `identity-registration.json`:

```json
{
  "provider": "zitadel",
  "issuer": "https://identity.example.com",
  "create_project": true,
  "project_name": "Selfhost",
  "selfhost_url": "http://localhost:8372",
  "id": "home",
  "name": "Home identity",
  "admin_subjects": ["your-exact-user-subject-id"]
}
```

```sh
selfhost identity register-account --file identity-registration.json --credential-env SELFHOST_IDP_TOKEN
selfhost identity register-plan --file identity-registration.json
selfhost identity register --file identity-registration.json --revision <reviewed-revision> --credential-env SELFHOST_IDP_TOKEN
```

Set `SELFHOST_IDP_TOKEN` privately in the process environment using your shell or secret manager. Do not put tokens in command arguments, screenshots or chat. Review the plan against the intended provider before applying.

`register-account` is read-only. You can start with `"admin_subjects": []`, run it, review the returned `suggested_subject`, and explicitly put the intended human ID in `admin_subjects` before `register-plan`. A machine token produces no suggested administrator; enter a human user's exact subject ID instead. Account lookup never grants access by itself.

## Move from localhost to a domain

1. Configure DNS and a TLS reverse proxy for the new domain, forwarding to Selfhost's loopback listener.
2. Add the new exact `/auth/callback` URL to every provider client. Keep the old callback until testing succeeds.
3. Change **Selfhost address** in Access and review the plan. It shows both addresses and the callbacks.
4. Confirm and save. The old login configuration is backed up privately under `login-backups`. Existing OIDC sessions are invalidated. Local recovery remains available.
5. Test login through the domain before removing old callbacks from your providers.

This updates Selfhost's address. It does not rename the identity provider itself, configure DNS, issue certificates, or update provider callbacks automatically. A provider issuer change is a separate connection edit and also invalidates existing Selfhost sessions. Restore a reviewed backup with `identity plan --file <backup>` and `identity apply --file <backup>` if needed.

## Contributor profiles

- `catalog/identity-directories/*.json`: detection image markers, issuer environment keys, declared native YAML pointers, optional origin components and setup instructions.
- `catalog/identity-registration/*.json`: provider endpoint, request headers/body and returned client credential JSON pointers. Optional `project_creation` declares project creation and authenticated account lookup, including organization and human subject pointers.
- Request templates support `application-id`, `project-id`, `project-name`, `organization-id`, `name`, `redirect-uri` and the boolean `loopback-development`. No shell execution is involved.

Adding a registration profile requires tests for the real API contract, secret handling, exact callbacks, issuer validation and uncertain-result recovery. Providers without a profile remain usable through manual OpenID Connect settings.

## References

- [ZITADEL application creation API](https://zitadel.com/docs/reference/api/application/zitadel.application.v2.ApplicationService.CreateApplication)
- [ZITADEL project creation API](https://zitadel.com/docs/reference/api/project/zitadel.project.v2.ProjectService.CreateProject)
- [ZITADEL current user API](https://zitadel.com/docs/reference/api/auth/zitadel.auth.v1.AuthService.GetMyUser)
- [ZITADEL applications and integration settings](https://zitadel.com/docs/guides/manage/console/applications-overview)
- [ZITADEL local callback development mode](https://zitadel.com/docs/sdk-examples/react)
