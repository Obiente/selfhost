<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Download, RefreshCw } from 'lucide-vue-next';
import BaseDialog from './components/BaseDialog.vue';
import { useAsyncTask } from './composables/useAsyncTask';
import { usePolling } from './composables/usePolling';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const { busy, error, run } = useAsyncTask();
const open = ref(false),
  status = ref<any>(null),
  plan = ref<any>(null),
  jobs = ref<any[]>([]),
  staged = ref<any>(null),
  confirmation = ref(''),
  restartConfirmation = ref(''),
  restarting = ref(false),
  recovering = ref(false),
  restartInstructions = ref('');
const instructions = computed(() =>
  Array.isArray(status.value?.instructions)
    ? status.value.instructions.join('\n')
    : status.value?.instructions || '',
);
const method = computed(
  () =>
    ({
      cargo: 'Cargo installation',
      source: 'Source checkout',
      npx: 'npx package',
      npm: 'npm package',
      npm_global: 'Global npm package',
      binary: 'Standalone binary',
      standalone: 'Standalone binary',
    })[status.value?.install_method as string] ||
    status.value?.install_method ||
    'Checking installation',
);
async function load() {
  status.value = await props.api('/updates');
}
async function check() {
  await run(async () => {
    status.value = await props.api('/updates/check', 'POST', {});
  });
}
async function show() {
  open.value = true;
  await run(async () => {
    await load();
    jobs.value = await props.api('/updates/jobs');
  });
}
async function review() {
  await run(async () => {
    plan.value = await props.api('/updates/plan', 'POST', {});
    staged.value = null;
    recovering.value = false;
    confirmation.value = '';
  });
}
async function stage() {
  if (confirmation.value !== plan.value?.confirmation) return;
  await run(async () => {
    staged.value = await props.api('/updates/stage', 'POST', {
      plan_id: plan.value.id,
      revision: plan.value.revision,
      confirmation: confirmation.value,
    });
    plan.value = null;
    recovering.value = false;
    restartConfirmation.value = '';
    jobs.value = await props.api('/updates/jobs');
  });
}
async function reconnect() {
  await run(async () => {
    const response = await fetch('/auth/info', { cache: 'no-store' });
    if (!response.ok) throw new Error('Selfhost is not ready yet. Try reconnecting in a moment.');
    location.reload();
  });
}
async function activate() {
  if (restartConfirmation.value !== staged.value?.confirmation) return;
  await run(async () => {
    const result = await props.api(
      recovering.value ? '/updates/recover' : '/updates/activate',
      'POST',
      { job_id: staged.value.job_id, confirmation: restartConfirmation.value },
    );
    restartInstructions.value = Array.isArray(result.recovery_instructions)
      ? result.recovery_instructions.join('\n')
      : result.recovery_instructions || '';
    restarting.value = !!result.shutdown_required;
  });
}
usePolling(check, 3600000, () => !busy.value && !restarting.value);
onMounted(async () => {
  await run(load);
  if (Date.now() / 1000 - (status.value?.checked_at || 0) > 21600) await check();
});
</script>
<template>
  <button
    class="update-trigger"
    :class="{ available: status?.update_available }"
    :aria-label="status?.update_available ? 'Selfhost update available' : 'Selfhost updates'"
    @click="show"
  >
    <Download :size="17" aria-hidden="true" /><span>{{
      status?.update_available ? 'Update available' : 'Updates'
    }}</span></button
  ><BaseDialog
    :open="open"
    title="Selfhost updates"
    description="Check your installed version and review an update before replacing it."
    :busy="busy"
    @update:open="open = $event"
    ><p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <template v-if="restarting"
      ><h3>Restarting Selfhost</h3>
      <p role="status">
        {{
          recovering
            ? 'The previous executable will be restored'
            : 'The verified update will be activated'
        }}
        after running operations finish. Sign in again when Selfhost is ready.
      </p>
      <button class="button primary" :disabled="busy" @click="reconnect">
        Reconnect to Selfhost
      </button>
      <pre v-if="restartInstructions" class="update-instructions">{{ restartInstructions }}</pre>
      <p>
        The previous executable is retained for recovery. If restarting fails, use the update
        recovery instructions in your terminal.
      </p></template
    ><template v-else
      ><dl v-if="status" class="version-details">
        <div>
          <dt>Installed</dt>
          <dd>{{ status.installed_version }}</dd>
        </div>
        <div>
          <dt>Latest available</dt>
          <dd>{{ status.latest_version || 'Not checked' }}</dd>
        </div>
        <div>
          <dt>Installation</dt>
          <dd>{{ method }}</dd>
        </div>
      </dl>
      <p v-if="status?.check_error" role="status">{{ status.check_error }}</p>
      <p v-else-if="status?.update_available">A newer version of Selfhost is available.</p>
      <p v-else-if="status?.latest_version">No newer version is available for this installation.</p>
      <button class="button" :disabled="busy" @click="check">
        <RefreshCw :size="15" />Check for updates
      </button>
      <pre v-if="instructions" class="update-instructions" tabindex="0">{{ instructions }}</pre>
      <button
        v-if="status?.can_stage && status?.update_available && !plan && !staged"
        class="button primary"
        :disabled="busy"
        @click="review"
      >
        Review update
      </button>
      <section v-if="plan" class="update-review">
        <h3>Update to {{ plan.version }}</h3>
        <ul>
          <li v-for="item in plan.scope" :key="item">{{ item }}</li>
        </ul>
        <p v-for="warning in plan.warnings" :key="warning">{{ warning }}</p>
        <label
          >Type <strong>{{ plan.confirmation }}</strong> to prepare this version<input
            v-model="confirmation"
            autocomplete="off"
            :disabled="busy" /></label
        ><button
          class="button primary"
          :disabled="busy || confirmation !== plan.confirmation"
          @click="stage"
        >
          {{ busy ? 'Preparing update…' : 'Prepare update' }}
        </button>
      </section>
      <section v-if="staged" class="update-review">
        <h3>
          {{
            recovering ? 'Recover the previous version' : `Version ${staged.version} is prepared`
          }}
        </h3>
        <p>
          {{
            recovering
              ? 'Recovery restores the previous executable. It does not undo changes made to your data by the newer version.'
              : 'The current executable is still running. Activate the update when you are ready to restart Selfhost.'
          }}
        </p>
        <p>
          After restarting, sign in with your identity provider. For local recovery, run
          <code>selfhost update jobs</code> in a terminal to find the private restart log containing
          your new sign-in link.
        </p>
        <pre v-if="staged.recovery_instructions" class="update-instructions">{{
          Array.isArray(staged.recovery_instructions)
            ? staged.recovery_instructions.join('\n')
            : staged.recovery_instructions
        }}</pre>
        <label
          >Type <strong>{{ staged.confirmation }}</strong> to
          {{ recovering ? 'recover' : 'update' }} and restart<input
            v-model="restartConfirmation"
            autocomplete="off"
            :disabled="busy" /></label
        ><button
          class="button primary"
          :disabled="busy || restartConfirmation !== staged.confirmation"
          @click="activate"
        >
          {{ recovering ? 'Recover and restart' : 'Update and restart' }}
        </button>
      </section>
      <details v-if="jobs.length">
        <summary>Prepared updates and recovery</summary>
        <article v-for="job in jobs" :key="job.job_id">
          <strong>{{ job.version }}</strong>
          <p>{{ job.status }}</p>
          <p v-if="job.error" role="alert">{{ job.error }}</p>
          <button
            v-if="job.status === 'staged' && job.confirmation"
            class="button"
            :disabled="busy"
            @click="
              staged = job;
              plan = null;
              recovering = false;
              restartConfirmation = '';
            "
          >
            Review activation</button
          ><button
            v-if="job.can_recover"
            class="button"
            :disabled="busy"
            @click="
              staged = { ...job, confirmation: job.recovery_confirmation };
              plan = null;
              recovering = true;
              restartConfirmation = '';
            "
          >
            Review recovery
          </button>
          <pre v-if="job.recovery_instructions" class="update-instructions">{{
            Array.isArray(job.recovery_instructions)
              ? job.recovery_instructions.join('\n')
              : job.recovery_instructions
          }}</pre>
        </article>
      </details>
      <p v-if="busy" role="status">
        Checking and preparing the requested update. Keep Selfhost running.
      </p></template
    ></BaseDialog
  >
</template>
<style scoped>
.update-trigger {
  display: flex;
  align-items: center;
  gap: 7px;
  background: transparent;
  border: 1px solid var(--border);
  padding: 8px 10px;
  border-radius: 7px;
  color: var(--muted);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.update-trigger.available {
  color: var(--green);
  border-color: var(--green);
}
.version-details {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 18px;
}
.version-details dt {
  font-size: 12px;
  color: var(--muted);
}
.version-details dd {
  margin: 6px 0 0;
  overflow-wrap: anywhere;
}
p,
li {
  line-height: 1.65;
}
.button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin: 8px 10px 8px 0;
}
.update-instructions {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--bg);
  padding: 16px;
  border-radius: 8px;
  line-height: 1.7;
  font-size: 12px;
}
.update-review {
  border-top: 1px solid var(--border);
  margin-top: 22px;
  padding-top: 12px;
}
.update-review label {
  display: block;
  margin-top: 20px;
}
.update-review input {
  display: block;
  width: 100%;
  margin-top: 8px;
}
.update-trigger:focus-visible,
summary:focus-visible {
  outline: 2px solid var(--green);
  outline-offset: 3px;
}
details {
  margin-top: 24px;
}
summary {
  cursor: pointer;
}
details article {
  border-bottom: 1px solid var(--border);
  padding: 16px 0;
}
@media (max-width: 700px) {
  .update-trigger span {
    display: none;
  }
  .update-trigger {
    padding: 9px;
  }
}
</style>
