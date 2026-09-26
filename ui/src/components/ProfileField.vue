<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue';
import StringListInput from './StringListInput.vue';
import StructuredValueInput from './StructuredValueInput.vue';
const props = defineProps<{ field: any; modelValue: any; disabled?: boolean }>();
const emit = defineEmits<{ 'update:modelValue': [value: any] }>();
const id = useId(),
  raw = ref(false),
  source = ref(''),
  error = ref(''),
  rawInput = ref<HTMLTextAreaElement>();
const list = computed(() =>
  ['string_list', 'arguments', 'string_array', 'paths'].includes(props.field.kind),
);
watch(raw, (value) => {
  if (value) source.value = JSON.stringify(props.modelValue ?? {}, null, 2);
  error.value = '';
});
function readRaw() {
  try {
    const value = JSON.parse(source.value);
    if (!value || typeof value !== 'object') throw Error('Use named fields or a list.');
    emit('update:modelValue', value);
    error.value = '';
    rawInput.value?.setCustomValidity('');
  } catch (e) {
    error.value = (e as Error).message;
    rawInput.value?.setCustomValidity('Enter a valid object or list, then use the edited value.');
  }
}
</script>
<template>
  <div class="profile-field">
    <fieldset v-if="field.kind === 'ca_pool'" :disabled="disabled">
      <legend>{{ field.label }}</legend>
      <label :for="id">Certificate authority</label>
      <select
        :id="id"
        :value="modelValue == null ? 'system' : 'file'"
        @change="
          emit(
            'update:modelValue',
            ($event.target as HTMLSelectElement).value === 'system'
              ? null
              : { provider: 'file', pem_files: [] },
          )
        "
      >
        <option value="system">System certificate authorities</option>
        <option value="file">Private CA files on the proxy</option>
      </select>
      <StringListInput
        v-if="modelValue != null"
        :model-value="modelValue.pem_files || []"
        label="CA certificate file paths"
        :disabled="disabled"
        @update:model-value="emit('update:modelValue', { provider: 'file', pem_files: $event })"
      />
    </fieldset>
    <fieldset v-else-if="field.kind === 'certificate'" :disabled="disabled">
      <legend>{{ field.label }}</legend>
      <label :for="id">Certificate</label>
      <select
        :id="id"
        :value="modelValue === 'new' ? 'new' : 'existing'"
        @change="
          emit(
            'update:modelValue',
            ($event.target as HTMLSelectElement).value === 'new' ? 'new' : '',
          )
        "
      >
        <option value="new">Request a new certificate automatically</option>
        <option value="existing">Use an existing certificate</option>
      </select>
      <template v-if="modelValue !== 'new'"
        ><label :for="`${id}-certificate`">Certificate ID in your proxy</label
        ><input
          :id="`${id}-certificate`"
          type="number"
          min="1"
          required
          :value="modelValue || ''"
          @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      /></template>
    </fieldset>
    <StringListInput
      v-else-if="list"
      :model-value="modelValue || []"
      :label="field.label"
      :ordered="field.kind === 'arguments'"
      :disabled="disabled"
      @update:model-value="emit('update:modelValue', $event)"
    />
    <template v-else-if="field.kind === 'json'">
      <StructuredValueInput
        v-if="!raw"
        :model-value="modelValue ?? {}"
        :label="field.label"
        container-only
        :disabled="disabled"
        @update:model-value="emit('update:modelValue', $event)"
      />
      <label class="check-row"
        ><input type="checkbox" v-model="raw" :disabled="disabled" />Advanced JSON editor</label
      >
      <template v-if="raw"
        ><label :for="id">{{ field.label }}</label
        ><textarea
          :id="id"
          ref="rawInput"
          v-model="source"
          rows="8"
          :disabled="disabled"
          @input="rawInput?.setCustomValidity('Use the edited value before continuing.')"
        /><button type="button" class="button" :disabled="disabled" @click="readRaw">
          Use edited value
        </button>
        <p v-if="error" role="alert">{{ error }}</p></template
      >
    </template>
    <template v-else>
      <label :for="id">{{ field.label }}</label>
      <input
        v-if="['boolean', 'bool', 'accept'].includes(field.kind)"
        :id="id"
        type="checkbox"
        :checked="modelValue === true"
        :disabled="disabled"
        @change="emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
      />
      <select
        v-else-if="field.choices?.length"
        :id="id"
        :value="modelValue ?? ''"
        :disabled="disabled"
        :required="field.required"
        @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
      >
        <option value="">Not set</option>
        <option v-for="value in field.choices" :key="value" :value="value">{{ value }}</option>
      </select>
      <input
        v-else
        :id="id"
        :type="
          ['integer', 'number', 'port'].includes(field.kind)
            ? 'number'
            : field.kind === 'secret'
              ? 'password'
              : field.kind === 'email'
                ? 'email'
                : 'text'
        "
        :value="modelValue ?? ''"
        :disabled="disabled"
        :required="field.required"
        :min="field.minimum"
        :max="field.maximum"
        :minlength="field.minimum_length"
        :maxlength="field.maximum_length"
        :autocomplete="field.kind === 'secret' ? 'new-password' : 'off'"
        :aria-describedby="field.description ? `${id}-help` : undefined"
        @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      />
    </template>
    <small v-if="field.description" :id="`${id}-help`">{{ field.description }}</small>
  </div>
</template>
<style scoped>
.profile-field {
  margin: 16px 0;
}
label {
  display: block;
  margin-bottom: 6px;
}
input:not([type='checkbox']),
select,
textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 8px 0;
}
.check-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
small {
  color: var(--muted);
}
</style>
