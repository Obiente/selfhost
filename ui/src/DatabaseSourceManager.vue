<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import BaseDialog from './components/BaseDialog.vue';
const props = defineProps<{
  source: any;
  mode: 'edit' | 'rename' | 'remove';
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const emit = defineEmits<{ close: []; saved: [] }>();
const checked = ref(false);
const busy = ref(false),
  loading = ref(true),
  error = ref(''),
  references = ref<string[]>([]),
  confirmation = ref('');
const source = props.source;
const form = reactive({
  id: source.id,
  name: source.name,
  engine: source.engine || 'postgres',
  auth_database: source.auth_database || '',
  kind: source.kind,
  server_id: source.server_id,
  host: source.host,
  port: source.port,
  username: source.username,
  password: '',
  database: source.database,
  ssl_mode: source.ssl_mode,
  network: source.network ?? null,
  managed_project: source.managed_project ?? null,
});
const path = `/databases/${encodeURIComponent(source.id)}`;
const destinationLocked = computed(() => references.value.length > 0);
const credentialsLocked = computed(() => destinationLocked.value && source.kind !== 'shared');
onMounted(async () => {
  try {
    references.value = await props.api(`${path}/references`);
    checked.value = true;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    loading.value = false;
  }
});
async function save() {
  if (busy.value || loading.value || !checked.value) return;
  if (props.mode === 'remove' && (destinationLocked.value || confirmation.value !== source.name))
    return;
  busy.value = true;
  error.value = '';
  try {
    if (props.mode === 'remove')
      await props.api(path, 'DELETE', { confirmation: confirmation.value });
    else if (props.mode === 'rename')
      await props.api(`${path}/rename`, 'POST', { name: form.name });
    else await props.api(path, 'PUT', { ...form });
    form.password = '';
    emit('saved');
    emit('close');
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <BaseDialog
    :open="true"
    :busy="busy"
    :title="`${mode === 'remove' ? 'Remove' : mode === 'rename' ? 'Rename' : 'Edit'} database source`"
    @update:open="emit('close')"
  >
    <p v-if="loading" role="status">Checking projects that use this source…</p>
    <p v-if="references.length" class="form-help">
      Used by {{ references.length }} {{ references.length === 1 ? 'project' : 'projects' }}. Its
      destination cannot change while projects use it.
    </p>
    <form @submit.prevent="save">
      <fieldset :disabled="busy || loading || !checked || (mode === 'remove' && destinationLocked)">
        <template v-if="mode === 'remove'">
          <p v-if="destinationLocked" role="alert">
            Change these projects to another source before removing this record:
            {{ references.join(', ') }}.
          </p>
          <p>Removes the saved connection. Databases, accounts and their data remain in place.</p>
          <p v-if="source.managed_project">
            The hosted database project and its storage remain available under Projects.
          </p>
          <label
            >Type {{ source.name }} to confirm<input
              v-model="confirmation"
              required
              autocomplete="off"
          /></label>
        </template>
        <template v-else>
          <label>Name<input v-model="form.name" required maxlength="50" /></label>
          <template v-if="mode === 'edit'">
            <fieldset :disabled="destinationLocked">
              <legend>Connection</legend>
              <label>Database hostname<input v-model="form.host" required /></label>
              <label
                >Port<input v-model.number="form.port" type="number" min="1" max="65535" required
              /></label>
              <label>Database name<input v-model="form.database" required /></label>
              <label v-if="form.engine === 'mongodb'"
                >Authentication database<input v-model="form.auth_database" required
              /></label>
              <label
                >TLS<select v-model="form.ssl_mode">
                  <option value="require">Required</option>
                  <option value="verify-full">Verify certificate and hostname</option>
                  <option value="disable">Disabled</option>
                </select></label
              >
            </fieldset>
            <fieldset :disabled="credentialsLocked">
              <legend>Account</legend>
              <label>Username<input v-model="form.username" required autocomplete="off" /></label>
              <label
                >Replacement password<input
                  v-model="form.password"
                  type="password"
                  autocomplete="new-password"
              /></label>
              <p class="form-help">
                Leave the password blank to keep the saved password. Enter credentials that already
                work on the database server.
              </p>
              <p v-if="credentialsLocked" class="form-help">
                This external account is used by existing projects and cannot be replaced here.
              </p>
            </fieldset>
            <p class="form-help">Selfhost tests the connection before saving.</p>
          </template>
        </template>
        <button
          class="button"
          :class="mode === 'remove' ? 'danger' : 'primary'"
          :disabled="
            busy ||
            loading ||
            !checked ||
            (mode === 'remove' && (destinationLocked || confirmation !== source.name))
          "
        >
          {{
            busy
              ? 'Working…'
              : mode === 'remove'
                ? 'Remove source record'
                : mode === 'rename'
                  ? 'Save name'
                  : 'Test and save'
          }}
        </button>
      </fieldset>
    </form>
    <p v-if="error" role="alert">{{ error }}</p>
  </BaseDialog>
</template>
<style scoped>
fieldset {
  border: 0;
  padding: 0;
  margin: 16px 0;
  min-width: 0;
}
legend {
  font-weight: 600;
}
label {
  display: block;
  margin: 14px 0;
}
input,
select {
  display: block;
  width: 100%;
  margin-top: 6px;
  box-sizing: border-box;
}
</style>
