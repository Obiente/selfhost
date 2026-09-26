<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { Download, Eye, EyeOff, Plus, Save, Trash2 } from 'lucide-vue-next';
const props = defineProps<{
  project?: { id: string; name: string; server_id: string; services?: any[] };
  servers: { id: string; name: string; provider: string; read_only: boolean }[];
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  download: (id: string) => Promise<void>;
  downloadBackup: (id: string, backup: string) => Promise<void>;
}>();
const emit = defineEmits<{ saved: [project: any]; cancel: [] }>();
const name = ref(props.project?.name || ''),
  server = ref(props.project?.server_id || 'local');
const compose = ref('services: {}\n');
const environment = ref<{ name: string; value: string }[]>([]),
  files = ref<{ name: string; content: string }[]>([]);
const reveal = ref(false),
  busy = ref(false),
  error = ref(''),
  notice = ref('');
const database = ref<any>(null),
  engines = ref<any[]>([]),
  engine = ref(''),
  sources = ref<any[]>([]),
  mode = ref('dedicated'),
  source = ref('');
const backups = ref<
    { id: string; created_at: number; bytes: number; engine?: string; format?: string }[]
  >([]),
  restoreId = ref(''),
  confirmProject = ref('');
const savedDraft = ref('');
const draft = () =>
  JSON.stringify({ compose: compose.value, environment: environment.value, files: files.value });
const dirty = computed(() => draft() !== savedDraft.value);
const compatibleEngines = computed(() => {
  const recipes = (props.project?.services || []).flatMap((service) =>
    service.definition?.database ? [service.definition.database] : [],
  );
  return engines.value.filter((driver) =>
    recipes.every((recipe) => [recipe.engine, ...(recipe.engines || [])].includes(driver.engine)),
  );
});
const defaultEngine = () => {
  const declared = props.project?.services?.find((service) => service.definition?.database)
    ?.definition.database.engine;
  return (
    compatibleEngines.value.find((driver) => driver.engine === declared)?.engine ||
    compatibleEngines.value[0]?.engine ||
    ''
  );
};
const matchingSources = computed(() =>
  sources.value.filter((item) => {
    const selectedEngine = item.engine || 'postgres';
    return (
      item.kind === mode.value &&
      compatibleEngines.value.some((driver) => driver.engine === selectedEngine) &&
      (props.project?.services || []).every((service) => {
        const modes = service.definition?.database?.engine_ssl_modes?.[selectedEngine];
        return !modes || modes.includes(item.ssl_mode);
      })
    );
  }),
);
watch(mode, () => {
  source.value = '';
  if (mode.value === 'dedicated') engine.value = defaultEngine();
});
watch(source, () => {
  if (mode.value !== 'dedicated')
    engine.value =
      matchingSources.value.find((item) => item.id === source.value)?.engine || 'postgres';
});
async function load() {
  if (!props.project) return;
  const data = await props.api(`/projects/${props.project.id}/setup`);
  compose.value = JSON.stringify(data.compose, null, 2);
  environment.value = Object.entries(data.environment).map(([name, value]) => ({
    name,
    value: String(value),
  }));
  files.value = Object.entries(data.files).map(([name, content]) => ({
    name,
    content: String(content),
  }));
  database.value = data.database;
  savedDraft.value = draft();
  backups.value = await props.api(`/projects/${props.project.id}/database/backups`);
}
async function save() {
  busy.value = true;
  error.value = '';
  notice.value = '';
  try {
    if (
      new Set(environment.value.map((e) => e.name)).size !== environment.value.length ||
      new Set(files.value.map((f) => f.name)).size !== files.value.length
    )
      throw new Error('Names must be unique.');
    const parsed = await props.api('/compose/parse', 'POST', { text: compose.value });
    const setup = {
      compose: parsed,
      environment: Object.fromEntries(environment.value.map((e) => [e.name, e.value])),
      files: Object.fromEntries(files.value.map((f) => [f.name, f.content])),
      database: database.value,
    };
    const saved = props.project
      ? await props.api(`/projects/${props.project.id}/setup`, 'PUT', setup)
      : await props.api('/setups', 'POST', { name: name.value, server_id: server.value, setup });
    savedDraft.value = draft();
    notice.value = 'Saved. Start the project to apply changes.';
    emit('saved', saved);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function attach() {
  if (!props.project || dirty.value) return;
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/projects/${props.project.id}/database`, 'POST', {
      mode: mode.value,
      source_id: mode.value === 'dedicated' ? null : source.value,
      engine: engine.value || null,
    });
    await load();
    notice.value = 'Database settings saved. Map DATABASE_* variables into your app’s environment.';
    emit('saved', props.project);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function provision() {
  if (!props.project || dirty.value) return;
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/projects/${props.project.id}/database/provision`, 'POST', {});
    await load();
    notice.value = 'Project database created.';
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function backup() {
  if (!props.project || dirty.value) return;
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/projects/${props.project.id}/database/backups`, 'POST', {});
    backups.value = await props.api(`/projects/${props.project.id}/database/backups`);
    notice.value = 'Database backup saved.';
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function restoreBackup() {
  if (!props.project || confirmProject.value !== props.project.id || dirty.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await props.api(
      `/projects/${props.project.id}/database/backups/${restoreId.value}/restore`,
      'POST',
      { confirm_project: confirmProject.value },
    );
    notice.value = `Database restored. Safety backup: ${result.safety_backup.id}`;
    restoreId.value = '';
    confirmProject.value = '';
    backups.value = await props.api(`/projects/${props.project.id}/database/backups`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function downloadArchive(id: string) {
  if (!props.project) return;
  try {
    await props.downloadBackup(props.project.id, id);
  } catch (e) {
    error.value = (e as Error).message;
  }
}
async function download() {
  if (!props.project) return;
  try {
    await props.download(props.project.id);
  } catch (e) {
    error.value = (e as Error).message;
  }
}
onMounted(async () => {
  try {
    await load();
    [sources.value, engines.value] = await Promise.all([
      props.api('/databases'),
      props.api('/database-engines'),
    ]);
    engine.value = defaultEngine();
  } catch (e) {
    error.value = (e as Error).message;
  }
});
</script>
<template>
  <section class="setup-editor">
    <div class="section-heading">
      <h2>{{ project ? 'Setup configuration' : 'Custom setup' }}</h2>
      <button class="text-button" @click="emit('cancel')">Close</button>
    </div>
    <p class="form-help">
      Use ordinary Docker Compose YAML or JSON. Your setup can run with Docker Compose independently
      of selfhost.
    </p>
    <form @submit.prevent="save">
      <template v-if="!project"
        ><label>Project name<input v-model="name" required maxlength="64" /></label
        ><label
          >Server<select v-model="server">
            <option
              v-for="s in servers.filter((s) => s.provider !== 'proxmox_ssh')"
              :key="s.id"
              :value="s.id"
            >
              {{ s.name }}{{ s.read_only ? ' (read-only)' : '' }}
            </option>
          </select></label
        ></template
      >
      <label
        >Compose configuration<textarea
          v-model="compose"
          class="code-editor"
          rows="18"
          spellcheck="false"
          required
          aria-label="Compose configuration"
        />
      </label>
      <div class="section-heading">
        <h3>Environment</h3>
        <button type="button" class="text-button" @click="reveal = !reveal">
          <EyeOff v-if="reveal" :size="16" /><Eye v-else :size="16" />{{
            reveal ? 'Hide values' : 'Reveal values'
          }}
        </button>
      </div>
      <p class="form-help">
        Reference a value in Compose with <code>${VARIABLE_NAME}</code>. Values are saved privately
        and included in exports.
      </p>
      <div v-for="(item, index) in environment" :key="index" class="env-row">
        <input
          v-model="item.name"
          aria-label="Variable name"
          placeholder="VARIABLE_NAME"
          required
        /><input
          v-model="item.value"
          :type="reveal ? 'text' : 'password'"
          autocomplete="off"
          aria-label="Variable value"
        /><button
          type="button"
          class="icon-button"
          aria-label="Remove variable"
          @click="environment.splice(index, 1)"
        >
          <Trash2 :size="16" />
        </button>
      </div>
      <button type="button" class="button small" @click="environment.push({ name: '', value: '' })">
        <Plus :size="16" />Add variable
      </button>
      <h3 class="setup-section-title">Configuration files</h3>
      <p class="form-help">
        Mount files using <code>./files/NAME:/container/path:ro</code>. Scripts can be mounted the
        same way and run by a service command or an initialization container.
      </p>
      <div v-for="(file, index) in files" :key="index" class="file-editor">
        <div class="env-row">
          <input
            v-model="file.name"
            aria-label="Config filename"
            placeholder="app.yaml"
            required
          /><button
            type="button"
            class="icon-button"
            aria-label="Remove config file"
            @click="files.splice(index, 1)"
          >
            <Trash2 :size="16" />
          </button>
        </div>
        <textarea
          v-model="file.content"
          class="code-editor"
          rows="8"
          spellcheck="false"
          aria-label="Config file contents"
        />
      </div>
      <button type="button" class="button small" @click="files.push({ name: '', content: '' })">
        <Plus :size="16" />Add file
      </button>
      <div class="form-footer">
        <p>
          Saving does not restart services. A snapshot is saved before changes to an existing
          project.
        </p>
        <button class="button primary" :disabled="busy" type="submit">
          <Save :size="16" />Save setup
        </button>
      </div>
    </form>
    <section v-if="project" class="database-attachment">
      <h3>Project database</h3>
      <p v-if="dirty" class="form-help">
        Save your configuration changes before configuring or provisioning a database.
      </p>
      <p class="form-help">
        Dedicated containers keep data in their own volume. Shared servers use a separate database
        and role for each project. External sources use the database and account you supply.
      </p>
      <template v-if="database"
        ><p>
          {{ database.engine || 'postgres' }} · {{ database.mode }} database ·
          {{
            database.provisioning_uncertain
              ? 'Needs inspection before recovery'
              : database.provisioned
                ? 'Configured'
                : 'Awaiting provisioning'
          }}
        </p>
        <button
          v-if="!database.provisioned && !database.provisioning_uncertain"
          class="button"
          :disabled="busy || dirty"
          @click="provision"
        >
          Create database and user
        </button>
        <p class="form-help">
          Changing database placement requires transferring its data first.
        </p></template
      >
      <template v-else
        ><label v-if="mode === 'dedicated'"
          >Database engine<select v-model="engine" required>
            <option v-for="item in compatibleEngines" :key="item.engine" :value="item.engine">
              {{ item.name || item.engine }}
            </option>
          </select></label
        ><label
          >Database placement<select v-model="mode">
            <option value="dedicated">Separate container for this project</option>
            <option value="shared">Shared database server</option>
            <option value="external">External database</option>
          </select></label
        ><label v-if="mode !== 'dedicated'"
          >Database source<select v-model="source">
            <option disabled value="">Choose a source</option>
            <option v-for="s in matchingSources" :key="s.id" :value="s.id">
              {{ s.name }} ({{ s.engine || 'postgres' }})
            </option>
          </select></label
        >
        <p class="form-help">
          Save pending edits above before adding a database. Manage sources on the Databases page.
        </p>
        <p v-if="mode !== 'dedicated' && !matchingSources.length" class="form-help">
          No sources match this project's database engines and TLS settings. Add a compatible source
          on the Databases page.
        </p>
        <button
          class="button"
          :disabled="busy || dirty || !engine || (mode !== 'dedicated' && !source)"
          @click="attach"
        >
          Configure database
        </button></template
      >
    </section>
    <section v-if="project && database?.provisioned" class="database-attachment">
      <h3>Database backups</h3>
      <p class="form-help">
        Back up this project's database using its engine's native archive format. Archives are
        stored privately on this computer, separately from setup exports. Keep a copy on another
        device.
      </p>
      <button class="button" :disabled="busy || dirty" @click="backup">Back up database</button>
      <p v-if="database.engine === 'mongodb'" class="form-help">
        Stop app and external database writers before backing up MongoDB when you need a consistent
        snapshot across collections.
      </p>
      <p v-if="['mysql', 'mariadb'].includes(database.engine)" class="form-help">
        SQL backups take table read locks and can temporarily block writes.
      </p>
      <p v-if="database.engine && database.engine !== 'postgres'" class="form-help">
        Restore can partially apply if it fails. Selfhost creates a safety backup first and keeps it
        available for recovery.
      </p>
      <p v-if="!backups.length" class="form-help">No database backups yet.</p>
      <div v-for="item in backups" :key="item.id" class="backup-row">
        <span
          >{{ new Date(item.created_at * 1000).toLocaleString() }} ·
          {{ (item.bytes / 1024).toFixed(1) }} KiB · {{ item.engine || 'postgres' }} ·
          {{ item.format || 'postgres-custom' }}</span
        ><button class="text-button" :disabled="busy" @click="downloadArchive(item.id)">
          Download</button
        ><button
          class="text-button"
          :disabled="busy || dirty"
          @click="
            restoreId = item.id;
            confirmProject = '';
          "
        >
          Restore {{ item.id }}
        </button>
      </div>
      <form v-if="restoreId" @submit.prevent="restoreBackup">
        <h4>Restore database</h4>
        <p class="form-help">
          This replaces matching database objects with the selected backup. Stop this project's apps
          first and keep the database running. Stop any external writers too. A safety backup is
          saved before restoring. Apps remain stopped afterward.
        </p>
        <label
          >Enter {{ project.id }} to confirm<input
            v-model="confirmProject"
            autocomplete="off" /></label
        ><button
          class="button"
          type="submit"
          :disabled="busy || dirty || confirmProject !== project.id"
        >
          Restore database</button
        ><button class="text-button" type="button" :disabled="busy" @click="restoreId = ''">
          Cancel
        </button>
      </form>
    </section>
    <section v-if="project" class="export-section">
      <h3>Take your setup with you</h3>
      <p class="form-help">
        Download the saved Compose file, environment and configuration files. The archive contains
        credentials. Database contents and Docker volume data need a separate backup.
      </p>
      <button class="button" @click="download">
        <Download :size="16" />Export standalone setup
      </button>
    </section>
    <p v-if="notice" role="status" class="setup-notice">{{ notice }}</p>
    <p v-if="error" role="alert" class="setup-error">{{ error }}</p>
  </section>
</template>
<style scoped>
.setup-editor {
  max-width: 1000px;
}
.backup-row {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  margin: 16px 0;
}
.code-editor {
  box-sizing: border-box;
  width: 100%;
  resize: vertical;
  padding: 16px;
  background: var(--paper);
  color: inherit;
  border: 1px solid var(--border);
  border-radius: 6px;
  font: 13px/1.6 monospace;
}
.setup-editor label {
  display: block;
  margin: 18px 0;
}
.env-row {
  display: flex;
  gap: 12px;
  margin: 12px 0;
}
.env-row input {
  min-width: 0;
  flex: 1;
}
.file-editor {
  margin: 18px 0;
}
.setup-section-title,
.database-attachment,
.export-section {
  margin-top: 32px;
}
.database-attachment,
.export-section {
  padding-top: 24px;
  border-top: 1px solid var(--border);
}
.form-help {
  line-height: 1.7;
  margin: 12px 0;
}
.setup-error {
  color: #ffb7a2;
  margin: 18px 0;
}
.setup-notice {
  color: var(--green);
  margin: 18px 0;
}
@media (max-width: 640px) {
  .env-row {
    flex-wrap: wrap;
  }
  .env-row input {
    min-width: 150px;
  }
}
</style>
