<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { TabsRoot, TabsList, TabsTrigger, TabsContent } from 'reka-ui';
import { KeyRound, Plus, Download, ChevronRight } from 'lucide-vue-next';
import IdentityRegister from './IdentityRegister.vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const config = ref<any>({ public_url: location.origin, providers: [] }),
  busy = ref(false),
  message = ref(''),
  error = ref(''),
  review = ref<any>(null),
  confirmed = ref(false),
  mode = ref('manual');
const callback = computed(() => `${config.value.public_url.replace(/\/$/, '')}/auth/callback`);
function add() {
  config.value.providers.push({
    id: `identity-${config.value.providers.length + 1}`,
    name: 'My identity provider',
    issuer: '',
    client_id: '',
    client_secret: '',
    ca_certificate: '',
    admin_subjects: [],
    subjects: '',
  });
}
function hydrate(value: any) {
  config.value = {
    public_url: value.public_url || location.origin,
    providers: (value.providers || []).map((p: any) => ({
      ...p,
      client_secret: p.client_secret || '',
      subjects: (p.admin_subjects || []).join('\n'),
    })),
  };
}
onMounted(async () => {
  try {
    hydrate(await props.api('/login/settings'));
    if (!config.value.providers.length) add();
  } catch (e) {
    error.value = (e as Error).message;
  }
});
watch(
  config,
  () => {
    review.value = null;
    confirmed.value = false;
  },
  { deep: true },
);
function payload() {
  return {
    public_url: config.value.public_url,
    providers: config.value.providers.map((p: any) => ({
      id: p.id,
      name: p.name,
      issuer: p.issuer,
      client_id: p.client_id,
      client_secret: p.client_secret,
      ca_certificate: p.ca_certificate || '',
      admin_subjects: p.subjects
        .split('\n')
        .map((s: string) => s.trim())
        .filter(Boolean),
    })),
  };
}
async function prepare() {
  busy.value = true;
  error.value = '';
  message.value = '';
  try {
    review.value = await props.api('/login/plan', 'POST', payload());
    confirmed.value = false;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function save() {
  busy.value = true;
  error.value = '';
  try {
    const result = await props.api('/login/apply', 'POST', {
      config: payload(),
      expected_revision: review.value.revision,
      callbacks_confirmed: confirmed.value,
    });
    for (const p of config.value.providers) {
      p.has_secret = p.has_secret || !!p.client_secret;
      p.client_secret = '';
    }
    review.value = null;
    message.value = `Connection saved. Test sign-in at ${result.public_url}. Keep this recovery session open until sign-in succeeds.`;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function importFile(event: Event) {
  const input = event.target as HTMLInputElement,
    file = input.files?.[0];
  if (!file) return;
  try {
    if (file.size > 1048576) throw new Error('Choose a JSON file smaller than 1 MiB.');
    const value = JSON.parse(await file.text());
    if (typeof value.public_url !== 'string' || !Array.isArray(value.providers))
      throw new Error('Choose a Selfhost login manifest.');
    hydrate(value);
    mode.value = 'manual';
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    input.value = '';
  }
}
function download() {
  const value = payload();
  for (const p of value.providers) p.client_secret = '';
  const link = document.createElement('a');
  link.href = URL.createObjectURL(
    new Blob([JSON.stringify(value, null, 2) + '\n'], { type: 'application/json' }),
  );
  link.download = 'selfhost-login.json';
  link.click();
  URL.revokeObjectURL(link.href);
  message.value = 'Downloaded without secrets. Fill in required credentials locally.';
}
</script>
<template>
  <section class="panel access-settings" aria-labelledby="connect-identity-title">
    <div class="section-heading">
      <KeyRound :size="24" aria-hidden="true" />
      <div>
        <h2 id="connect-identity-title">Connect an existing identity provider</h2>
        <p>
          Sign in with your existing ZITADEL, Keycloak, authentik, or any OpenID Connect provider.
        </p>
      </div>
    </div>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <p v-if="message" role="status">{{ message }}</p>
    <TabsRoot v-model="mode"
      ><TabsList class="setup-tabs" aria-label="Connection method"
        ><TabsTrigger value="manual">Enter connection details</TabsTrigger
        ><TabsTrigger value="automatic">Set up automatically</TabsTrigger
        ><TabsTrigger value="directory">Use the CLI</TabsTrigger></TabsList
      >
      <TabsContent value="automatic"
        ><label
          >Selfhost address<input v-model="config.public_url" type="url" required /><small
            >Start on localhost or use your HTTPS domain.</small
          ></label
        ><IdentityRegister
          :api="api"
          :selfhost-url="config.public_url"
          @connected="
            (value: any) => {
              hydrate(value);
              mode = 'manual';
              message =
                'Identity provider connected. Test sign-in while keeping local recovery open.';
            }
          "
      /></TabsContent>
      <TabsContent value="manual"
        ><form @submit.prevent="prepare">
          <h3>1. Choose your Selfhost address</h3>
          <p>Start on this computer and switch to a domain whenever you are ready.</p>
          <label
            >Selfhost address<input
              v-model="config.public_url"
              type="url"
              placeholder="http://localhost:8372"
              required
            /><small
              >HTTP is allowed only on localhost or a loopback IP. Domain names require
              HTTPS.</small
            ></label
          >
          <div class="callback">
            <span>Register this callback with your provider</span><code>{{ callback }}</code>
          </div>
          <h3>2. Connect your provider</h3>
          <p>
            Create or select a Web OIDC client with authorization code and PKCE S256, then copy its
            details.
          </p>
          <article v-for="(p, i) in config.providers" :key="i">
            <h4>{{ p.name || 'Identity provider' }}</h4>
            <div class="fields">
              <label
                >Provider name<input v-model="p.name" maxlength="100" required /><small
                  >Shown on the sign-in button.</small
                ></label
              ><label
                >Issuer URL<input
                  v-model="p.issuer"
                  type="url"
                  placeholder="https://identity.example.com"
                  required
                /><small>Copy the issuer from your provider's OpenID configuration.</small></label
              ><label>Client ID<input v-model="p.client_id" required /></label
              ><label
                >Client secret (if required)<input
                  v-model="p.client_secret"
                  type="password"
                  autocomplete="new-password"
                /><small>{{
                  p.has_secret
                    ? 'Leave blank to preserve the saved secret for this client.'
                    : 'Public clients use PKCE without a secret.'
                }}</small></label
              >
            </div>
            <label
              >Who can administer Selfhost?<textarea
                v-model="p.subjects"
                rows="2"
                required
                placeholder="Exact user subject ID, one per line"
              /><small
                >Use the exact OIDC subject ID (sub), not an email. These users have full access to
                connected services.</small
              ></label
            >
            <details>
              <summary>Advanced connection settings</summary>
              <label>Connection ID<input v-model="p.id" pattern="[a-z0-9_-]+" required /></label
              ><label
                >Private CA certificates (PEM)<textarea
                  v-model="p.ca_certificate"
                  rows="4"
                  spellcheck="false"
                /><small
                  >Optional. Trust your private CA while preserving certificate verification.</small
                ></label
              >
              <p class="help">
                Keycloak issuers include <code>/realms/your-realm</code>. authentik issuers include
                the application slug. ZITADEL needs development mode for HTTP loopback callbacks.
              </p>
            </details>
            <button
              v-if="config.providers.length > 1"
              type="button"
              class="text-button"
              @click="config.providers.splice(i, 1)"
            >
              Remove {{ p.name }} from draft
            </button>
          </article>
          <button type="button" class="button" @click="add">
            <Plus :size="16" /> Add another provider
          </button>
          <h3>3. Review and test sign-in</h3>
          <p>
            Review the callback and address changes before saving. Previous settings are backed up
            automatically.
          </p>
          <button class="button primary" :disabled="busy || !config.providers.length">
            {{ busy ? 'Checking…' : 'Review connection' }}<ChevronRight :size="16" />
          </button>
        </form>
        <section v-if="review" class="connection-review" aria-labelledby="review-login-title">
          <h3 id="review-login-title">Review sign-in settings</h3>
          <p v-if="review.address_changed">
            Address changes from <code>{{ review.previous_url }}</code> to
            <code>{{ review.public_url }}</code
            >.
          </p>
          <ul>
            <li v-for="p in review.providers" :key="p.id">
              <strong>{{ p.name }}</strong
              >: {{ p.administrators }} administrator(s); callback <code>{{ p.callback }}</code>
            </li>
          </ul>
          <ol>
            <li v-for="step in review.steps" :key="step">{{ step }}</li>
          </ol>
          <label class="confirmation"
            ><input v-model="confirmed" type="checkbox" /> I registered this exact callback for
            every provider and can still access local recovery.</label
          ><button
            type="button"
            class="button primary"
            :disabled="busy || !confirmed"
            @click="save"
          >
            Save connection
          </button>
        </section>
      </TabsContent>
      <TabsContent value="directory"
        ><h3>Connect from the provider directory</h3>
        <p>
          Run Selfhost where you manage the provider's configuration. Inspection reads Compose and
          environment files without running them.
        </p>
        <pre><code>selfhost identity inspect . --selfhost-url {{config.public_url||'http://localhost:8372'}}</code></pre>
        <p>Use the detected provider guidance to fill in a connection template.</p>
        <button type="button" class="button" @click="download">
          <Download :size="16" /> Download connection template
        </button>
        <pre><code>selfhost identity plan --directory .
selfhost identity apply --directory . --revision &lt;reviewed-revision&gt; --confirm-callbacks</code></pre>
        <p class="help">
          Use the same Selfhost data directory as your dashboard. The CLI saves the connection and
          backs up previous settings without changing the provider's files or running services.
          Templates exclude secrets.
        </p>
        <label
          >Load a prepared connection file<input
            type="file"
            accept="application/json,.json"
            @change="importFile" /></label
      ></TabsContent>
    </TabsRoot>
  </section>
</template>
<style scoped>
.access-settings {
  max-width: 980px;
  margin-bottom: 24px;
}
.section-heading {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}
.section-heading > svg {
  color: var(--green);
  margin-top: 4px;
}
h2,
h4 {
  margin: 0 0 10px;
}
h3 {
  margin: 28px 0 10px;
}
p,
li {
  line-height: 1.6;
}
.setup-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  border-bottom: 1px solid var(--border);
  margin: 22px 0;
}
.setup-tabs button {
  background: transparent;
  color: var(--muted);
  border: 0;
  border-bottom: 2px solid transparent;
  padding: 12px 16px;
  font: inherit;
  cursor: pointer;
}
.setup-tabs button[data-state='active'] {
  color: var(--green);
  border-bottom-color: var(--green);
}
.fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 20px;
}
label {
  display: block;
  margin: 16px 0;
}
input,
textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 8px 0;
}
article {
  border: 1px solid var(--border);
  padding: 18px;
  border-radius: 10px;
  margin: 16px 0;
}
small,
.help {
  color: var(--muted);
  line-height: 1.5;
}
code {
  overflow-wrap: anywhere;
}
.callback {
  padding: 16px;
  border-radius: 8px;
  background: #191d1b;
  display: grid;
  gap: 8px;
}
.callback > span {
  color: var(--muted);
  font-size: 13px;
}
.button {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  margin: 8px 12px 0 0;
}
summary {
  cursor: pointer;
  padding: 12px 0;
  color: var(--green);
}
.connection-review {
  margin-top: 24px;
  border: 1px solid var(--green);
  border-radius: 10px;
  padding: 20px;
}
.confirmation {
  display: flex;
  gap: 10px;
  line-height: 1.5;
  align-items: flex-start;
}
.confirmation input {
  width: auto;
  margin: 5px 0;
  flex: none;
}
pre {
  white-space: pre-wrap;
  word-break: break-word;
  padding: 16px;
  background: #191d1b;
  border-radius: 8px;
  line-height: 1.7;
}
button:focus-visible,
summary:focus-visible,
input:focus-visible,
textarea:focus-visible {
  outline: 2px solid var(--green);
  outline-offset: 3px;
}
@media (max-width: 680px) {
  .fields {
    grid-template-columns: 1fr;
  }
  .setup-tabs button {
    padding: 12px 8px;
  }
}
</style>
