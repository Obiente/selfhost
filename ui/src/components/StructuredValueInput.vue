<script setup lang="ts">
import { computed, ref, useId } from 'vue';
const props = withDefaults(
  defineProps<{
    modelValue: any;
    label: string;
    disabled?: boolean;
    depth?: number;
    containerOnly?: boolean;
  }>(),
  { depth: 0 },
);
const emit = defineEmits<{ 'update:modelValue': [value: any] }>();
const id = useId(),
  key = ref(''),
  error = ref('');
const kind = computed(() =>
  Array.isArray(props.modelValue)
    ? 'list'
    : props.modelValue === null || props.modelValue === undefined
      ? 'empty'
      : typeof props.modelValue === 'object'
        ? 'group'
        : typeof props.modelValue,
);
const choices = computed(() =>
  props.containerOnly
    ? ['group', 'list']
    : ['string', 'number', 'boolean', 'group', 'list', 'empty'],
);
const entries = computed<[string, any][]>(() =>
  ['group', 'list'].includes(kind.value) ? Object.entries(props.modelValue) : [],
);
function changeKind(value: string) {
  emit(
    'update:modelValue',
    value === 'group'
      ? {}
      : value === 'list'
        ? []
        : value === 'boolean'
          ? false
          : value === 'number'
            ? 0
            : value === 'empty'
              ? null
              : '',
  );
}
function update(name: string, value: any) {
  const next = Array.isArray(props.modelValue) ? [...props.modelValue] : { ...props.modelValue };
  next[name as any] = value;
  emit('update:modelValue', next);
}
function remove(name: string) {
  if (Array.isArray(props.modelValue))
    emit(
      'update:modelValue',
      props.modelValue.filter((_: any, i: number) => i !== Number(name)),
    );
  else {
    const next = { ...props.modelValue };
    delete next[name];
    emit('update:modelValue', next);
  }
}
function add() {
  error.value = '';
  if (kind.value === 'list') {
    emit('update:modelValue', [...props.modelValue, '']);
    return;
  }
  const name = key.value.trim();
  if (!name) {
    error.value = 'Enter a field name.';
    return;
  }
  if (Object.hasOwn(props.modelValue, name)) {
    error.value = 'That field already exists.';
    return;
  }
  emit('update:modelValue', { ...props.modelValue, [name]: '' });
  key.value = '';
}
function number(input: HTMLInputElement) {
  const value = input.value;
  input.setCustomValidity('');
  if (value.trim() && Number.isFinite(Number(value))) {
    error.value = '';
    emit('update:modelValue', Number(value));
  } else {
    error.value = 'Enter a valid number.';
    input.setCustomValidity(error.value);
  }
}
</script>
<template>
  <fieldset class="structured-input" :disabled="disabled">
    <legend>{{ label }}</legend>
    <label :for="`${id}-kind`">Value type</label>
    <select
      :id="`${id}-kind`"
      :value="kind"
      @change="changeKind(($event.target as HTMLSelectElement).value)"
    >
      <option v-for="value in choices" :key="value" :value="value">
        {{
          {
            string: 'Text',
            number: 'Number',
            boolean: 'On / off',
            group: 'Named fields',
            list: 'List',
            empty: 'Empty',
          }[value as 'string']
        }}
      </option>
    </select>
    <template v-if="kind === 'string'"
      ><label class="sr-only" :for="`${id}-value`">{{ label }}</label
      ><textarea
        :id="`${id}-value`"
        :value="modelValue"
        rows="2"
        @input="emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
      />
    </template>
    <template v-else-if="kind === 'number'"
      ><label class="sr-only" :for="`${id}-value`">{{ label }}</label
      ><input
        :id="`${id}-value`"
        type="number"
        step="any"
        :value="modelValue"
        required
        @input="number($event.target as HTMLInputElement)"
    /></template>
    <label v-else-if="kind === 'boolean'" class="check-row"
      ><input
        type="checkbox"
        :checked="modelValue"
        @change="emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
      />Enabled</label
    >
    <template v-else-if="kind === 'group' || kind === 'list'">
      <p v-if="!entries.length" class="field-help">
        {{ kind === 'list' ? 'No items added.' : 'No fields added.' }}
      </p>
      <div v-for="[name, value] in entries" :key="name" class="structured-row">
        <StructuredValueInput
          v-if="depth < 16"
          :model-value="value"
          :label="kind === 'list' ? `Item ${Number(name) + 1}` : name"
          :depth="depth + 1"
          :disabled="disabled"
          @update:model-value="update(name, $event)"
        />
        <p v-else>Use the advanced editor to change deeper values.</p>
        <button
          type="button"
          class="button"
          :aria-label="`Remove ${kind === 'list' ? 'item ' + (Number(name) + 1) : name}`"
          @click="remove(name)"
        >
          Remove
        </button>
      </div>
      <div v-if="entries.length < 128" class="add-field">
        <label v-if="kind === 'group'" :for="`${id}-key`"
          >New field name<input
            :id="`${id}-key`"
            v-model="key"
            autocomplete="off"
            @keydown.enter.prevent="add"
        /></label>
        <button type="button" class="button" @click="add">
          Add {{ kind === 'list' ? 'item' : 'field' }}
        </button>
      </div>
    </template>
    <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
  </fieldset>
</template>
<style scoped>
.structured-input {
  min-width: 0;
  padding: 14px;
  margin: 10px 0;
  border: 1px solid var(--border);
  border-radius: 8px;
}
legend {
  padding: 0 5px;
  font-weight: 600;
}
label {
  display: block;
  font-size: 13px;
}
select,
textarea,
input:not([type='checkbox']) {
  display: block;
  width: 100%;
  margin: 7px 0;
  box-sizing: border-box;
}
.structured-row {
  border-left: 2px solid var(--border);
  padding-left: 10px;
  margin: 10px 0;
}
.structured-row > .structured-input {
  margin-bottom: 6px;
}
.field-help {
  color: var(--muted);
}
.add-field {
  margin-top: 12px;
}
button:focus-visible,
select:focus-visible,
textarea:focus-visible,
input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 3px;
}
</style>
