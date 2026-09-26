<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import BaseDialog from './components/BaseDialog.vue';
import { useAsyncTask } from './composables/useAsyncTask';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  projects: { id: string; name: string }[];
}>();
const { busy, error, run } = useAsyncTask();
const rules = ref<any[]>([]),
  destinations = ref<any[]>([]),
  existing = ref<any[]>([]);
const editorOpen = ref(false),
  review = ref<any>(null),
  destination = ref(''),
  removal = ref<any>(null),
  confirmation = ref('');
const form = reactive({
  name: '',
  kind: 'dashboard_links',
  managed: true,
  existing: false,
  project_ids: [] as string[],
  existing_ids: [] as string[],
  interval_seconds: 300,
  action: '',
});
const eligible = computed(() =>
  destinations.value.filter((d) =>
    form.kind === 'dashboard_links' ? d.configured && d.supports_sync : d.actions?.length,
  ),
);
const selected = computed(() =>
  eligible.value.find((d) => `${d.project_id}:${d.service}` === destination.value),
);
function payload() {
  return {
    ...form,
    ...(form.kind === 'service_action'
      ? { managed: false, existing: false, project_ids: [], existing_ids: [] }
      : {}),
    destination: {
      project_id: selected.value?.project_id || '',
      service: selected.value?.service || '',
    },
  };
}
async function load() {
  [rules.value, destinations.value, existing.value] = await Promise.all([
    props.api('/tasks'),
    props.api('/tasks/destinations'),
    props.api('/existing'),
  ]);
}
onMounted(() => run(load));
watch(
  [form, destination],
  () => {
    review.value = null;
  },
  { deep: true },
);
watch(
  () => form.kind,
  () => {
    destination.value = '';
    form.action = '';
  },
);
function start() {
  review.value = null;
  editorOpen.value = true;
}
function plan() {
  return run(async () => {
    review.value = await props.api('/tasks/plan', 'POST', payload());
  });
}
function create() {
  if (!review.value) return;
  return run(async () => {
    await props.api('/tasks', 'POST', { request: payload(), revision: review.value.revision });
    await load();
    editorOpen.value = false;
  });
}
function toggle(rule: any) {
  return run(async () => {
    await props.api(`/tasks/${rule.id}/enabled`, 'POST', { enabled: !rule.enabled });
    await load();
  });
}
function execute(rule: any) {
  return run(async () => {
    try {
      await props.api(`/tasks/${rule.id}/run`, 'POST', {});
    } finally {
      await load();
    }
  });
}
function remove() {
  if (!removal.value) return;
  return run(async () => {
    await props.api(`/tasks/${removal.value.id}`, 'DELETE', { confirmation: confirmation.value });
    removal.value = null;
    confirmation.value = '';
    await load();
  });
}
function date(value: number) {
  return value ? new Date(value * 1000).toLocaleString() : 'Not run yet';
}
</script>
<template>
  <section class="tasks" :aria-busy="busy">
    <header class="task-heading">
      <div>
        <h2>Automation tasks</h2>
      </div>
      <button class="button primary" :disabled="busy" @click="start">Add a task</button>
    </header>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <p class="task-note">
      Enabled tasks run while the Selfhost server is running. You can also run a task from the CLI
      without opening the dashboard.
    </p>
    <div v-if="!rules.length" class="panel empty-state">
      <h2>No tasks yet</h2>
      <p>
        Connect a dashboard through app setup, then choose which projects and existing apps it
        should include.
      </p>
    </div>
    <article v-for="rule in rules" :key="rule.id" class="panel task-card">
      <div class="task-heading">
        <div>
          <h2>{{ rule.request.name }}</h2>
          <p>
            {{ rule.request.kind === 'service_action' ? 'Service action' : 'Dashboard links' }} ·
            {{ rule.request.destination.service }}
          </p>
        </div>
        <span class="task-status">{{ rule.enabled ? rule.status : 'Paused' }}</span>
      </div>
      <dl>
        <div>
          <dt>Last run</dt>
          <dd>{{ date(rule.last_run) }}</dd>
        </div>
        <div>
          <dt>Check interval</dt>
          <dd>{{ Math.round(rule.request.interval_seconds / 60) }} minutes</dd>
        </div>
      </dl>
      <p
        v-if="rule.pending || rule.status === 'blocked' || rule.status === 'uncertain'"
        role="status"
      >
        This task needs inspection before it can continue. Check its history and the destination app
        to avoid repeating a change.
      </p>
      <div class="task-actions">
        <button
          class="button"
          :disabled="busy || (!rule.enabled && !!rule.pending)"
          @click="toggle(rule)"
        >
          {{ rule.enabled ? 'Pause' : 'Enable' }}</button
        ><button
          class="button"
          :disabled="busy || !rule.enabled || !!rule.pending"
          @click="execute(rule)"
        >
          Run now</button
        ><button
          class="text-button"
          :disabled="busy"
          @click="
            removal = rule;
            confirmation = '';
          "
        >
          Remove task
        </button>
      </div>
      <details>
        <summary>Run history ({{ rule.history?.length || 0 }})</summary>
        <p v-if="!rule.history?.length">This task has not run yet.</p>
        <ol v-else>
          <li v-for="entry in rule.history" :key="entry.id">
            <strong>{{ entry.status }}</strong> · {{ date(entry.started_at) }}
            <p>{{ entry.message }}</p>
            <span v-if="entry.count">{{ entry.count }} links added</span>
          </li>
        </ol>
      </details>
    </article>
    <BaseDialog
      v-model:open="editorOpen"
      title="Create a task"
      description="Choose its destination and scope, then review what it can change."
      :busy="busy"
      wide
    >
      <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
      <form @submit.prevent="plan">
        <fieldset :disabled="busy">
          <label
            >Task name<input
              v-model="form.name"
              required
              maxlength="100"
              placeholder="Keep my home dashboard up to date"
          /></label>
          <label
            >Task type<select v-model="form.kind">
              <option value="dashboard_links">Add service links to a dashboard</option>
              <option value="service_action">Run a service action</option>
            </select></label
          >
          <label
            >Destination<select v-model="destination" required>
              <option value="" disabled>Choose a configured app</option>
              <option
                v-for="item in eligible"
                :key="`${item.project_id}:${item.service}`"
                :value="`${item.project_id}:${item.service}`"
              >
                {{ item.name }} · {{ item.service }}
              </option>
            </select></label
          >
          <p v-if="!eligible.length">
            No compatible destination is configured. Complete app setup or choose a service with a
            supported action first.
          </p>
          <label v-if="form.kind === 'service_action'"
            >Action<select v-model="form.action" required>
              <option value="" disabled>Choose an action</option>
              <option
                v-for="item in selected?.actions || []"
                :key="item.id || item"
                :value="item.id || item"
              >
                {{ item.label || item }}
              </option>
            </select></label
          >
          <template v-else>
            <label class="check"
              ><input v-model="form.managed" type="checkbox" /> Include Selfhost projects</label
            >
            <label class="check"
              ><input v-model="form.existing" type="checkbox" /> Include linked existing apps</label
            >
            <p>
              Newly added apps in these groups will be included automatically. Existing links are
              kept.
            </p>
            <details>
              <summary>Limit which apps are included</summary>
              <p>
                Leave a group unselected to include all current and future entries in that group.
              </p>
              <fieldset v-if="form.managed">
                <legend>Selected projects</legend>
                <label v-for="project in projects" :key="project.id" class="check"
                  ><input v-model="form.project_ids" type="checkbox" :value="project.id" />{{
                    project.name
                  }}</label
                >
              </fieldset>
              <fieldset v-if="form.existing">
                <legend>Selected existing apps</legend>
                <label v-for="app in existing" :key="app.id" class="check"
                  ><input v-model="form.existing_ids" type="checkbox" :value="app.id" />{{
                    app.name
                  }}</label
                >
              </fieldset>
            </details>
          </template>
          <label
            >Check every (seconds)<input
              v-model.number="form.interval_seconds"
              type="number"
              min="60"
              step="60"
              required
          /></label>
          <button class="button" :disabled="!selected">Review task</button>
        </fieldset>
      </form>
      <section v-if="review" class="task-review">
        <h3>Review task</h3>
        <pre v-if="review.action" tabindex="0">{{ JSON.stringify(review.action, null, 2) }}</pre>
        <p>
          <strong>{{ form.name }}</strong> will use {{ selected?.name }} · {{ selected?.service }}.
        </p>
        <p v-if="form.kind === 'dashboard_links'">
          {{ review.apps?.length || 0 }} service links are eligible now. The selected scope also
          controls future additions.
        </p>
        <ul>
          <li v-for="warning in review.warnings || []" :key="warning">{{ warning }}</li>
        </ul>
        <details v-if="review.apps?.length">
          <summary>Review addresses</summary>
          <ul>
            <li v-for="app in review.apps" :key="app.url">
              {{ app.name }}: <code>{{ app.url }}</code>
            </li>
          </ul>
        </details>
        <button class="button primary" :disabled="busy" @click="create">
          Create reviewed task
        </button>
      </section>
    </BaseDialog>
    <BaseDialog
      :open="!!removal"
      title="Remove task"
      description="This removes the automation rule. Apps and links already created by it remain."
      :busy="busy"
      @update:open="
        (open) => {
          if (!open) removal = null;
        }
      "
      ><form @submit.prevent="remove">
        <label
          >Type {{ removal?.request.name }} to confirm<input
            v-model="confirmation"
            required
            autocomplete="off" /></label
        ><button class="button" :disabled="busy || confirmation !== removal?.request.name">
          Remove task
        </button>
      </form></BaseDialog
    >
  </section>
</template>
<style scoped>
.tasks {
  max-width: 1000px;
}
h1 {
  font-size: 30px;
  margin: 0 0 12px;
}
h2 {
  font-size: 20px;
  margin: 0 0 8px;
}
p {
  color: var(--muted);
  line-height: 1.65;
}
.task-heading {
  display: flex;
  justify-content: space-between;
  align-items: start;
  gap: 20px;
}
.task-heading button {
  flex-shrink: 0;
}
.task-note {
  margin: 24px 0;
}
.task-card {
  margin: 20px 0;
}
.task-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  margin: 22px 0;
}
.task-status {
  border: 1px solid var(--border);
  padding: 6px 12px;
  border-radius: 20px;
}
dl {
  display: flex;
  gap: 40px;
  flex-wrap: wrap;
}
dt {
  color: var(--muted);
  font-size: 12px;
}
dd {
  margin: 8px 0;
}
label {
  display: grid;
  gap: 8px;
  margin: 18px 0;
}
input,
select {
  width: 100%;
}
.check {
  display: flex;
  align-items: center;
  gap: 10px;
}
.check input {
  width: auto;
}
fieldset {
  border: 0;
  margin: 0;
  padding: 0;
}
details {
  margin: 20px 0;
}
summary {
  cursor: pointer;
}
li {
  margin: 12px 0;
  overflow-wrap: anywhere;
}
.task-review {
  padding: 20px;
  margin-top: 24px;
  border: 1px solid var(--border);
  border-radius: 10px;
}
.task-review button {
  margin-top: 20px;
}
@media (max-width: 600px) {
  .task-heading {
    flex-direction: column;
  }
}
</style>
