# Networking and Selfhost login

## Connections and routes

The **Networking** page and CLI share the same provider profiles and operations.
Proxy placement is independent of application placement: the SSH alias can point
to a physical host, VM, or LXC. Use an existing SSH configuration and verified host
key. SSH operations are non-interactive and never accept unknown host keys.

Connect a management API over verified HTTPS, optionally trusting a private CA.
Alternatively, Selfhost can use SSH to call a loopback HTTP API on the proxy host.
For this mode, that host needs `curl`. Tokens travel through stdin and are stored
in Selfhost's private data directory. They are not returned by the inventory API.
Connections default to read only; edit the connection to permit reviewed writes.

| Proxy               | Native configuration                                                                     | Certificate handling                                                                                     |
| ------------------- | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Caddy               | Append a hostname route to an existing HTTPS server, using an ETag for the entire server | Existing automatic HTTPS policy issues and renews certificates                                           |
| Traefik             | Create a separate JSON file in an existing watched directory over SSH                    | Reference an existing ACME resolver and HTTPS entrypoint                                                 |
| Nginx Proxy Manager | Create a proxy host through its API                                                      | Select an existing certificate ID, or `new` with a contact email and explicit acceptance of the CA terms |

Caddy must listen on port 443 with certificate automation enabled and no skipped
domains. Run it with persistent autosave storage and `caddy run --resume` so API
changes survive a restart. Traefik's SSH account and process need compatible
permissions for its watched directory; its static entrypoint/resolver configuration
is managed separately. Nginx Proxy Manager needs a valid API bearer token.

Routes require a project, service, hostname and upstream origin. Cross-server
upstreams use HTTPS with certificate verification. An HTTP loopback upstream is
also supported when the proxy has an SSH alias; this represents a local service
or the local end of an existing restricted tunnel. A proxy running in a container
needs connectivity from its own network namespace, not just its host.

Private upstream CAs can be configured using files available to the proxy process.
Caddy uses its native file CA module; Traefik uses a servers transport; Nginx Proxy
Manager uses its trusted certificate bundle. The CA's presence does not disable
hostname verification. Upstream certificates and their renewal remain the
operator's responsibility.

Preview reads the current proxy configuration and returns a revision bound to the
proxy profile, settings, route and project setup. Apply rechecks the revision and
requires a writable project server and proxy. Caddy also sends `If-Match` to guard
against another writer. Traefik files are created without overwriting an existing
file. Operation records are saved before writing; an uncertain operation blocks
automatic retries to avoid creating duplicate resources.

An applied route is **configuration submitted**, not a certificate readiness
claim. DNS and ACME challenge reachability must already be configured. The probe
runs from the SSH host and checks upstream reachability and HTTPS verification.
It does not verify the public certificate, a container's network namespace, or
the network provider's access policy. Route update, removal, adoption and automatic
recovery of interrupted writes are not yet implemented.

```sh
selfhost networking
selfhost proxy-add proxy.json
selfhost proxy-edit proxy.json
selfhost network-add network.json
selfhost route-plan route.json
selfhost route-apply route.json --revision REVIEWED_REVISION
selfhost route-probe route.json
```

An example proxy file:

```json
{
  "id": "",
  "name": "Home proxy",
  "provider": "caddy",
  "ssh_alias": "proxy-lxc",
  "admin_url": "http://127.0.0.1:2019",
  "read_only": true,
  "settings": { "server": "srv0" }
}
```

An example route file:

```json
{
  "id": "cloud",
  "proxy_id": "PROXY_ID",
  "project_id": "PROJECT_ID",
  "service": "nextcloud",
  "domain": "cloud.example.com",
  "upstream": "https://cloud.internal.example.com:8443",
  "network_id": null
}
```

## Private networks

WireGuard, NetBird, Tailscale/Headscale, OpenSSH tunnels and custom networks can be
registered with their endpoint identities and access-policy reference. This is an
inventory of **existing networks**. Selfhost does not currently install agents,
enroll peers, distribute keys, create tunnels, or apply network ACLs.

Use separate identities and narrowly scoped rules for proxy-to-service traffic.
WireGuard AllowedIPs do not replace destination-port firewall rules. For NetBird,
use separate groups and explicit unidirectional policies. SSH tunnels should bind
to loopback and use a restricted account with specific PermitOpen destinations.
Network registration is not proof that these controls are active.

## Selfhost login

**Access > Connect an existing identity provider** accepts a compatible OpenID
Connect issuer without requiring a brand-specific login adapter. Configure Selfhost's address, a client and exact
provider subject IDs allowed to administer the workspace. Register the callback
`https://selfhost.example.com/auth/callback` at the provider, or the exact local
callback such as `http://localhost:8797/auth/callback`. Use the actual dashboard
port. Non-loopback addresses require HTTPS. Public clients and
clients using a secret are supported. Providers must support Authorization Code
with S256 PKCE and the token authentication method used by the client.

The implementation validates discovery, issuer, signature, audience, nonce,
expiry and the access-token hash when present. Browser state is bound to an
HttpOnly cookie and can be consumed only once. HTTPS sessions use Secure,
HttpOnly, SameSite=Lax cookies. Both HTTP and HTTPS loopback OIDC sessions use
port-specific cookie names plus an origin-bound browser proof in session storage.
HTTP omits Secure; HTTPS retains it. HTTPS does not provide cookie isolation
between ports, so the extra local proof remains required.
Sessions expire after eight hours or 30 minutes without dashboard interaction,
and are held in memory. Background polling does not extend idle time. Changes to login settings
invalidate existing OIDC sessions. All allowed identities are administrators; scoped
roles, group mapping, SAML and direct LDAP login are not implemented.

Private CA bundles can be supplied for the identity provider. TLS verification
stays enabled, discovery/token responses are bounded, and redirects are disabled
for server-to-server identity requests.

An explicitly configured localhost or loopback-IP identity issuer may use HTTP.
Plaintext discovery, keys and token requests must remain on that exact issuer
origin. This does not permit a remote HTTP issuer or a HTTPS issuer to downgrade
its endpoints. Local HTTP still requires PKCE, nonce, single-use state and exact
administrator subject IDs; providers may require a development-mode setting for
the loopback callback.

See [identity setup](identity-setup.md) for the simple manual form, advanced
settings, directory inspection and supported automatic client registration.
Changing from localhost to a domain uses a reviewed address change after adding
the new exact callbacks at the provider. Existing OIDC sessions are invalidated;
DNS, certificates, callbacks and the provider's own issuer are managed separately.

Selfhost continues to listen on loopback. Put a TLS proxy on that host or use a
restricted encrypted tunnel from another proxy. Preserve the configured Host
header. Selfhost does not trust forwarded headers to choose its public origin.
All API writes require an exact Origin match. Cross-origin and same-site browser
requests to the API are rejected. The local startup link expires after ten minutes
and can be exchanged only once, on the local dashboard origin. It is not an API
bearer token. Keep your signed-in local tab available while configuring login.
Restart `selfhost serve` to issue a new recovery link. A local CLI can also repair login settings:

```sh
selfhost login-config login.json
selfhost login-config
```

The second command prints redacted settings. Saving with `login-config` is limited
to initial configuration or the existing Selfhost address. To change that address,
use the reviewed workflow and confirm the new callbacks:

```sh
selfhost identity plan --file login.json
selfhost identity apply --file login.json --revision REVIEWED_REVISION --confirm-callbacks
```

The dashboard uses the same plan/apply checks. `GET /api/login/settings` reads the
redacted settings; direct PUT updates are not supported. An empty secret preserves an existing
secret only for the same provider ID, issuer and client ID. To switch that same
client to public-client authentication, remove its saved secret from the private
login configuration while the server is stopped.

### Account and session controls

The top-right account button shows the authenticated identity, provider, access
level and absolute session expiry. Local recovery is explicitly labeled as local
administrator access. Sign out revokes the server session immediately. OIDC users
can also sign out all their Selfhost sessions without ending other users' sessions.
These controls do not sign you out of the identity provider itself.

Local recovery sessions use an HttpOnly, SameSite=Strict cookie plus a random browser
proof in tab-scoped session storage. Local OIDC uses a separate SameSite=Lax cookie
and separate proof under both HTTP and HTTPS; HTTPS cookies also remain Secure.
Because cookies cannot be isolated by port,
both values are required: another loopback service that sees a cookie cannot use
it alone. Local sessions are bound to their exact scheme, hostname and port and
cannot authenticate at a different origin. Session IDs and browser proofs are never
returned by the account endpoint. The initial link is removed from browser history
before exchange. Closing the tab discards its browser proof; local recovery then
requires a fresh startup link, while local OIDC requires a new sign-in. Do not
share browser proofs, local links, or session cookies.

Responses, including errors and login responses, are non-cacheable and carry
content, framing and referrer protections. Configured HTTPS origins, including
HTTPS loopback, also send HSTS. Discovery is limited to four concurrent requests and 30 starts per
minute globally, before contacting an identity provider. JSON bodies are capped
at 2 MiB. No cross-origin API access is enabled.

See [security boundaries](security.md) for deployment assumptions and audit limits.

## Create your own identity provider

The Access page can create Keycloak, authentik or ZITADEL as a stopped project.
These are Selfhost-owned, declarative Compose definitions, with generated secrets,
private PostgreSQL storage, loopback ingress and editable configuration.

```sh
selfhost stacks
selfhost stack-create identity.json
```

```json
{
  "blueprint": "keycloak",
  "name": "Identity",
  "server_id": "local",
  "inputs": {
    "domain": "identity.example.com",
    "port": 18080,
    "admin-email": "admin@example.com"
  }
}
```

Follow the stack's bootstrap instructions and configured external address. These
hosted stack profiles use domain/TLS settings; allowing a local Selfhost callback
does not automatically reconfigure the identity provider deployment. Existing
provider clients can be connected manually; supported registration profiles can
create a reviewed client using a temporary provider API token. Exact administrator
subjects must still be supplied. See [automatic identity setup](identity-setup.md#let-selfhost-create-the-client).
ZITADEL's master key must be backed up with its
database, and its login-client PAT needs rotation before its generated one-year
expiry. authentik outposts are managed separately without exposing the Docker socket.

Projects use the existing Compose/environment editor and standalone export.
Exports include secrets, configuration and ordinary Compose files, but not volume
contents. Back up database and persistent volumes separately. Startup wait time is
configured through `x-selfhost.wait_seconds` in Compose (10 to 900 seconds).

## Add providers

Contributors add proxy profiles under `catalog/infrastructure/proxies`, network
entries in `catalog/infrastructure/networks.json`, and stack blueprints under
`catalog/stacks`. Local overrides can be placed in `proxy-profiles` and
`stack-profiles` under the Selfhost data directory. The dashboard reads these
schemas to build provider choices and input forms.

Proxy profiles declare native JSON templates, typed settings, optional loopback
HTTP templates, preflight checks and one of `json_collection`, `rest_create` or
`file_watch`. A provider that needs a different protocol requires another generic
driver. Exact substitutions preserve JSON types; null optional substitutions omit
their property. Stack blueprints declare inputs, Compose and secret/expiry
generators. Provider names and app-specific paths belong in these profiles.

## Verification and current limits

Local disposable tests verified real Keycloak Authorization Code/PKCE login,
authenticated APIs, CSRF rejection, logout, callback replay rejection, administrator
allowlist enforcement, and rejection of an untrusted CA. Caddy route creation,
ETag concurrency, preservation of existing routes, internal certificate issuance
and an HTTPS request through a route with verified upstream TLS were exercised.
Public ACME issuance and renewal were not exercised. Traefik and Nginx Proxy
Manager profiles have validation coverage but have not been tested against live
instances. Keycloak, authentik and ZITADEL stacks started locally; full sign-in was
exercised with Keycloak only. No production environment was changed.

Provider references: [Caddy API](https://caddyserver.com/docs/api),
[automatic HTTPS](https://caddyserver.com/docs/automatic-https),
[Traefik TLS](https://doc.traefik.io/traefik/reference/routing-configuration/http/tls/overview/),
[Nginx Proxy Manager](https://nginxproxymanager.com/advanced-config/),
[Keycloak containers](https://www.keycloak.org/server/containers),
[authentik automated install](https://docs.goauthentik.io/install-config/automated-install),
[ZITADEL Compose deployment](https://zitadel.com/docs/self-hosting/deploy/compose),
[NetBird access control](https://docs.netbird.io/manage/access-control).
