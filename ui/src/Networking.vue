<script setup lang="ts">
import SshConnections from './SshConnections.vue';
import ProfileField from './components/ProfileField.vue';
import StringListInput from './components/StringListInput.vue';
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  projects: any[];
}>();
const inventory = ref<any>({
  data: { proxies: [], networks: [], routes: [] },
  proxy_profiles: {},
  network_profiles: [],
});
const tab = ref('Routes'),
  busy = ref(false),
  error = ref(''),
  message = ref(''),
  plan = ref<any>(null);
const proxy = reactive({
  id: '',
  name: '',
  provider: '',
  ssh_alias: '',
  admin_url: '',
  api_token: '',
  ca_certificate: '',
  config_directory: '',
  settings: {} as Record<string, any>,
  read_only: true,
});
const network = reactive({
  id: '',
  name: '',
  provider: '',
  description: '',
  endpoints: [] as string[],
  policy_reference: '',
});
const route = reactive({
  id: '',
  proxy_id: '',
  project_id: '',
  service: '',
  domain: '',
  upstream: '',
  network_id: '',
});
const profile = computed(() => inventory.value.proxy_profiles[proxy.provider]);
const proxyFields = computed(() =>
  Object.entries(profile.value?.settings_schema || {})
    .filter(
      ([, rule]: any) =>
        !rule.required_when || proxy.settings[rule.required_when[0]] === rule.required_when[1],
    )
    .map(([id, rule]: any) => ({ id, ...rule })),
);
const selectedProject = computed(() => props.projects.find((p) => p.id === route.project_id));
watch(
  () => proxy.provider,
  () => {
    proxy.settings = JSON.parse(JSON.stringify(profile.value?.defaults || {}));
  },
);
watch(route, () => (plan.value = null), { deep: true });
watch(
  () => route.project_id,
  () => (route.service = ''),
);
async function load() {
  inventory.value = await props.api('/networking');
}
async function run(fn: () => Promise<void>) {
  busy.value = true;
  error.value = '';
  message.value = '';
  try {
    await fn();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
function routeBody() {
  return { ...route, network_id: route.network_id || null };
}
async function editProxy(p: any) {
  proxy.provider = p.provider;
  await nextTick();
  Object.assign(proxy, JSON.parse(JSON.stringify(p)));
  message.value = '';
}
function newProxy() {
  Object.assign(proxy, {
    id: '',
    name: '',
    provider: '',
    ssh_alias: '',
    admin_url: '',
    api_token: '',
    ca_certificate: '',
    config_directory: '',
    settings: {},
    read_only: true,
  });
}
async function saveProxy() {
  await run(async () => {
    const settings = { ...proxy.settings };
    for (const [key, rule] of Object.entries(profile.value.settings_schema || {}) as [
      string,
      any,
    ][]) {
      if (rule.kind === 'certificate' && settings[key] !== 'new') {
        const value = Number(settings[key]);
        if (!Number.isSafeInteger(value) || value < 1)
          throw Error('Choose an existing certificate ID or request a new certificate.');
        settings[key] = value;
      }
    }
    await props.api(
      proxy.id ? `/networking/proxies/${proxy.id}` : '/networking/proxies',
      proxy.id ? 'PUT' : 'POST',
      { ...proxy, settings },
    );
    proxy.api_token = '';
    await load();
    message.value = 'Proxy connection saved.';
  });
}
async function saveNetwork() {
  await run(async () => {
    await props.api('/networking/networks', 'POST', {
      ...network,
      endpoints: network.endpoints.map((s) => s.trim()).filter(Boolean),
    });
    await load();
    message.value =
      'Existing network registered. Its enrollment and access rules remain managed by your network provider.';
  });
}
async function preview() {
  await run(async () => {
    plan.value = await props.api('/networking/routes/plan', 'POST', routeBody());
  });
}
async function probe() {
  await run(async () => {
    const result = await props.api('/networking/routes/probe', 'POST', routeBody());
    message.value = `${result.upstream_tls_verified ? 'Upstream HTTPS verified' : 'Loopback upstream reachable'} from ${result.tested_from} (HTTP ${result.http_status}).`;
  });
}
async function apply() {
  await run(async () => {
    const result = await props.api('/networking/routes/apply', 'POST', {
      route: routeBody(),
      revision: plan.value.revision,
    });
    message.value = result.message;
    plan.value = null;
    await load();
  });
}
onMounted(() => run(load));
</script>
<template>
  <div class="networking">
    <div class="tabs">
      <button
        v-for="name in ['Routes', 'Proxies', 'Private networks']"
        :key="name"
        class="button"
        :class="{ primary: tab === name }"
        @click="tab = name"
      >
        {{ name }}
      </button>
    </div>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <p v-if="message" role="status">{{ message }}</p>
    <section v-if="tab === 'Proxies'" class="panel">
      <h2>Reverse proxy connections</h2>
      <SshConnections
        :api="api"
        purpose="proxy"
        @select="
          (alias) => {
            proxy.ssh_alias = alias;
          }
        "
      />
      <p>Connect directly to its HTTPS API, or through an SSH alias for its host, VM or LXC.</p>
      <div v-for="p in inventory.data.proxies" :key="p.id" class="saved">
        <strong>{{ p.name }}</strong
        ><span
          >{{ inventory.proxy_profiles[p.provider]?.name || p.provider }} ·
          {{ p.read_only ? 'Read only' : 'Writes enabled' }}</span
        >
        <button type="button" class="button" :disabled="busy" @click="editProxy(p)">
          View and edit
        </button>
      </div>
      <button type="button" class="button" :disabled="busy" @click="newProxy">
        New proxy connection
      </button>
      <form @submit.prevent="saveProxy">
        <h3>{{ proxy.id ? 'Edit ' + proxy.name : 'Connect a reverse proxy' }}</h3>
        <div class="fields">
          <label>Name<input v-model="proxy.name" required /></label
          ><label
            >Provider<select v-model="proxy.provider" required>
              <option disabled value="">Choose a provider</option>
              <option v-for="p in inventory.proxy_profiles" :key="p.id" :value="p.id">
                {{ p.name }}
              </option>
            </select></label
          >
        </div>
        <template v-if="profile"
          ><p>{{ profile.description }}</p>
          <div class="fields">
            <label
              >SSH host alias<input
                v-model="proxy.ssh_alias"
                placeholder="proxy-lxc"
                :required="profile.driver === 'file_watch'"
              /><small>Uses your SSH configuration and verified host key.</small></label
            ><label v-if="profile.driver === 'file_watch'"
              >Watched configuration directory<input
                v-model="proxy.config_directory"
                placeholder="/etc/traefik/dynamic"
                required /></label
            ><template v-else
              ><label
                >Management API origin<input
                  v-model="proxy.admin_url"
                  type="url"
                  placeholder="http://127.0.0.1:2019"
                  required
                /><small>HTTP is allowed only for a loopback API reached over SSH.</small></label
              ><label v-if="!proxy.ssh_alias"
                >Private CA certificate (optional)<textarea
                  v-model="proxy.ca_certificate"
                  rows="3"
                /></label
              ><label
                >API bearer token<input
                  v-model="proxy.api_token"
                  type="password"
                  autocomplete="new-password"
                /><small v-if="proxy.id"
                  >Leave blank to keep the existing token for this connection.</small
                ></label
              ></template
            ><ProfileField
              v-for="rule in proxyFields"
              :key="rule.id"
              :field="rule"
              v-model="proxy.settings[rule.id]"
              :disabled="busy"
            />
          </div>
          <label class="check"><input v-model="proxy.read_only" type="checkbox" />Read only</label>
          <p v-if="!proxy.read_only">Reviewed route changes can be written to this proxy.</p>
          <button class="button primary" :disabled="busy">
            {{ proxy.id ? 'Save changes' : 'Save proxy connection' }}</button
          ><button v-if="proxy.id" type="button" class="button" :disabled="busy" @click="newProxy">
            Cancel editing
          </button></template
        >
      </form>
    </section>
    <section v-else-if="tab === 'Private networks'" class="panel">
      <h2>Connect an existing private network</h2>
      <p>
        Record the network that links your proxy and services, including the access policy that
        permits the connection.
      </p>
      <div v-for="n in inventory.data.networks" :key="n.id" class="saved">
        <strong>{{ n.name }}</strong
        ><span>{{ n.provider }} · {{ n.endpoints.length }} endpoints</span>
      </div>
      <form @submit.prevent="saveNetwork">
        <div class="fields">
          <label>Name<input v-model="network.name" required /></label
          ><label
            >Provider<select v-model="network.provider" required>
              <option disabled value="">Choose a provider</option>
              <option v-for="p in inventory.network_profiles" :key="p.id" :value="p.id">
                {{ p.name }}
              </option>
            </select></label
          >
        </div>
        <p>
          {{ inventory.network_profiles.find((p: any) => p.id === network.provider)?.guidance }}
        </p>
        <StringListInput
          v-model="network.endpoints"
          label="Peer identities or endpoints"
          :disabled="busy"
        /><label
          >Access policy reference<input
            v-model="network.policy_reference"
            placeholder="Proxy peers may reach app peers on TCP 8443"
            required /></label
        ><label>Notes<textarea v-model="network.description" rows="2" /></label
        ><button class="button primary" :disabled="busy">Register network</button>
      </form>
    </section>
    <section v-else class="panel">
      <h2>Service routes</h2>
      <p>
        Choose a proxy, a public hostname and an upstream on your service network. Cross-server
        connections require verified HTTPS; HTTP is allowed only on the proxy host's loopback
        address.
      </p>
      <div v-for="r in inventory.data.routes" :key="r.id" class="saved">
        <strong>{{ r.domain }}</strong
        ><span>{{ r.upstream }} · Configuration submitted</span>
      </div>
      <form @submit.prevent="preview">
        <div class="fields">
          <label>Route ID<input v-model="route.id" pattern="[a-z0-9_-]+" required /></label
          ><label
            >Proxy<select v-model="route.proxy_id" required>
              <option disabled value="">Choose a proxy</option>
              <option v-for="p in inventory.data.proxies" :key="p.id" :value="p.id">
                {{ p.name }}
              </option>
            </select></label
          ><label
            >Project<select v-model="route.project_id" required>
              <option disabled value="">Choose a project</option>
              <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select></label
          ><label
            >Service<select v-model="route.service" required>
              <option disabled value="">Choose a service</option>
              <option v-for="s in selectedProject?.services || []" :key="s.app" :value="s.app">
                {{ s.definition?.name || s.app }}
              </option>
            </select></label
          ><label
            >Public hostname<input
              v-model="route.domain"
              placeholder="cloud.example.com"
              required /></label
          ><label
            >Upstream origin<input
              v-model="route.upstream"
              type="url"
              placeholder="https://app.internal.example.com:8443"
              required /></label
          ><label
            >Private network<select v-model="route.network_id">
              <option value="">Direct HTTPS connection</option>
              <option v-for="n in inventory.data.networks" :key="n.id" :value="n.id">
                {{ n.name }}
              </option>
            </select></label
          >
        </div>
        <button class="button primary" :disabled="busy">Preview route</button
        ><button type="button" class="button" :disabled="busy || !route.upstream" @click="probe">
          Test upstream from proxy host
        </button>
      </form>
      <article v-if="plan" class="route-preview">
        <h3>Review proxy configuration</h3>
        <p>{{ plan.note }}</p>
        <pre>{{ JSON.stringify(plan.native_config, null, 2) }}</pre>
        <button class="button primary" :disabled="busy || !plan.can_apply" @click="apply">
          Apply route
        </button>
        <p v-if="!plan.can_apply">This proxy is read only.</p>
      </article>
    </section>
  </div>
</template>
<style scoped>
.networking {
  max-width: 1080px;
}
.tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 22px;
}
.fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 24px;
}
label {
  display: block;
  margin: 14px 0;
}
input:not([type='checkbox']),
select,
textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin-top: 8px;
}
input[type='checkbox'] {
  width: auto;
  margin-right: 8px;
}
small,
.saved span {
  display: block;
  color: var(--muted);
  margin-top: 6px;
}
.saved {
  padding: 16px 0;
  border-bottom: 1px solid var(--border);
}
.button {
  margin-right: 8px;
}
pre {
  max-height: 400px;
  overflow: auto;
  padding: 18px;
  background: var(--bg);
  border-radius: 12px;
}
.route-preview {
  margin-top: 24px;
}
p {
  line-height: 1.6;
}
@media (max-width: 700px) {
  .fields {
    grid-template-columns: 1fr;
  }
  .tabs {
    flex-wrap: wrap;
  }
}
</style>
