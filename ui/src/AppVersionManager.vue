<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import AppVersionPicker, { type VersionSelection } from './AppVersionPicker.vue';
import { sessionFetch } from './session';
import { useAsyncTask } from './composables/useAsyncTask';
const props = defineProps<{ projectId: string; service: string; writable: boolean }>();
const emit = defineEmits<{ changed: [] }>();
const { busy, error, run } = useAsyncTask();
const selection = ref<VersionSelection | null>(null),
  valid = ref(false),
  review = ref<any>(null),
  result = ref<any>(null);
const base = computed(
  () =>
    `/api/projects/${encodeURIComponent(props.projectId)}/services/${encodeURIComponent(props.service)}`,
);
watch(
  [selection, () => props.projectId, () => props.service],
  () => {
    review.value = null;
    result.value = null;
  },
  { deep: true },
);
async function request(path: string, body: unknown) {
  const response = await sessionFetch(base.value + path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || 'Unable to save app version.');
  return value;
}
function plan() {
  return run(async () => {
    result.value = null;
    review.value = await request('/version/plan', selection.value);
  });
}
function apply() {
  return run(async () => {
    result.value = await request('/version/apply', {
      selection: selection.value,
      revision: review.value.revision,
    });
    review.value = null;
    emit('changed');
  });
}
</script>
<template>
  <section class="version-manager">
    <h3>App version</h3>
    <p>
      Choose the image used for the next deployment. Review configuration and database migrations,
      and back up app data before changing versions.
    </p>
    <AppVersionPicker
      :key="base"
      v-model="selection"
      :app="service"
      :options-url="`${base}/versions`"
      :disabled="!writable || busy"
      @valid="valid = $event"
    />
    <p v-if="error" class="version-error" role="alert">{{ error }}</p>
    <button type="button" class="button" :disabled="!writable || busy || !valid" @click="plan">
      Review version change
    </button>
    <div v-if="review" class="version-review" aria-live="polite">
      <h4>Review saved image</h4>
      <dl>
        <dt>Current configuration</dt>
        <dd class="mono">{{ review.from }}</dd>
        <dt>New configuration</dt>
        <dd class="mono">{{ review.to }}</dd>
      </dl>
      <ul>
        <li v-for="warning in review.warnings" :key="warning">{{ warning }}</li>
      </ul>
      <button type="button" class="button primary" :disabled="!writable || busy" @click="apply">
        {{ busy ? 'Saving…' : 'Save version choice' }}
      </button>
    </div>
    <p v-if="result" role="status" class="version-result">
      Saved {{ result.image }}. Running containers have not changed.
      {{
        result.restart_required
          ? 'When ready, use the service Update action to pull this image and recreate the service.'
          : ''
      }}
    </p>
  </section>
</template>
<style scoped>
.version-manager {
  border: 1px solid var(--border);
  padding: 22px;
  border-radius: 10px;
  margin-top: 20px;
}
.version-manager h3 {
  margin: 0 0 8px;
  font-size: 16px;
}
.version-manager p,
.version-review li {
  font-size: 13px;
  line-height: 1.7;
  color: var(--muted);
}
.version-error {
  color: var(--red, #eb998e) !important;
}
.version-review {
  border-top: 1px solid var(--border);
  margin-top: 18px;
  padding-top: 12px;
}
.version-review dl {
  display: grid;
  gap: 6px;
  font-size: 13px;
}
.version-review dd {
  margin: 0 0 6px;
  overflow-wrap: anywhere;
}
.version-review ul {
  padding-left: 20px;
}
.version-result {
  color: var(--green) !important;
}
</style>
