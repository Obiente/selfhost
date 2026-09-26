<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import DatabaseSourceManager from './DatabaseSourceManager.vue';
import { Database, Plus, Play } from 'lucide-vue-next';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  servers: { id: string; name: string; provider: string }[];
}>();
const emit = defineEmits<{ changed: [] }>();
const selected = ref<any>(null),
  mode = ref<'edit' | 'rename' | 'remove'>('edit'),
  status = ref('');
function manage(source: any, action: 'edit' | 'rename' | 'remove') {
  selected.value = source;
  mode.value = action;
}
async function tested(source: any) {
  busy.value = true;
  error.value = '';
  status.value = '';
  try {
    await props.api(`/databases/${source.id}/test`, 'POST', {});
    status.value = `Connected to ${source.name}.`;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function reload() {
  try {
    await load();
    emit('changed');
  } catch (e) {
    error.value = (e as Error).message;
  }
}
const sources = ref<any[]>([]),
  engines = ref<any[]>([]),
  adding = ref(false),
  error = ref(''),
  busy = ref(false);
const form = reactive({
  name: '',
  engine: 'postgres',
  auth_database: '',
  kind: 'shared',
  managed: true,
  server_id: 'local',
  host: '',
  port: 5432,
  username: '',
  password: '',
  database: '',
  ssl_mode: 'require',
});
async function load() {
  [sources.value, engines.value] = await Promise.all([
    props.api('/databases'),
    props.api('/database-engines'),
  ]);
}
const driver = computed(() => engines.value.find((item) => item.engine === form.engine));
watch(
  () => form.engine,
  () => {
    form.port = driver.value?.port || 0;
    form.auth_database = driver.value?.auth_database || '';
  },
);
async function save() {
  busy.value = true;
  error.value = '';
  try {
    await props.api('/databases', 'POST', {
      ...form,
      managed: form.kind === 'shared' && form.managed,
    });
    form.password = '';
    adding.value = false;
    await load();
    emit('changed');
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function start(source: any) {
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/databases/${source.id}/start`, 'POST', {});
    emit('changed');
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
onMounted(() =>
  load().catch((e) => {
    error.value = e.message;
  }),
);
</script>
<template>
  <section>
    <div class="section-heading">
      <h2>Database sources</h2>
      <button class="button primary" @click="adding = !adding">
        <Plus :size="16" />Add source
      </button>
    </div>
    <p class="form-help">
      Connect an existing database or host a shared server. Projects can also create their own
      dedicated database container from setup settings.
    </p>
    <form v-if="adding" class="settings-form" @submit.prevent="save">
      <label
        >Database engine<select v-model="form.engine" required>
          <option v-for="item in engines" :key="item.engine" :value="item.engine">
            {{ item.name || item.engine }}
          </option>
        </select></label
      >
      <label>Name<input v-model="form.name" maxlength="50" required /></label
      ><label
        >Use<select v-model="form.kind">
          <option value="shared">Shared server with a database per project</option>
          <option value="external">Existing database and account</option>
        </select></label
      ><label v-if="form.kind === 'shared'" class="check-line"
        ><input v-model="form.managed" type="checkbox" />Host this database server with
        selfhost</label
      ><label
        >Docker server<select v-model="form.server_id">
          <option
            v-for="s in servers.filter((s) => s.provider !== 'proxmox_ssh')"
            :key="s.id"
            :value="s.id"
          >
            {{ s.name }}
          </option>
        </select></label
      >
      <template v-if="form.kind === 'external' || !form.managed"
        ><label>Database hostname<input v-model="form.host" required /></label
        ><label
          >Port<input
            v-model.number="form.port"
            type="number"
            min="1"
            max="65535"
            required /></label
        ><label>Database name<input v-model="form.database" required /></label
        ><label v-if="driver?.auth_database"
          >Authentication database<input v-model="form.auth_database" required /></label
        ><label>Username<input v-model="form.username" autocomplete="off" required /></label
        ><label
          >Password<input
            v-model="form.password"
            type="password"
            autocomplete="new-password"
            required /></label
        ><label
          >TLS<select v-model="form.ssl_mode">
            <option value="require">Required</option>
            <option value="verify-full">Verify certificate and hostname</option>
            <option value="disable">Disabled</option>
          </select></label
        >
        <p v-if="form.kind === 'shared'" class="form-help">
          This account needs permission to create databases and roles. Projects receive their own
          credentials.
        </p></template
      >
      <p v-else class="form-help">
        Creates a separate project for the database server with persistent storage and no published
        host port. Start it when ready, then select it from your app projects.
      </p>
      <button class="button primary" :disabled="busy">Save source</button>
    </form>
    <p v-if="status" role="status">{{ status }}</p>
    <div class="catalog-grid">
      <article v-for="source in sources" :key="source.id" class="catalog-card">
        <div class="catalog-card-top">
          <Database :size="28" /><span class="category">{{
            engines.find((item) => item.engine === (source.engine || 'postgres'))?.name ||
            source.engine ||
            'PostgreSQL'
          }}</span>
        </div>
        <h2>{{ source.name }}</h2>
        <p>
          {{ source.kind === 'shared' ? 'Shared server' : 'External database' }} ·
          {{ source.host }}:{{ source.port }}
        </p>
        <p v-if="source.managed_project">Managed as its own project</p>
        <div class="source-actions">
          <button class="button" :disabled="busy" @click="tested(source)">Test connection</button>
          <button
            v-if="!source.managed_project"
            class="button"
            :disabled="busy"
            @click="manage(source, 'edit')"
          >
            Edit connection
          </button>
          <button class="button" :disabled="busy" @click="manage(source, 'rename')">Rename</button>
          <button class="button danger" :disabled="busy" @click="manage(source, 'remove')">
            Remove source
          </button>
        </div>
        <button
          v-if="source.managed_project"
          class="button"
          :disabled="busy"
          @click="start(source)"
        >
          <Play :size="16" />Start database server
        </button>
      </article>
    </div>
    <div v-if="!sources.length && !adding" class="empty">
      <Database :size="32" />
      <h3>No database sources yet</h3>
      <p>Add a server here or choose a dedicated database in a project’s setup.</p>
    </div>
    <DatabaseSourceManager
      v-if="selected"
      :source="selected"
      :mode="mode"
      :api="api"
      @close="selected = null"
      @saved="reload"
    />
    <p v-if="error" role="alert">{{ error }}</p>
  </section>
</template>

<style scoped>
.source-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 14px 0;
}
</style>
