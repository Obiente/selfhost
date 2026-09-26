<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  selfhostUrl: string;
}>();
const emit = defineEmits<{ connected: [config: any] }>();
const providers = ref<any[]>([]),
  form = ref({
    provider: '',
    issuer: '',
    provider_project: '',
    create_project: true,
    project_name: 'Selfhost',
    organization_id: '',
    id: 'identity',
    name: 'Selfhost',
    admin_subjects: [] as string[],
    ca_certificate: '',
  }),
  subjects = ref(''),
  credential = ref(''),
  review = ref<any>(null),
  account = ref<any>(null),
  useAccount = ref(false),
  confirmed = ref(false),
  busy = ref(false),
  error = ref('');
const existing = ref<any[]>([]),
  linkedProvider = ref('');
const linkedProviders = computed(() =>
  existing.value.filter((app) =>
    providers.value.some((provider) => provider.id === app.profile?.id),
  ),
);
onMounted(async () => {
  const results = await Promise.allSettled([
    props.api('/login/registration-providers'),
    props.api('/existing'),
  ]);
  if (results[0].status === 'fulfilled') {
    providers.value = results[0].value;
    if (providers.value.length === 1) form.value.provider = providers.value[0].id;
  } else {
    error.value = (results[0].reason as Error).message;
  }
  if (results[1].status === 'fulfilled') existing.value = results[1].value;
});
function useLinkedProvider() {
  const app = linkedProviders.value.find((app) => app.id === linkedProvider.value);
  if (app) {
    form.value.provider = app.profile.id;
    form.value.issuer = app.url;
    credential.value = '';
  }
}
watch(
  [form, subjects, useAccount, () => props.selfhostUrl],
  () => {
    review.value = null;
    confirmed.value = false;
  },
  { deep: true },
);
watch(
  [() => form.value.issuer, () => form.value.provider, () => form.value.ca_certificate, credential],
  () => {
    account.value = null;
    useAccount.value = false;
  },
);
function payload() {
  return {
    ...form.value,
    provider_project: form.value.create_project ? '' : form.value.provider_project,
    selfhost_url: props.selfhostUrl,
    admin_subjects:
      useAccount.value && account.value?.suggested_subject
        ? [account.value.suggested_subject]
        : subjects.value
            .split('\n')
            .map((s) => s.trim())
            .filter(Boolean),
  };
}
async function discover() {
  busy.value = true;
  error.value = '';
  try {
    account.value = await props.api('/login/register/account', 'POST', {
      request: payload(),
      credential: credential.value,
    });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function plan() {
  busy.value = true;
  error.value = '';
  try {
    review.value = await props.api('/login/register/plan', 'POST', payload());
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function create() {
  busy.value = true;
  error.value = '';
  try {
    const result = await props.api('/login/register/apply', 'POST', {
      request: payload(),
      revision: review.value.revision,
      credential: credential.value,
    });
    credential.value = '';
    review.value = null;
    emit('connected', result.config);
  } catch (e) {
    error.value = (e as Error).message;
    credential.value = '';
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <section class="register-provider" aria-labelledby="auto-register-title">
    <h3 id="auto-register-title">Let Selfhost set up sign-in</h3>
    <p>Create a dedicated project and sign-in client in your identity provider.</p>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <form @submit.prevent="plan">
      <fieldset :disabled="busy">
        <label v-if="linkedProviders.length"
          >Use linked provider<select v-model="linkedProvider" @change="useLinkedProvider">
            <option value="">Enter provider details manually</option>
            <option v-for="app in linkedProviders" :key="app.id" :value="app.id">
              {{ app.name }} · {{ app.url }}
            </option></select
          ><small>Selects its address. Review and complete sign-in setup below.</small></label
        >
        <div class="fields">
          <label
            >Provider<select v-model="form.provider" required>
              <option value="" disabled>Choose provider</option>
              <option v-for="p in providers" :value="p.id" :key="p.id">{{ p.name }}</option>
            </select></label
          >
          <label
            >Issuer URL<input
              v-model="form.issuer"
              type="url"
              required
              placeholder="https://identity.example.com"
          /></label>
          <label v-if="form.create_project"
            >Project name<input v-model="form.project_name" required
          /></label>
          <label>Sign-in name<input v-model="form.name" required /></label>
        </div>
        <label
          >Temporary provider API token<input
            v-model="credential"
            type="password"
            autocomplete="new-password"
          /><small
            >Use a token allowed to create applications
            {{
              form.create_project ? 'and projects in your organization' : 'in the selected project'
            }}. It is never saved.</small
          ></label
        >
        <button
          type="button"
          class="button"
          :disabled="busy || !credential || !form.issuer || !form.provider"
          @click="discover"
        >
          Find my account
        </button>
        <div v-if="account" role="status">
          <label v-if="account.suggested_subject" class="confirmation"
            ><input v-model="useAccount" type="checkbox" /> Allow my account ({{
              account.suggested_subject
            }}) to administer Selfhost</label
          >
          <p v-else>
            This token belongs to a machine account or does not identify a human user. Enter your
            own user subject ID below.
          </p>
          <p v-if="account.organization_id">Provider organization: {{ account.organization_id }}</p>
        </div>
        <details :open="account && !account.suggested_subject">
          <summary>Advanced settings and administrator IDs</summary>
          <label class="confirmation"
            ><input v-model="form.create_project" type="checkbox" /> Create a dedicated Selfhost
            project</label
          >
          <label v-if="!form.create_project"
            >Existing provider project ID<input v-model="form.provider_project" required
          /></label>
          <label v-if="form.create_project"
            >Organization ID (optional)<input v-model="form.organization_id" /><small
              >Leave blank to use the API credential's organization.</small
            ></label
          >
          <label
            >Administrator subject IDs<textarea
              v-model="subjects"
              rows="2"
              :disabled="useAccount"
            /><small
              >Exact user IDs, one per line. These identities receive full administrator
              access.</small
            ></label
          >
          <label>Connection ID<input v-model="form.id" pattern="[a-z0-9_-]+" required /></label>
          <label>Private CA certificates<textarea v-model="form.ca_certificate" rows="3" /></label>
        </details>
        <button class="button" :disabled="busy || (!useAccount && !subjects.trim())">
          Review sign-in setup
        </button>
      </fieldset>
    </form>
    <div v-if="review" class="review">
      <h4>Confirm sign-in setup</h4>
      <p>{{ review.creates }}.</p>
      <p>
        <strong>{{ review.provider }}</strong> at {{ review.issuer }},
        {{ review.create_project ? 'new project' : 'existing project' }} {{ review.project }}
      </p>
      <p v-if="review.create_project">Organization: {{ review.organization }}</p>
      <p>
        Callback: <code>{{ review.callback }}</code>
      </p>
      <p>
        Administrators: <code>{{ payload().admin_subjects.join(', ') }}</code>
      </p>
      <p v-if="review.development_mode">
        Development mode allows this exact HTTP loopback callback.
      </p>
      <label class="confirmation"
        ><input v-model="confirmed" type="checkbox" /> Create
        {{ review.create_project ? 'this project and client' : 'this client' }} and grant these
        users administrator access. I have local recovery access.</label
      >
      <button
        type="button"
        class="button primary"
        :disabled="busy || !confirmed || !credential"
        @click="create"
      >
        {{ busy ? 'Connecting…' : 'Set up and connect' }}
      </button>
      <p>
        <small
          >If the provider's response is lost, Selfhost stops retries to avoid duplicate resources.
          A rejected client request can be retried after correcting permissions; the saved project
          is reused.</small
        >
      </p>
    </div>
  </section>
</template>
<style scoped>
fieldset {
  border: 0;
  padding: 0;
  margin: 0;
  min-width: 0;
}
h3 {
  margin: 24px 0 8px;
}
p {
  line-height: 1.6;
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
textarea,
select {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 8px 0;
}
small {
  color: var(--muted);
  line-height: 1.5;
}
summary {
  padding: 12px 0;
  cursor: pointer;
  color: var(--green);
}
.review {
  border: 1px solid var(--green);
  padding: 18px;
  border-radius: 10px;
  margin-top: 20px;
}
.confirmation {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}
.confirmation input {
  width: auto;
  margin: 4px 0;
}
code {
  overflow-wrap: anywhere;
}
@media (max-width: 680px) {
  .fields {
    grid-template-columns: 1fr;
  }
}
</style>
