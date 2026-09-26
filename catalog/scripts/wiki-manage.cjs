// App-native GraphQL operations for Wiki.js 2.5. Credentials are never printed.
const { createHash } = require('node:crypto');
class OperationError extends Error {}
const env = process.env;
const endpoint = 'http://127.0.0.1:3000/graphql';
let jwt = '';
async function graphql(query, variables = {}) {
  const response = await fetch(endpoint, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(jwt ? { Authorization: `Bearer ${jwt}` } : {}),
    },
    body: JSON.stringify({ query, variables }),
    signal: AbortSignal.timeout(20000),
    redirect: 'error',
  });
  if (!response.ok) throw new OperationError('Wiki.js rejected the operation');
  const body = await response.json();
  if (body.errors?.length) throw new OperationError('Wiki.js rejected the GraphQL operation');
  return body.data;
}
const currentQuery =
  '{authentication{activeStrategies{key strategy{key} displayName order isEnabled selfRegistration domainWhitelist autoEnrollGroups config{key value}}}}';
const digest = (value) => createHash('sha256').update(JSON.stringify(value)).digest('hex');
async function main() {
  const login = await graphql(
    'mutation($username:String!,$password:String!){authentication{login(username:$username,password:$password,strategy:"local"){responseResult{succeeded} jwt mustProvideTFA mustChangePwd}}}',
    { username: env.SELFHOST_ADMIN_EMAIL, password: env.SELFHOST_ADMIN_PASSWORD },
  );
  const auth = login.authentication.login;
  if (!auth.responseResult?.succeeded || !auth.jwt || auth.mustProvideTFA || auth.mustChangePwd)
    throw new OperationError(
      'A local administrator without a pending MFA or password challenge is required; otherwise use Wiki.js administration',
    );
  jwt = auth.jwt;
  const current = (await graphql(currentQuery)).authentication.activeStrategies;
  if (env.SELFHOST_OPERATION === 'inspect') {
    console.log(
      JSON.stringify({
        revision: digest(current),
        providers: current.map((s) => ({
          id: s.key,
          name: s.displayName,
          kind: s.strategy.key,
          enabled: s.isEnabled,
        })),
      }),
    );
    return;
  }
  if (env.SELFHOST_OPERATION !== 'oidc') throw new OperationError('Unsupported operation');
  if (digest(current) !== env.SELFHOST_REVISION)
    throw new OperationError(
      'Identity providers changed; inspect them again before adding a provider',
    );
  const key = env.SELFHOST_PROVIDER;
  if (!/^[a-z][a-z0-9-]{2,40}$/.test(key) || key === 'local' || current.some((s) => s.key === key))
    throw new OperationError('Choose a new provider ID; existing providers are never replaced');
  for (const name of ['ISSUER', 'AUTHORIZE_URL', 'TOKEN_URL', 'USERINFO_URL']) {
    const url = new URL(env[`SELFHOST_${name}`]);
    if (url.protocol !== 'https:' || url.username || url.password || url.hash)
      throw new OperationError('Provider endpoints must use HTTPS without credentials');
  }
  const previous = current.map((s) => ({
    key: s.key,
    strategyKey: s.strategy.key,
    displayName: s.displayName,
    order: s.order,
    isEnabled: s.isEnabled,
    selfRegistration: s.selfRegistration,
    domainWhitelist: s.domainWhitelist,
    autoEnrollGroups: s.autoEnrollGroups,
    config: s.config.map((c) => ({
      key: c.key,
      value: JSON.stringify({ v: JSON.parse(c.value).value }),
    })),
  }));
  const config = {
    clientId: env.SELFHOST_CLIENT_ID,
    clientSecret: env.SELFHOST_CLIENT_SECRET,
    issuer: env.SELFHOST_ISSUER,
    authorizationURL: env.SELFHOST_AUTHORIZE_URL,
    tokenURL: env.SELFHOST_TOKEN_URL,
    userInfoURL: env.SELFHOST_USERINFO_URL,
    skipUserProfile: false,
    emailClaim: 'email',
    displayNameClaim: 'name',
    pictureClaim: 'picture',
    mapGroups: false,
    groupsClaim: 'groups',
    logoutURL: '',
    acrValues: '',
  };
  const next = {
    key,
    strategyKey: 'oidc',
    displayName: env.SELFHOST_LABEL,
    order: current.length,
    isEnabled: true,
    selfRegistration: false,
    domainWhitelist: [],
    autoEnrollGroups: [],
    config: Object.entries(config).map(([key, v]) => ({ key, value: JSON.stringify({ v }) })),
  };
  if (
    digest((await graphql(currentQuery)).authentication.activeStrategies) !== env.SELFHOST_REVISION
  )
    throw new OperationError('Identity providers changed during review');
  const result = await graphql(
    'mutation($strategies:[AuthenticationStrategyInput]!){authentication{updateStrategies(strategies:$strategies){responseResult{succeeded}}}}',
    { strategies: [...previous, next] },
  );
  if (!result.authentication.updateStrategies.responseResult?.succeeded)
    throw new OperationError(
      'Wiki.js could not complete the change; inspect provider state before retrying',
    );
  console.log(
    JSON.stringify({
      configured: key,
      localLoginPreserved: previous.some((s) => s.strategyKey === 'local' && s.isEnabled),
      selfRegistration: false,
      administratorMapping: false,
    }),
  );
}
main().catch((error) => {
  console.error(
    error instanceof OperationError
      ? error.message
      : 'Wiki.js operation failed; inspect provider state before retrying.',
  );
  process.exitCode = 1;
});
