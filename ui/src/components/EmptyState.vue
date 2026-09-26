<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>
<template>
  <div class="empty-state">
    <div v-if="$slots.icon" class="empty-state-icon"><slot name="icon" /></div>
    <h2>{{ title }}</h2>
    <p>{{ description }}</p>
    <div v-if="$slots.default" class="empty-state-actions"><slot /></div>
  </div>
</template>
<style scoped>
.empty-state {
  padding: 40px 24px;
  border: 1px dashed var(--border);
  border-radius: 12px;
  text-align: center;
}
.empty-state-icon {
  display: grid;
  place-items: center;
  color: var(--green);
  margin-bottom: 16px;
}
.empty-state p {
  max-width: 520px;
  margin: 12px auto 0;
  color: var(--muted);
  line-height: 1.6;
}
.empty-state-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
  flex-wrap: wrap;
  margin-top: 22px;
}
</style>
