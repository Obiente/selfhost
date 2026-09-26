<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { sessionFetch } from './session';

export type VersionSelection = { image: string; allow_untested: boolean };
type Choice = { image: string; label: string; notes: string; tested: boolean };
const props = defineProps<{
  app: string;
  modelValue?: VersionSelection | null;
  optionsUrl?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{
  'update:modelValue': [selection: VersionSelection];
  valid: [valid: boolean];
}>();
const loading = ref(false),
  checking = ref(false),
  error = ref('');
const info = ref<{ default_image: string; current_image?: string; choices: Choice[] } | null>(null);
const selected = ref(''),
  custom = ref(''),
  acknowledged = ref(false);
const releases = ref<any>(null);
const reload = ref(0);
let generation = 0;
const image = computed(() => (selected.value === 'custom' ? custom.value.trim() : selected.value));
const choice = computed(() => info.value?.choices.find((item) => item.image === image.value));
const requiresAcknowledgement = computed(
  () => !!image.value && image.value !== info.value?.default_image && !choice.value?.tested,
);
const valid = computed(
  () =>
    !loading.value &&
    !!info.value &&
    !!image.value &&
    (!requiresAcknowledgement.value || acknowledged.value),
);
async function request(path: string, method = 'GET') {
  const response = await sessionFetch(path, { method });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error || 'Unable to load app versions.');
  return value;
}
watch(
  () => [props.app, props.optionsUrl, reload.value],
  async () => {
    const current = ++generation;
    loading.value = true;
    checking.value = false;
    info.value = null;
    error.value = '';
    releases.value = null;
    try {
      const value = await request(
        props.optionsUrl || `/api/catalog/${encodeURIComponent(props.app)}/versions`,
      );
      if (current !== generation) return;
      info.value = value;
      const initial = props.modelValue?.image || value.current_image || value.default_image;
      selected.value = value.choices.some((item: Choice) => item.image === initial)
        ? initial
        : 'custom';
      custom.value = initial;
      acknowledged.value = props.modelValue?.allow_untested || false;
    } catch (reason) {
      if (current === generation)
        error.value = reason instanceof Error ? reason.message : String(reason);
    } finally {
      if (current === generation) loading.value = false;
    }
  },
  { immediate: true },
);
watch(
  [image, acknowledged, valid],
  () => {
    emit('valid', valid.value);
    if (info.value)
      emit('update:modelValue', {
        image: image.value,
        allow_untested: requiresAcknowledgement.value && acknowledged.value,
      });
  },
  { immediate: true },
);
function changeImage() {
  acknowledged.value = false;
}
async function checkReleases() {
  const current = generation;
  checking.value = true;
  error.value = '';
  try {
    const value = await request(
      `/api/catalog/${encodeURIComponent(props.app)}/versions/check`,
      'POST',
    );
    if (current === generation) releases.value = value;
  } catch (reason) {
    if (current === generation)
      error.value = reason instanceof Error ? reason.message : String(reason);
  } finally {
    if (current === generation) checking.value = false;
  }
}
function chooseRelease(value: string) {
  custom.value = value;
  selected.value = 'custom';
  acknowledged.value = false;
}
</script>

<template>
  <fieldset class="version-picker" :disabled="disabled || loading" :aria-busy="loading">
    <legend>App version</legend>
    <p v-if="loading" role="status">Loading version choices…</p>
    <p v-if="error" class="version-error" role="alert">{{ error }}</p>
    <button v-if="error && !info" type="button" class="button small" @click="reload++">
      Reload version choices
    </button>
    <template v-if="info">
      <label
        >Container image
        <select v-model="selected" @change="changeImage">
          <option v-for="item in info.choices" :key="item.image" :value="item.image">
            {{ item.image }}{{ item.image === info.default_image ? ' · Recipe default' : ''
            }}{{ item.tested ? ' · Tested' : '' }}
          </option>
          <option value="custom">Choose another tag or digest…</option>
        </select>
      </label>
      <p v-if="choice" class="version-note">{{ choice.notes }}</p>
      <label v-if="selected === 'custom'"
        >Image tag or SHA-256 digest
        <input
          v-model="custom"
          class="mono"
          required
          autocomplete="off"
          spellcheck="false"
          :placeholder="info.default_image"
          @input="changeImage"
        />
        <span class="version-note"
          >Use the same image repository as this recipe. Other images belong in a custom
          setup.</span
        >
      </label>
      <label v-if="requiresAcknowledgement" class="version-ack">
        <input v-model="acknowledged" type="checkbox" required />
        <span
          >I have reviewed this version's configuration and migrations. Its compatibility with this
          recipe and automatic setup has not been verified.</span
        >
      </label>
      <details class="release-discovery">
        <summary>Check upstream releases</summary>
        <p class="version-note">
          Release discovery does not verify image availability or compatibility.
        </p>
        <button
          type="button"
          class="button small"
          :disabled="checking || disabled"
          @click="checkReleases"
        >
          {{ checking ? 'Checking…' : 'Check releases' }}
        </button>
        <div v-if="releases" aria-live="polite">
          <p v-if="!releases.available" class="version-note">{{ releases.reason }}</p>
          <template v-else>
            <p class="version-note">{{ releases.note }}</p>
            <p v-if="!releases.candidates?.length" class="version-note">
              No recent candidate releases were found.
            </p>
            <ul v-else class="release-list">
              <li v-for="release in releases.candidates" :key="release.image">
                <span class="mono">{{ release.image }}</span>
                <button type="button" class="button small" @click="chooseRelease(release.image)">
                  Select untested version
                </button>
              </li>
            </ul>
          </template>
        </div>
      </details>
    </template>
  </fieldset>
</template>

<style scoped>
.version-picker {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 16px;
  margin: 16px 0;
  min-width: 0;
}
.version-picker legend {
  padding: 0 5px;
  font-weight: 600;
}
.version-picker label {
  display: grid;
  gap: 8px;
  font-size: 13px;
  margin: 10px 0;
}
.version-picker input:not([type='checkbox']),
.version-picker select {
  width: 100%;
  min-width: 0;
}
.version-note {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  overflow-wrap: anywhere;
}
.version-picker .version-ack {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  line-height: 1.6;
}
.version-ack input {
  width: 17px;
  height: 17px;
  margin-top: 3px;
  flex: none;
}
.version-error {
  color: var(--red, #eb998e);
  font-size: 13px;
}
.release-discovery {
  margin-top: 12px;
  font-size: 13px;
}
.release-discovery summary {
  cursor: pointer;
  padding: 6px 0;
}
.release-list {
  list-style: none;
  margin: 12px 0 0;
  padding: 0;
  display: grid;
  gap: 10px;
}
.release-list li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}
.release-list .mono {
  font-size: 12px;
  overflow-wrap: anywhere;
}
</style>
