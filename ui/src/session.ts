// The local proof is scoped to this origin and tab. It is insufficient without
// the HttpOnly session cookie and must never be attached to another service.
export async function sessionFetch(path: string, options: RequestInit = {}) {
  const url = new URL(path, location.origin);
  if (url.origin !== location.origin || !url.pathname.startsWith('/api/'))
    throw new Error('Invalid Selfhost API address.');
  const headers = new Headers(options.headers);
  const proof = sessionStorage.getItem('selfhost-client');
  if (proof) headers.set('x-selfhost-client', proof);
  const oidcProof = sessionStorage.getItem('selfhost-oidc-client');
  if (oidcProof) headers.set('x-selfhost-oidc-client', oidcProof);
  const response = await fetch(url, {
    ...options,
    headers,
    credentials: 'same-origin',
    redirect: 'error',
  });
  if (response.status === 401) {
    sessionStorage.removeItem('selfhost-client');
    sessionStorage.removeItem('selfhost-oidc-client');
    window.dispatchEvent(new Event('selfhost-session-ended'));
  }
  return response;
}
