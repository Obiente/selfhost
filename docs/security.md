# Dashboard and API security

Selfhost is a privileged administration tool. An authorized administrator can
edit service configuration, run declared actions, install Compose workloads and
access stored credentials. Give administrator access only to people trusted with
all connected services. There are no project-scoped roles or limited API keys yet.

## Deployment boundary

- The management listener binds to loopback. Remote login requires the configured
  HTTPS origin and an explicitly allowed OpenID Connect subject, never a wildcard.
- Local OIDC login may use the exact configured HTTP loopback origin. Non-loopback
  HTTP addresses are rejected. A local identity-provider issuer may also use HTTP;
  every plaintext identity endpoint must stay on that explicitly selected issuer
  origin. An HTTPS issuer cannot downgrade token or discovery requests to HTTP.
- Use a dedicated origin for Selfhost. Do not serve untrusted applications or
  contributor files under its origin. A same-origin script injection could act
  with the administrator's privileges despite HttpOnly cookies.
- Terminate HTTPS at a trusted proxy with a local or restricted encrypted connection
  to Selfhost. Preserve Host. Forwarded headers cannot override the allowed origin.
- Require MFA at the identity provider and restrict network access to the management
  endpoint. Selfhost validates OIDC tokens but does not enforce a provider-specific
  MFA claim. Proxy rate and connection limits should protect the listener as well.
- Keep local applications and the operating-system account trusted. Local recovery
  uses HTTP and is restricted to loopback. It is not a remote access mechanism.
  Local OIDC uses the same operating-system trust boundary. Cookies are not
  isolated by port, even under HTTPS, so both HTTP and HTTPS loopback OIDC sessions
  additionally require a random, origin-bound browser proof held in tab-scoped
  session storage. Local recovery requires its own separate proof.
- Protect the private data directory, backups, proxy credentials and SSH keys.
  Filesystem permissions do not provide encryption at rest. Use encrypted storage
  where needed and grant each remote credential only the access it needs.
- Treat contributed recipes and setup scripts as executable code. Inspect custom
  commands, image privileges, host mounts and environment settings before applying.

## Session and API enforcement

See [login and account controls](networking-and-login.md#selfhost-login) for session
lifetimes and local recovery. Every registered API route goes through the shared
authorization middleware. GET and HEAD do not authorize writes. Other methods
require the exact Origin, and browser Fetch Metadata must indicate same-origin.
Local recovery credentials cannot authenticate a public-host request. Server-side
read-only connection restrictions remain in force after login.

HTTPS OIDC sessions use Secure, HttpOnly, SameSite=Lax cookies. All loopback OIDC
sessions use port-specific cookie names and their own browser-proof header;
HTTP omits Secure, while HTTPS keeps it. Local
recovery uses an HttpOnly, SameSite=Strict cookie and a separate recovery proof.
The callback sends the local OIDC proof in a URL fragment, which the dashboard
immediately removes from the visible URL and stores for that tab. A cookie alone
does not authorize either local session. PKCE, single-use state, nonce and the
exact administrator subject allowlist still apply to local OIDC.

Changing Selfhost's configured address or provider settings invalidates OIDC
sessions. It does not update DNS, TLS, provider callbacks or an identity provider's
own external domain. The settings endpoint is read-only; browser changes use the
reviewed `/api/login/plan` and `/api/login/apply` operations. The legacy
`login-config` CLI can inspect or save a connection at its existing address but
cannot change an existing Selfhost origin. Use `identity plan` and `identity apply`
with explicit callback confirmation, following the staged
[localhost-to-domain procedure](identity-setup.md#move-from-localhost-to-a-domain).
Retain local recovery while testing.

Logout, idle expiry and login-configuration changes affect subsequent requests;
they do not cancel an operation already authorized and running. Restarting the
management process discards all sessions. Background schedules continue while the
process runs, independently of whether a browser is signed in.

## Existing resources and removal

Existing-app links are separate from managed project records. Their granted
actions remain bounded by their profile, pinned container identity and read-only
server setting. An app URL is not evidence of runtime ownership.

Removal plans bind project settings, server settings and selected container IDs to
a short-lived review. Configuration backup is the default; its coverage excludes
application volume data. Container removal stops the exact reviewed IDs and never
passes a volume-removal or prune flag. Shared database-source projects and stacks
with an external lifecycle, including AIO, cannot use this runtime deletion path.
Interrupted removal blocks further project mutations until reconciliation reads
runtime state and recovers desired configuration. Reconciliation never restarts
or deletes containers. The ordinary setup editor and public setup API also reject
configuration saves that omit existing services, so deleting a Compose service
cannot bypass the reviewed removal path. See [removal coverage and recovery](removal.md).

## npm launcher trust

The launcher does not download executables or evaluate a shell command. It accepts
an explicitly selected `SELFHOST_BINARY`, a matching bundled binary with a valid
SHA-256 manifest entry, or a native executable on an absolute `PATH` entry. Script
and npm-shim rejection prevents recursive invocation.

Checksums detect changes relative to the package's manifest; they do not authenticate
the publisher or replace signed releases. Executable-header checks identify native
file formats, not the program's source language or trustworthiness. The explicit
override, installed executable, package contents and filesystem remain trusted inputs.
The launcher is not a sandbox. See [local packaging and release boundaries](distribution.md).

## Verification and dependency findings

Automated regression tests cover recovery replay, cookie/proof separation,
origin binding, idle and absolute expiry, logout and identity-scoped revocation,
API authentication, CSRF rejection, duplicate cookies, host spoofing, body limits,
security headers, login-work limits and OIDC configuration revocation.

These checks are not an independent penetration test or a guarantee against every
attack. New API routes, credential handling and executable recipe features need
security review as they are added.

The lockfile pins `rsa 0.9.10`, pulled in by `openidconnect 4.0.1`, which is affected by
[RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).
The advisory concerns private-key
timing leakage and has no patched version. Selfhost's OIDC code uses that library
to verify provider signatures with public keys; it does not perform RSA private-key
signing or decryption. The vulnerable private-key operation is not used by this
integration, but the dependency finding remains open and is not suppressed. Recheck
this assessment if private-key operations or another consumer of `rsa` are added.

The session and request controls follow the
[OWASP session guidance](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
and [CSRF guidance](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html).
