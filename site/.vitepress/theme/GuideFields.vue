<script setup lang="ts">
defineProps<{ fields: any[] }>();
</script>
<template>
  <div class="guide-fields">
    <dl>
      <div v-for="field in fields" :key="field.id">
        <dt>
          <strong>{{ field.label }}</strong> <code>{{ field.id }}</code>
        </dt>
        <dd>
          {{ field.description }}
          <span class="field-type"
            >{{ field.kind }}<template v-if="field.required"> · required</template
            ><template v-if="field.advanced"> · advanced</template></span
          >
          <span v-if="field.choices.length">Choices: {{ field.choices.join(', ') }}</span>
          <span v-if="field.default !== undefined"
            >Default: <code>{{ JSON.stringify(field.default) }}</code></span
          >
          <span v-if="field.minimum !== undefined || field.maximum !== undefined"
            >Range: {{ field.minimum ?? 'unbounded' }} to {{ field.maximum ?? 'unbounded' }}</span
          >
          <span v-if="field.minimum_length || field.maximum_length"
            >Length: {{ field.minimum_length || 0 }} to
            {{ field.maximum_length || 'unbounded' }} characters</span
          >
        </dd>
      </div>
    </dl>
  </div>
</template>
<style scoped>
dl > div {
  border-bottom: 1px solid var(--vp-c-divider);
  padding: 12px 0;
}
dt {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: baseline;
}
dd {
  margin: 4px 0 0;
  font-size: 14px;
  color: var(--vp-c-text-2);
}
dd span {
  display: block;
}
code {
  overflow-wrap: anywhere;
}
.field-type {
  font-size: 12px;
}
</style>
