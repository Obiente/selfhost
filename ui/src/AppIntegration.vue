<script setup lang="ts">
import { sessionFetch } from './session';
import { ref, computed, onMounted } from 'vue';
import IdentityConnection from './IdentityConnection.vue';
type Field = {
  id: string;
  label: string;
  kind: string;
  description?: string;
  advanced?: boolean;
  default?: any;
  choices?: string[];
};
const props = defineProps<{ projectId: string; service: string; writable: boolean }>();
const profile = ref<any>(null),
  values = ref<Record<string, any>>({}),
  original = ref<Record<string, any>>({}),
  inputs = ref<Record<string, any>>({});
const advanced = ref(false),
  busy = ref(false),
  error = ref(''),
  notice = ref(''),
  plan = ref<any>(null),
  action = ref<any>(null),
  result = ref<any>(null);
const backups = ref<string[]>([]),
  backup = ref(''),
  restorePlan = ref<any>(null);
async function history() {
  backups.value = await api('/backups');
}
async function restore(apply = false) {
  busy.value = true;
  error.value = '';
  try {
    const d = await api('/restore', {
      backup: backup.value,
      ...(apply ? { revision: restorePlan.value.revision } : {}),
    });
    if (apply) {
      notice.value = d.restart_required
        ? 'Previous settings saved. Start the project to apply them.'
        : 'Previous settings restored and verified.';
      restorePlan.value = null;
      await load();
      await history();
    } else restorePlan.value = d;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function api(path: string, body?: unknown) {
  const r = await sessionFetch(
    `/api/projects/${props.projectId}/services/${props.service}/integration${path}`,
    {
      method: body === undefined ? 'GET' : 'POST',
      headers: { ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
      body: body === undefined ? undefined : JSON.stringify(body),
    },
  );
  const d = await r.json();
  if (!r.ok) throw Error(d.error);
  return d;
}
const fields = computed(
  () => profile.value?.fields.filter((f: Field) => advanced.value || !f.advanced) || [],
);
const actions = computed(
  () => profile.value?.actions.filter((a: any) => advanced.value || !a.advanced) || [],
);
function display(v: any, f: Field) {
  return ['string_list', 'arguments', 'json'].includes(f.kind)
    ? JSON.stringify(v ?? [], null, 2)
    : (v ?? '');
}
function typed(v: any, f: Field) {
  if (['string_list', 'arguments', 'json'].includes(f.kind)) return JSON.parse(v);
  if (f.kind === 'integer') {
    if (v === '') throw Error(`${f.label} needs a number`);
    return Number(v);
  }
  return v;
}
function changes() {
  const out: Record<string, any> = {};
  for (const f of profile.value.fields) {
    if (values.value[f.id] !== original.value[f.id]) out[f.id] = typed(values.value[f.id], f);
  }
  return out;
}
async function load() {
  busy.value = true;
  error.value = '';
  try {
    const d = await api('/config');
    for (const f of profile.value.fields) {
      values.value[f.id] = display(d.values[f.id], f);
    }
    original.value = { ...values.value };
    plan.value = null;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function preview() {
  busy.value = true;
  error.value = '';
  try {
    plan.value = await api('/plan', { values: changes() });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function apply() {
  if (!plan.value) return;
  busy.value = true;
  error.value = '';
  try {
    const d = await api('/apply', { values: changes(), revision: plan.value.revision });
    notice.value = d.restart_required
      ? 'Configuration saved. Start the project to apply it.'
      : `Settings applied. Previous values saved as ${d.backup}.`;
    await load();
    await history();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
function choose(a: any) {
  action.value = a;
  inputs.value = Object.fromEntries(a.inputs.map((f: Field) => [f.id, display(f.default, f)]));
  result.value = null;
}
async function run() {
  busy.value = true;
  error.value = '';
  try {
    const data = Object.fromEntries(
      action.value.inputs.map((f: Field) => [f.id, typed(inputs.value[f.id], f)]),
    );
    result.value = await api('/actions', { action: action.value.id, inputs: data });
    for (const f of action.value.inputs.filter((f: Field) => f.kind === 'secret'))
      inputs.value[f.id] = '';
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
onMounted(async () => {
  try {
    profile.value = await api('');
    await load();
    await history();
  } catch (e) {
    error.value = (e as Error).message;
  }
});
</script>
<template>
  <section class="app-integration">
    <header>
      <h2>App settings &amp; connections</h2>
      <label><input type="checkbox" v-model="advanced" />Advanced</label>
    </header>
    <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <template v-if="profile"
      ><ul v-if="profile.warnings?.length">
        <li v-for="warning in profile.warnings" :key="warning">{{ warning }}</li>
      </ul>
      <p>
        Manage {{ profile.name }} using its own configuration format. Review changes before applying
        them.
      </p>
      <button class="text-button" :disabled="busy" @click="load">Read current settings</button>
      <form @submit.prevent="preview">
        <label v-for="f in fields" :key="f.id"
          >{{ f.label
          }}<input
            v-if="f.kind === 'boolean'"
            type="checkbox"
            v-model="values[f.id]"
            @change="plan = null"
          /><select v-else-if="f.choices?.length" v-model="values[f.id]" @change="plan = null">
            <option value="">Not set</option>
            <option v-for="v in f.choices" :key="v">{{ v }}</option></select
          ><textarea
            v-else-if="['string_list', 'arguments', 'json'].includes(f.kind)"
            v-model="values[f.id]"
            rows="3"
            @input="plan = null"
          /><input
            v-else
            :type="f.kind === 'integer' ? 'number' : f.kind === 'secret' ? 'password' : 'text'"
            v-model="values[f.id]"
            @input="plan = null"
          /><small>{{ f.description }}</small></label
        ><button class="button" :disabled="busy || !writable">Preview changes</button>
      </form>
      <section v-if="plan">
        <h3>Review changes</h3>
        <p v-if="!plan.changes.length">No settings changed.</p>
        <div v-for="c in plan.changes" :key="c.id">
          <strong>{{ c.label }}</strong>
          <pre>{{ JSON.stringify(c.before) }} → {{ JSON.stringify(c.after) }}</pre>
        </div>
        <p>
          Previous values are saved before applying.
          {{
            plan.restart_required
              ? 'This changes the saved configuration; restart the project afterward.'
              : 'This changes the running application.'
          }}
        </p>
        <button
          class="button primary"
          :disabled="busy || !writable || !plan.changes.length"
          @click="apply"
        >
          Apply reviewed changes
        </button>
      </section>
      <IdentityConnection
        v-if="profile.oidc_client"
        :project-id="projectId"
        :service="service"
        :writable="writable"
      />
      <section v-if="advanced && backups.length">
        <h3>Restore previous settings</h3>
        <label
          >Saved change<select v-model="backup" @change="restorePlan = null">
            <option value="">Choose a saved change</option>
            <option v-for="b in backups" :key="b" :value="b">{{ b }}</option>
          </select></label
        ><button class="button" :disabled="busy || !writable || !backup" @click="restore()">
          Preview restore</button
        ><template v-if="restorePlan"
          ><div v-for="c in restorePlan.changes" :key="c.id">
            <strong>{{ c.label }}</strong>
            <pre>{{ JSON.stringify(c.before) }} → {{ JSON.stringify(c.after) }}</pre>
          </div>
          <p>The current values will also be saved before restoring.</p>
          <button
            class="button primary"
            :disabled="busy || !writable || !restorePlan.changes.length"
            @click="restore(true)"
          >
            Restore reviewed settings
          </button></template
        >
      </section>
      <h3>Manage app</h3>
      <article v-for="a in actions" :key="a.id">
        <h4>{{ a.label }}</h4>
        <p>{{ a.description }}</p>
        <button class="button" :disabled="busy || !writable" @click="choose(a)">
          {{ a.label }}
        </button>
      </article>
      <form v-if="action" @submit.prevent="run">
        <h3>{{ action.label }}</h3>
        <p>{{ action.description }}</p>
        <label v-for="f in action.inputs" :key="f.id"
          >{{ f.label
          }}<textarea
            v-if="['arguments', 'string_list', 'json'].includes(f.kind)"
            v-model="inputs[f.id]"
            rows="3"
            required /><input
            v-else
            :type="f.kind === 'secret' ? 'password' : 'text'"
            v-model="inputs[f.id]"
            required
            autocomplete="off" /></label
        ><button class="button primary" :disabled="busy || !writable">
          {{ busy ? 'Running…' : 'Run ' + action.label.toLowerCase() }}</button
        ><button type="button" class="text-button" :disabled="busy" @click="action = null">
          Cancel
        </button>
      </form>
      <section v-if="result">
        <p>{{ result.ok ? 'Completed.' : result.message }}</p>
        <div v-for="(s, i) in result.steps" :key="i">
          <strong>{{ s.label }} · {{ s.status }}</strong>
          <pre v-if="s.output">{{ s.output }}</pre>
        </div>
        <p v-if="result.failed_step">Stopped at: {{ result.failed_step }}</p>
      </section></template
    >
  </section>
</template>
<style scoped>
.app-integration {
  max-width: 900px;
}
.app-integration header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.app-integration label {
  display: block;
  margin: 16px 0;
}
.app-integration input:not([type='checkbox']),
select,
textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 8px 0;
}
.app-integration article,
.app-integration section {
  padding: 20px 0;
  border-top: 1px solid var(--border);
}
.app-integration pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  padding: 12px;
  background: var(--paper);
}
small {
  color: var(--muted);
}
h3 {
  margin-top: 24px;
}
</style>
