<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { ExternalLink, Link, Plus, RefreshCw, Settings2, Unlink } from 'lucide-vue-next';
import BaseDialog from './components/BaseDialog.vue';
import EmptyState from './components/EmptyState.vue';
import { useAsyncTask } from './composables/useAsyncTask';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const { busy, error, run, clearError } = useAsyncTask();
const apps = ref<any[]>([]),
  profiles = ref<any[]>([]),
  servers = ref<any[]>([]);
const addOpen = ref(false),
  profile = ref(''),
  name = ref(''),
  url = ref(''),
  server = ref(''),
  container = ref(''),
  advanced = ref(false);
const selected = ref<any>(null),
  dialog = ref(''),
  confirmation = ref(''),
  allowed = ref<string[]>([]),
  action = ref<any>(null),
  backup = ref(false);
const output = ref(''),
  outputTitle = ref(''),
  resultOpen = ref(false);
const resultData = ref<Record<string, any> | null>(null),
  resultKind = ref('');
const detailRows = computed(() => {
  const d = resultData.value;
  if (!d) return [];
  return resultKind.value === 'status'
    ? [
        ['Status', d.status],
        ['Container', d.name?.replace(/^\//, '')],
        ['Image', d.image],
      ]
    : [
        ['CPU', d.CPUPerc],
        ['Memory', d.MemUsage],
        ['Memory used', d.MemPerc],
        ['Network received / sent', d.NetIO],
        ['Disk read / written', d.BlockIO],
        ['Processes', d.PIDs],
      ];
});
function readOnly(app: any) {
  return !!servers.value.find((s) => s.id === app.server_id)?.read_only;
}
const selectedProfile = computed(() => profiles.value.find((p) => p.id === profile.value));
watch(profile, () => {
  name.value = selectedProfile.value?.name || '';
  container.value = selectedProfile.value?.container_hint || '';
  if (!selectedProfile.value?.image_repositories.length) advanced.value = false;
});
async function load() {
  [apps.value, profiles.value, servers.value] = await Promise.all([
    props.api('/existing'),
    props.api('/existing/profiles').then((v) => (Array.isArray(v) ? v : Object.values(v))),
    props.api('/servers'),
  ]);
}
onMounted(() => run(load));
function add() {
  clearError();
  profile.value = '';
  name.value = '';
  url.value = '';
  server.value = '';
  container.value = '';
  advanced.value = false;
  addOpen.value = true;
}
async function create() {
  await run(async () => {
    await props.api('/existing', 'POST', {
      profile: profile.value,
      name: name.value,
      url: url.value,
      server_id: advanced.value ? server.value : '',
      container: advanced.value ? container.value : '',
    });
    await load();
    addOpen.value = false;
  });
}
function manage(app: any, mode: string) {
  clearError();
  selected.value = app;
  dialog.value = mode;
  confirmation.value = '';
  allowed.value = [...app.allowed_actions];
  backup.value = false;
}
async function savePermissions() {
  await run(async () => {
    await props.api(`/existing/${selected.value.id}/permissions`, 'PUT', {
      confirmation: confirmation.value,
      allowed_actions: allowed.value,
    });
    await load();
    dialog.value = '';
  });
}
async function unlink() {
  await run(async () => {
    await props.api(`/existing/${selected.value.id}`, 'DELETE', {
      confirmation: confirmation.value,
    });
    await load();
    dialog.value = '';
  });
}
async function show(app: any, path: string, title: string) {
  clearError();
  outputTitle.value = `${app.name}: ${title}`;
  output.value = '';
  resultData.value = null;
  resultKind.value = path;
  resultOpen.value = true;
  await run(async () => {
    const result = await props.api(`/existing/${app.id}/${path}`);
    resultData.value = result;
    output.value = typeof result === 'string' ? result : JSON.stringify(result, null, 2);
  });
}
async function runAction(app: any, item: any) {
  if (item.write) {
    action.value = item;
    manage(app, 'action');
    return;
  }
  await execute(app, item, '');
}
async function execute(app: any, item: any, confirm: string) {
  await run(async () => {
    const value = await props.api(`/existing/${app.id}/actions/${item.id}`, 'POST', {
      confirmation: confirm,
    });
    outputTitle.value = `${app.name}: ${item.label}`;
    resultData.value = null;
    output.value =
      typeof value.output === 'string'
        ? value.output
        : typeof value === 'string'
          ? value
          : JSON.stringify(value, null, 2);
    dialog.value = '';
    resultOpen.value = true;
  });
}
</script>
<template>
  <section aria-labelledby="existing-title" class="existing-apps">
    <header class="section-header">
      <div>
        <h1 id="existing-title">Existing apps</h1>
        <p>Connect services you already run. Their deployment and data stay where they are.</p>
      </div>
      <button class="button primary" @click="add"><Plus :size="17" />Link an app</button>
    </header>
    <p v-if="error && !addOpen && !dialog && !resultOpen" class="monitor-error" role="alert">
      {{ error }}
    </p>
    <p v-if="busy && !apps.length" role="status">Loading your linked apps…</p>
    <EmptyState
      v-else-if="!apps.length"
      title="Your existing services belong here"
      description="Add an app's URL to keep it close, or connect its Docker container for status and management."
      ><button class="button" @click="add">
        <Link :size="16" />Link your first app
      </button></EmptyState
    >
    <div v-else class="existing-grid">
      <article v-for="app in apps" :key="app.id" class="panel existing-card">
        <div class="existing-card-heading">
          <div>
            <h2>{{ app.name }}</h2>
            <p>{{ app.profile.name }}</p>
          </div>
          <span class="ownership">{{
            app.allowed_actions.length ? 'Selected actions enabled' : 'Read-only'
          }}</span>
        </div>
        <a :href="app.url" target="_blank" rel="noopener noreferrer" class="app-address"
          >{{ app.url }}<ExternalLink :size="15" /><span class="sr-only">
            (opens in a new tab)</span
          ></a
        >
        <p v-if="app.container_id" class="connection-description">
          {{ app.container_name }} on
          {{ servers.find((s) => s.id === app.server_id)?.name || app.server_id }}
        </p>
        <p v-else class="connection-description">Linked by URL</p>
        <div class="existing-controls">
          <template v-if="app.container_id"
            ><button
              class="button"
              :disabled="busy"
              @click="show(app, 'status', 'Container status')"
            >
              <RefreshCw :size="15" />Status</button
            ><button class="button" :disabled="busy" @click="show(app, 'stats', 'Resource usage')">
              Usage
            </button></template
          ><button class="button" @click="manage(app, 'unlink')">
            <Unlink :size="15" />Unlink
          </button>
        </div>
        <details v-if="app.container_id && app.profile.actions.length">
          <summary>App actions</summary>
          <p>
            {{
              readOnly(app)
                ? 'This server allows read-only actions. Enable server management before granting app permissions.'
                : 'Changes run only when you enable and confirm the specific action.'
            }}
          </p>
          <div class="existing-controls">
            <button
              v-for="item in app.profile.actions.filter(
                (a: any) => !a.write || app.allowed_actions.includes(a.id),
              )"
              :key="item.id"
              class="button"
              :disabled="busy"
              @click="runAction(app, item)"
            >
              {{ item.label }}</button
            ><button
              v-if="!readOnly(app) && app.profile.actions.some((a: any) => a.write)"
              class="button"
              @click="manage(app, 'permissions')"
            >
              <Settings2 :size="15" />Manage permissions
            </button>
          </div>
        </details>
      </article>
    </div>
  </section>
  <BaseDialog
    :open="addOpen"
    title="Link an existing app"
    description="Start with its web address. Add a Docker connection if you want to inspect the app or enable selected actions."
    :busy="busy"
    @update:open="addOpen = $event"
  >
    <form @submit.prevent="create">
      <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
      <label
        >Application<select v-model="profile" required>
          <option disabled value="">Choose an app</option>
          <option v-for="item in profiles" :key="item.id" :value="item.id">{{ item.name }}</option>
        </select></label
      >
      <p v-if="selectedProfile">{{ selectedProfile.description }}</p>
      <label>Name<input v-model="name" required maxlength="64" autocomplete="off" /></label
      ><label
        >App URL<input
          v-model="url"
          type="url"
          required
          placeholder="https://files.example.com"
          aria-describedby="existing-url-help"
      /></label>
      <p id="existing-url-help" class="field-help">
        Use HTTPS for a domain, or HTTP for localhost.
      </p>
      <label v-if="selectedProfile?.image_repositories.length" class="check-row"
        ><input v-model="advanced" type="checkbox" />Connect its Docker container</label
      ><template v-if="advanced"
        ><label
          >Server<select v-model="server" required>
            <option disabled value="">Choose a connected Docker server</option>
            <option
              v-for="s in servers.filter((s) => s.provider !== 'proxmox_ssh')"
              :key="s.id"
              :value="s.id"
            >
              {{ s.name }}
            </option>
          </select></label
        ><label
          >Container name or ID<input
            v-model="container"
            required
            maxlength="128"
            autocomplete="off"
        /></label>
        <p>
          Selfhost checks the container image and records its identity. If the container is
          replaced, review and link the replacement explicitly.
        </p></template
      ><button class="button primary" :disabled="busy">{{ busy ? 'Linking…' : 'Link app' }}</button>
    </form>
  </BaseDialog>
  <BaseDialog
    :open="!!dialog"
    :title="
      dialog === 'unlink'
        ? 'Unlink app'
        : dialog === 'action'
          ? action?.label || 'Run action'
          : 'Management permissions'
    "
    :description="
      dialog === 'unlink'
        ? 'Remove this link from Selfhost. The running app, its configuration, volumes and backups stay untouched.'
        : 'This app is managed outside Selfhost. Enable only the actions you want Selfhost to perform.'
    "
    :busy="busy"
    @update:open="
      (value) => {
        if (!value) dialog = '';
      }
    "
  >
    <form
      v-if="selected"
      @submit.prevent="
        dialog === 'unlink'
          ? unlink()
          : dialog === 'permissions'
            ? savePermissions()
            : execute(selected, action, confirmation)
      "
    >
      <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
      <template v-if="dialog === 'permissions'"
        ><label
          v-for="item in selected.profile.actions.filter((a: any) => a.write)"
          :key="item.id"
          class="permission-option"
          ><span class="check-row"
            ><input v-model="allowed" type="checkbox" :value="item.id" />{{ item.label }}</span
          ><small>{{ item.description }}</small></label
        ></template
      ><template v-if="dialog === 'action'"
        ><p>{{ action.description }}</p>
        <label class="check-row"
          ><input v-model="backup" type="checkbox" required />I have a recent backup and can restore
          this app.</label
        ></template
      ><label
        >Type <strong>{{ selected.name }}</strong> to confirm<input
          v-model="confirmation"
          autocomplete="off"
          required /></label
      ><button
        class="button"
        :class="{ primary: dialog !== 'unlink' }"
        :disabled="busy || confirmation !== selected.name || (dialog === 'action' && !backup)"
      >
        {{
          dialog === 'unlink'
            ? 'Unlink app'
            : dialog === 'permissions'
              ? 'Save permissions'
              : 'Run action'
        }}
      </button>
    </form>
  </BaseDialog>
  <BaseDialog
    :open="resultOpen"
    :title="outputTitle"
    description="Only this session displays the command output. Review it before sharing, as app output may contain private information."
    :busy="busy"
    @update:open="resultOpen = $event"
    ><p v-if="busy" role="status">Reading app information…</p>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <dl v-if="resultData" class="result-details">
      <div v-for="[label, value] in detailRows" :key="label">
        <dt>{{ label }}</dt>
        <dd>{{ value ?? 'Unavailable' }}</dd>
      </div>
    </dl>
    <details v-if="resultData && output">
      <summary>Full details</summary>
      <pre class="existing-output" tabindex="0">{{ output }}</pre>
    </details>
    <pre v-else-if="output" class="existing-output" tabindex="0">{{ output }}</pre>
  </BaseDialog>
</template>
<style scoped>
.result-details {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}
.result-details div {
  min-width: 0;
}
.result-details dt {
  font-size: 12px;
  color: var(--muted);
  margin-bottom: 6px;
}
.result-details dd {
  margin: 0;
  overflow-wrap: anywhere;
  font-size: 16px;
}
.existing-apps > .section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 24px;
}
.section-header p,
.connection-description,
.existing-card-heading p,
.field-help {
  color: var(--muted);
  line-height: 1.6;
}
.existing-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(380px, 100%), 1fr));
  gap: 20px;
}
.existing-card {
  padding: 24px;
}
.existing-card-heading {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}
.existing-card h2 {
  margin: 0;
  font-size: 19px;
}
.existing-card-heading p {
  margin: 6px 0 18px;
}
.ownership {
  padding: 5px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: 11px;
  white-space: nowrap;
  color: var(--muted);
}
.app-address {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  word-break: break-all;
  color: var(--green);
}
.existing-controls {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 16px;
}
.existing-controls .button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.existing-card details {
  margin-top: 22px;
  border-top: 1px solid var(--border);
  padding-top: 18px;
}
.existing-card summary {
  cursor: pointer;
  min-height: 28px;
}
.existing-card details p {
  font-size: 13px;
  line-height: 1.6;
  color: var(--muted);
}
.check-row {
  display: flex !important;
  gap: 10px;
  align-items: flex-start;
  line-height: 1.6;
}
.check-row input {
  flex-shrink: 0;
  width: 18px !important;
  height: 18px;
  margin: 3px 0 !important;
}
.permission-option small {
  display: block;
  margin: 5px 0 16px 28px;
  color: var(--muted);
  line-height: 1.6;
}
.existing-output {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 50vh;
  overflow: auto;
  background: #1b211c;
  padding: 18px;
  border-radius: 8px;
  font-size: 12px;
}
@media (max-width: 600px) {
  .existing-apps > .section-header {
    align-items: flex-start;
    flex-direction: column;
  }
  .existing-card-heading {
    flex-direction: column;
    gap: 4px;
  }
  .ownership {
    margin-bottom: 12px;
  }
}
</style>
