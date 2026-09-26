<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { sessionFetch } from './session';
import { useAsyncTask } from './composables/useAsyncTask';

type Link = { name: string; url: string; icon: string; description: string; selected?: boolean };
type Field = {
  id: string;
  label: string;
  kind: string;
  required?: boolean;
  modes?: string[];
  default?: unknown;
};
const props = defineProps<{ projectId: string; service: string; writable: boolean }>();
const emit = defineEmits<{ changed: [] }>();
const task = useAsyncTask();
const info = ref<any>(null);
const mode = ref('bootstrap');
const inputs = reactive<Record<string, string>>({});
const apps = ref<Link[]>([]);
const review = ref<any>(null);
const result = ref<any>(null);
const modes = computed<string[]>(() =>
  (info.value?.profile?.modes || []).filter((value: string) =>
    info.value?.state?.completed ? value === 'sync' : value !== 'sync',
  ),
);
const modeLabels: Record<string, string> = {
  bootstrap: 'Initialize a fresh installation',
  connect: 'Connect an existing installation',
  sync: 'Add links to the connected dashboard',
};
const fields = computed<Field[]>(() =>
  (info.value?.profile?.fields || []).filter(
    (field: Field) => !field.modes || field.modes.includes(mode.value),
  ),
);
const base = computed(
  () => `/api/projects/${props.projectId}/services/${props.service}/onboarding`,
);
async function request(path: string, body?: unknown) {
  const response = await sessionFetch(base.value + path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || 'Unable to configure this app.');
  return value;
}
function payload() {
  return {
    mode: mode.value,
    inputs: Object.fromEntries(fields.value.map((field) => [field.id, inputs[field.id] || ''])),
    apps: info.value?.profile?.accepts_apps
      ? apps.value.filter((app) => app.selected).map(({ selected: _, ...app }) => app)
      : [],
  };
}
async function load() {
  info.value = await request('');
  if (info.value?.state?.completed && info.value?.profile?.supports_sync) mode.value = 'sync';
}
onMounted(() =>
  task.run(async () => {
    await load();
    if (!info.value?.supported) return;
    mode.value = modes.value[0] || 'bootstrap';
    for (const field of info.value.profile.fields || [])
      inputs[field.id] = String(field.default ?? '');
    apps.value = (info.value.suggestions || [])
      .filter((app: Link) => !(info.value.state?.linked_urls || []).includes(app.url))
      .map((app: Link) => ({ ...app, selected: true }));
  }),
);
watch(
  [mode, inputs, apps],
  () => {
    review.value = null;
  },
  { deep: true },
);
function addLink() {
  apps.value.push({ name: '', url: '', icon: '', description: '', selected: true });
}
function plan() {
  result.value = null;
  return task.run(async () => {
    review.value = await request('/plan', payload());
  });
}
function apply() {
  if (!review.value) return;
  return task.run(async () => {
    try {
      result.value = await request('/apply', {
        request: payload(),
        revision: review.value.revision,
      });
      review.value = null;
      apps.value = apps.value.filter((app) => !app.selected);
      emit('changed');
    } finally {
      for (const field of info.value?.profile?.fields || [])
        if (field.kind === 'secret' || field.kind === 'password') inputs[field.id] = '';
      await load();
    }
  });
}
</script>
<template>
  <section
    v-if="info?.supported"
    class="onboarding"
    :aria-labelledby="`onboarding-${service}`"
    :aria-busy="task.busy.value"
  >
    <h2 :id="`onboarding-${service}`">{{ info.profile.name }}</h2>
    <p>Review the setup steps and account settings before applying changes.</p>
    <p v-if="task.error.value" role="alert" class="monitor-error">{{ task.error.value }}</p>
    <p v-if="info.state?.pending" role="alert">
      An earlier setup needs inspection. Review its recorded progress before retrying.
    </p>
    <p v-if="info.state?.has_api_key" role="status">
      Selfhost has a private API connection for this app.
    </p>
    <p v-if="info.state?.completed" role="status">
      Setup is complete.<span v-if="info.profile.supports_sync">
        Add more links using the saved private API connection.</span
      >
    </p>
    <form
      v-if="
        !info.state?.started ||
        (info.state?.completed && info.profile.supports_sync && !info.state?.pending)
      "
      @submit.prevent="plan"
    >
      <fieldset :disabled="task.busy.value || !writable">
        <legend>Setup options</legend>
        <label
          >What would you like to do?
          <select v-model="mode">
            <option v-for="value in modes" :key="value" :value="value">
              {{ modeLabels[value] || value }}
            </option>
          </select>
        </label>
        <label v-for="field in fields" :key="field.id"
          >{{ field.label }}
          <input
            v-model="inputs[field.id]"
            :type="
              ['secret', 'password'].includes(field.kind)
                ? 'password'
                : field.kind === 'url'
                  ? 'url'
                  : 'text'
            "
            :required="field.required"
            :autocomplete="['secret', 'password'].includes(field.kind) ? 'new-password' : 'off'"
          />
        </label>
        <p>
          Passwords are used for this operation. API credentials are kept in private configuration
          and are never included in the setup result.
        </p>
        <details v-if="info.profile.accepts_apps">
          <summary>Services to include ({{ apps.filter((app) => app.selected).length }})</summary>
          <p>
            Choose the links your dashboard users should see. Check their addresses, especially
            services on other servers.
          </p>
          <article v-for="(app, index) in apps" :key="index" class="app-link">
            <label class="checkbox"
              ><input type="checkbox" v-model="app.selected" /> Include service
              {{ index + 1 }}</label
            >
            <label>Name<input v-model="app.name" :required="app.selected" /></label>
            <label
              >Address<input
                v-model="app.url"
                type="url"
                :required="app.selected"
                placeholder="https://app.example.com"
            /></label>
            <button type="button" class="text-button" @click="apps.splice(index, 1)">
              Remove this link
            </button>
          </article>
          <button type="button" class="button" @click="addLink">Add a service link</button>
        </details>
        <button class="button" type="submit">
          {{ task.busy.value ? 'Checking…' : 'Review setup' }}
        </button>
      </fieldset>
    </form>
    <section v-if="review" class="review" aria-label="Review app setup">
      <h3>Review setup</h3>
      <ol>
        <li v-for="(step, index) in review.steps" :key="index">
          {{ typeof step === 'string' ? step : step.label }}
        </li>
      </ol>
      <ul v-if="review.warnings?.length">
        <li v-for="warning in review.warnings" :key="warning">{{ warning }}</li>
      </ul>
      <p v-if="review.apps?.length">{{ review.apps.length }} service links will be included.</p>
      <button
        type="button"
        class="button primary"
        :disabled="task.busy.value || !writable"
        @click="apply"
      >
        {{ task.busy.value ? 'Setting up…' : 'Apply reviewed setup' }}
      </button>
    </section>
    <section v-if="result" role="status" class="review">
      <h3>{{ result.completed ? 'Setup completed' : 'Setup needs attention' }}</h3>
      <p v-if="info.profile.accepts_apps">
        {{ result.apps_added || 0 }} app links and {{ result.items_added || 0 }} dashboard items
        added.
      </p>
      <a
        v-if="result.board_url"
        :href="result.board_url"
        class="text-link"
        target="_blank"
        rel="noopener noreferrer"
        >Open dashboard</a
      >
    </section>
  </section>
</template>
<style scoped>
.onboarding {
  margin-top: 24px;
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 24px;
}
h2 {
  font-size: 20px;
  margin-top: 0;
}
p {
  color: var(--muted);
  line-height: 1.6;
}
fieldset {
  border: 0;
  margin: 0;
  padding: 0;
}
legend {
  font-weight: 600;
  margin: 12px 0;
}
label {
  display: grid;
  gap: 8px;
  margin: 16px 0;
}
input,
select {
  width: 100%;
  box-sizing: border-box;
}
.checkbox {
  display: flex;
  align-items: center;
}
.checkbox input {
  width: auto;
}
details {
  margin: 20px 0;
}
summary {
  cursor: pointer;
}
.app-link,
.review {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 16px;
  margin: 16px 0;
}
li {
  margin: 8px 0;
  overflow-wrap: anywhere;
}
</style>
