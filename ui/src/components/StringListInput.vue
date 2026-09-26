<script setup lang="ts">
import { nextTick, ref, useId } from 'vue';
const props = withDefaults(
  defineProps<{
    modelValue?: string[];
    label: string;
    disabled?: boolean;
    maximum?: number;
    ordered?: boolean;
  }>(),
  { maximum: 64 },
);
const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>();
const group = useId();
const root = ref<HTMLElement>();
function update(index: number, value: string) {
  const next = [...(props.modelValue || [])];
  next[index] = value;
  emit('update:modelValue', next);
}
async function add() {
  emit('update:modelValue', [...(props.modelValue || []), '']);
  await nextTick();
  root.value
    ?.querySelectorAll<HTMLInputElement>('input')
    .item((props.modelValue?.length || 1) - 1)
    ?.focus();
}
async function remove(index: number) {
  emit(
    'update:modelValue',
    (props.modelValue || []).filter((_, i) => i !== index),
  );
  await nextTick();
  const fields = root.value?.querySelectorAll<HTMLInputElement>('input');
  if (fields?.length) fields.item(Math.min(index, fields.length - 1))?.focus();
  else root.value?.querySelector<HTMLButtonElement>('[data-add]')?.focus();
}
function move(index: number, offset: number) {
  const next = [...(props.modelValue || [])];
  [next[index], next[index + offset]] = [next[index + offset]!, next[index]!];
  emit('update:modelValue', next);
}
</script>
<template>
  <fieldset ref="root" class="list-input" :disabled="disabled" :aria-describedby="`${group}-help`">
    <legend>{{ label }}</legend>
    <p :id="`${group}-help`" class="field-help">
      {{
        ordered
          ? 'One argument per row, in the order it should run. Spaces and commas stay part of the argument.'
          : 'Add one value per row. Spaces and commas stay part of each value.'
      }}
    </p>
    <p v-if="!modelValue?.length" class="field-help">No values added.</p>
    <div v-for="(value, index) in modelValue || []" :key="index" class="list-row">
      <label class="sr-only" :for="`${group}-${index}`">{{ label }}: value {{ index + 1 }}</label>
      <input
        :id="`${group}-${index}`"
        :value="value"
        autocomplete="off"
        @input="update(index, ($event.target as HTMLInputElement).value)"
      />
      <button
        v-if="ordered"
        type="button"
        class="button"
        :disabled="index === 0"
        :aria-label="`Move ${label} value ${index + 1} up`"
        @click="move(index, -1)"
      >
        ↑
      </button>
      <button
        v-if="ordered"
        type="button"
        class="button"
        :disabled="index === (modelValue?.length || 0) - 1"
        :aria-label="`Move ${label} value ${index + 1} down`"
        @click="move(index, 1)"
      >
        ↓
      </button>
      <button
        type="button"
        class="button"
        :aria-label="`Remove ${label} value ${index + 1}`"
        @click="remove(index)"
      >
        Remove
      </button>
    </div>
    <button
      data-add
      type="button"
      class="button"
      :disabled="(modelValue?.length || 0) >= maximum"
      @click="add"
    >
      Add {{ ordered ? 'argument' : 'value' }}
    </button>
  </fieldset>
</template>
<style scoped>
.list-input {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 14px;
  min-width: 0;
}
legend {
  padding: 0 5px;
  font-weight: 600;
}
.list-row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin: 8px 0;
}
.list-row input {
  flex: 1;
  min-width: 0;
  width: 100%;
}
.field-help {
  color: var(--muted);
  font-size: 13px;
  margin: 5px 0 10px;
}
button:focus-visible,
input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}
@media (max-width: 520px) {
  .list-row {
    flex-wrap: wrap;
  }
  .list-row input {
    flex-basis: 100%;
  }
}
</style>
