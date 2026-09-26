<script setup lang="ts">
import { computed, ref, watch } from 'vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  server: string;
  profile: string;
  modelValue: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();
const rows = ref<any[]>([]),
  loading = ref(false),
  error = ref(''),
  manual = ref(false);
let request = 0;
const selected = computed(() => rows.value.find((row) => row.id === props.modelValue));
async function load() {
  const current = ++request;
  rows.value = [];
  error.value = '';
  if (!props.server) {
    loading.value = false;
    return;
  }
  loading.value = true;
  try {
    const result = await props.api(
      `/servers/${encodeURIComponent(props.server)}/existing?profile=${encodeURIComponent(props.profile)}`,
    );
    if (current === request) {
      rows.value = result;
      if (!manual.value && !result.some((row: any) => row.id === props.modelValue))
        emit('update:modelValue', '');
    }
  } catch (e) {
    if (current === request) error.value = (e as Error).message;
  } finally {
    if (current === request) loading.value = false;
  }
}
watch(
  () => [props.server, props.profile],
  () => {
    emit('update:modelValue', '');
    manual.value = false;
    void load();
  },
  { immediate: true },
);
</script>
<template>
  <div class="container-picker">
    <p v-if="!server" class="field-help">Choose a server to find its running apps.</p>
    <template v-else>
      <p v-if="loading" role="status">Finding matching containers…</p>
      <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
      <label v-if="!manual"
        >Container<select
          :value="modelValue"
          :disabled="disabled || loading"
          required
          @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
        >
          <option value="">Choose a matching container</option>
          <option v-for="row in rows" :key="row.id" :value="row.id">
            {{ row.name }} · {{ row.image }} · {{ row.status }}
          </option>
        </select></label
      >
      <p v-if="!loading && !error && !rows.length" class="field-help">
        No matching containers were found. Check the selected app and server, or enter a container
        name.
      </p>
      <p v-if="selected" class="field-help">
        {{ selected.name }} will be linked read-only. Review management permissions separately.
      </p>
      <button type="button" class="button" :disabled="disabled || loading" @click="load">
        Refresh containers
      </button>
      <label class="check-row"
        ><input
          v-model="manual"
          type="checkbox"
          :disabled="disabled"
          @change="emit('update:modelValue', '')"
        />Enter a container name manually</label
      >
      <label v-if="manual"
        >Exact container name or ID<input
          :value="modelValue"
          :disabled="disabled"
          required
          maxlength="128"
          autocomplete="off"
          @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      /></label>
    </template>
  </div>
</template>
<style scoped>
label {
  display: block;
  margin: 12px 0;
}
select,
input:not([type='checkbox']) {
  display: block;
  width: 100%;
  margin: 6px 0;
  box-sizing: border-box;
}
.check-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.field-help {
  color: var(--muted);
  font-size: 13px;
}
</style>
