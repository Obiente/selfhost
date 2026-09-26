<script setup lang="ts">
import { sessionFetch } from './session';
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { useAsyncTask } from './composables/useAsyncTask';
const props = defineProps<{ projectId: string; service: string; writable: boolean }>();
const providers = ref<{ id: string; name: string; creates_project: boolean }[]>([]);
const state = ref<any>(null),
  preview = ref<any>(null),
  account = ref<any>(null),
  credential = ref('');
const task = useAsyncTask();
const form = reactive({
  provider: '',
  issuer: '',
  provider_project: '',
  create_project: true,
  project_name: 'Selfhost',
  organization_id: '',
  administrator_subject: '',
  replace_existing: false,
  ca_certificate: '',
  app_url: '',
  name: 'Selfhost',
});
const stages: Record<string, string> = {
  project_creation_uncertain: 'Project creation needs inspection',
  creation_uncertain: 'Client creation needs inspection',
  project_created: 'Project created',
  client_created: 'Client created',
  configuring_app: 'Configuring app',
  app_configuration_failed: 'App configuration needs attention',
  awaiting_login_test: 'Ready for a sign-in test',
};
const base = computed(
  () => `/api/projects/${props.projectId}/services/${props.service}/integration/connection`,
);
async function request(path: string, body?: unknown) {
  const response = await sessionFetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw Error(value.error || 'Unable to configure sign-in.');
  return value;
}
function payload() {
  return { ...form, provider_project: form.create_project ? '' : form.provider_project };
}
async function load() {
  state.value = await request(base.value);
}
function lookupAccount() {
  return task.run(async () => {
    account.value = await request(base.value + '/account', {
      ...payload(),
      credential: credential.value,
    });
    form.administrator_subject =
      account.value.human && account.value.administrator_role
        ? account.value.suggested_subject || ''
        : '';
  });
}
function plan() {
  return task.run(async () => {
    preview.value = await request(base.value + '/plan', payload());
  });
}
function connect() {
  if (!preview.value) return;
  return task.run(async () => {
    try {
      await request(base.value + '/create', {
        ...payload(),
        credential: credential.value,
        revision: preview.value.revision,
      });
      preview.value = null;
    } finally {
      credential.value = '';
      await load();
    }
  });
}
function resume() {
  return task.run(async () => {
    await request(base.value + '/resume', {});
    await load();
  });
}
watch(
  form,
  () => {
    preview.value = null;
  },
  { deep: true },
);
watch(
  () => [form.provider, form.issuer, form.ca_certificate, credential.value],
  () => {
    account.value = null;
    form.administrator_subject = '';
    preview.value = null;
  },
);
watch(
  () => form.provider,
  () => {
    form.create_project =
      providers.value.find((p) => p.id === form.provider)?.creates_project ?? false;
  },
);
onMounted(() =>
  task.run(async () => {
    providers.value = await request('/api/identity-providers');
    form.provider = providers.value[0]?.id || '';
    await load();
  }),
);
</script>
<template>
  <section class="identity" :aria-busy="task.busy.value">
    <h3>Connect single sign-on</h3>
    <p>Create a login client at your identity provider and configure this app to use it.</p>
    <p v-if="task.error.value" role="alert" class="monitor-error">{{ task.error.value }}</p>
    <template v-if="state">
      <h4>{{ stages[state.stage] || state.stage }}</h4>
      <dl>
        <dt>Provider</dt>
        <dd>{{ state.issuer }}</dd>
        <dt>Project</dt>
        <dd>{{ state.provider_project }}</dd>
        <dt>Application ID</dt>
        <dd>{{ state.application_id }}</dd>
        <dt>Callback</dt>
        <dd>{{ state.callback }}</dd>
        <template v-if="state.administrator_subject"
          ><dt>App administrator</dt>
          <dd>{{ state.administrator_subject }} ({{ state.administrator_role }})</dd></template
        >
      </dl>
      <p v-if="state.stage.includes('uncertain')">
        Inspect the project and client in your provider before recovering this connection. A
        creation response may have been interrupted.
      </p>
      <template v-else>
        <p>
          Keep your local administrator login until you have tested sign-in in a separate browser
          session. Environment-based login changes take effect when you recreate the app from its
          updated Compose configuration.
        </p>
        <button
          type="button"
          class="button"
          :disabled="task.busy.value || !writable"
          @click="resume"
        >
          Reapply app connection
        </button>
      </template>
    </template>
    <form v-else @submit.prevent="plan">
      <fieldset :disabled="task.busy.value || !writable">
        <legend>Provider and app</legend>
        <label
          >Identity provider<select v-model="form.provider" required>
            <option v-for="provider in providers" :key="provider.id" :value="provider.id">
              {{ provider.name }}
            </option>
          </select></label
        >
        <label
          >Provider URL<input
            v-model="form.issuer"
            type="url"
            placeholder="https://identity.example.com"
            required
        /></label>
        <label
          >App URL<input
            v-model="form.app_url"
            type="url"
            placeholder="http://localhost:3000"
            required
        /></label>
        <label
          >Provider access token<input
            v-model="credential"
            type="password"
            autocomplete="off"
            required
          /><small
            >Used only for this operation. Needs permission to create the reviewed project and
            client.</small
          ></label
        >
        <button
          type="button"
          class="button"
          :disabled="!credential || !form.issuer"
          @click="lookupAccount"
        >
          Use my provider account
        </button>
        <section v-if="account" aria-label="Provider account">
          <label
            v-if="account.human && account.suggested_subject && account.administrator_role"
            class="checkbox"
            ><input
              v-model="form.administrator_subject"
              type="checkbox"
              :true-value="account.suggested_subject"
              false-value="''"
            />
            Make {{ account.suggested_subject }} the {{ account.administrator_role }}</label
          >
          <p v-else-if="!account.human">
            This credential belongs to a machine account. It will not receive app administrator
            access.
          </p>
          <p v-else>
            This app has no automatic administrator mapping. Configure its administrator permissions
            in the app after connecting.
          </p>
        </section>
        <details>
          <summary>Advanced options</summary>
          <label
            v-if="providers.find((p) => p.id === form.provider)?.creates_project"
            class="checkbox"
            ><input v-model="form.create_project" type="checkbox" /> Create a dedicated provider
            project</label
          >
          <label v-if="form.create_project"
            >New project name<input v-model="form.project_name" required
          /></label>
          <label v-else
            >Existing provider project ID<input v-model="form.provider_project" required
          /></label>
          <label>Login name<input v-model="form.name" required /></label>
          <label>Provider organization ID (optional)<input v-model="form.organization_id" /></label>
          <label
            >Private CA certificate in PEM format (optional)<textarea
              v-model="form.ca_certificate"
              rows="5"
              spellcheck="false"
            /><small
              >Used to verify provider requests during setup. Configure the app's own CA trust
              separately.</small
            ></label
          >
          <label class="checkbox"
            ><input v-model="form.replace_existing" type="checkbox" /> Allow replacement of existing
            login environment settings</label
          >
        </details>
        <p v-if="form.create_project">
          Selfhost will create a dedicated project named {{ form.project_name }}.
        </p>
        <button class="button">Review connection</button>
      </fieldset>
      <section v-if="preview" class="review" aria-label="Review connection">
        <h4>Review connection</h4>
        <p v-if="preview.details.create_project">
          Create project {{ preview.details.project_name }} and one login client at
          {{ preview.details.issuer }}.
        </p>
        <p v-else>
          Create one login client in project {{ preview.details.provider_project }} at
          {{ preview.details.issuer }}.
        </p>
        <p>
          Allowed callback: <code>{{ preview.details.callback }}</code>
        </p>
        <p v-if="preview.details.administrator_subject">
          <strong
            >Grant {{ preview.details.administrator_role }} to
            {{ preview.details.administrator_subject }}.</strong
          >
        </p>
        <p v-else>
          Selfhost will not assign an administrator role. Review the app's own first-login policy
          below.
        </p>
        <ul v-if="preview.details.warnings?.length">
          <li v-for="warning in preview.details.warnings" :key="warning">{{ warning }}</li>
        </ul>
        <p v-if="preview.details.restart_required">
          The updated Compose settings will be saved. Recreate the app when you are ready to
          activate sign-in.
        </p>
        <p v-if="preview.details.replace_existing">
          Existing login environment settings may be replaced. A configuration backup will be kept.
        </p>
        <button
          type="button"
          class="button primary"
          :disabled="task.busy.value || !writable || !credential"
          @click="connect"
        >
          {{ task.busy.value ? 'Connecting…' : 'Create and connect' }}
        </button>
      </section>
    </form>
  </section>
</template>
<style scoped>
.identity {
  padding: 20px 0;
  border-top: 1px solid var(--border);
}
fieldset {
  border: 0;
  padding: 0;
  margin: 0;
}
legend {
  font-weight: 600;
}
label {
  display: block;
  margin: 16px 0;
}
input,
select,
textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 8px 0;
}
.checkbox {
  display: flex;
  gap: 10px;
  align-items: center;
}
.checkbox input {
  width: auto;
}
dd {
  margin: 4px 0 14px;
  overflow-wrap: anywhere;
}
small {
  color: var(--muted);
}
.review {
  margin-top: 20px;
  padding: 16px;
  background: var(--paper);
}
code {
  overflow-wrap: anywhere;
}
details {
  margin: 20px 0;
}
summary {
  cursor: pointer;
}
</style>
