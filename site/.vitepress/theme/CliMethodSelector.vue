<script setup lang="ts">
import { onMounted } from 'vue';
import release from '../../../packages/selfhost/package.json';
import {
  cliMethod,
  cliCommands,
  loadCliMethod,
  selectCliMethod,
  type CliMethod,
} from './cliMethod';
onMounted(loadCliMethod);
</script>
<template>
  <aside class="cli-method-selector" aria-label="Documentation command preferences">
    <label for="docs-cli-method">Run commands with</label
    ><select
      id="docs-cli-method"
      :value="cliMethod"
      @change="selectCliMethod(($event.target as HTMLSelectElement).value as CliMethod)"
    >
      <option value="installed">Installed CLI (Cargo / binary)</option>
      <option value="npx">npx (Node.js)</option>
      <option value="pnpm">pnpm dlx</option>
      <option value="pnpx">pnpx</option></select
    ><span aria-live="polite"
      >Examples use <code>{{ cliCommands[cliMethod] }}</code></span
    ><small class="release-status">Documentation for {{ release.version }}.</small>
  </aside>
</template>
<style scoped>
.cli-method-selector {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px 14px;
  padding: 16px;
  margin: 0 0 28px;
  background: var(--vp-c-bg-soft);
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  font-size: 13px;
  line-height: 1.6;
}
.cli-method-selector label {
  font-weight: 600;
}
.cli-method-selector select {
  appearance: auto;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
  background: var(--vp-c-bg);
  padding: 8px;
  color: var(--vp-c-text-1);
  font: inherit;
  max-width: 100%;
}
.cli-method-selector select:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
.cli-method-selector span {
  color: var(--vp-c-text-2);
}
.release-status {
  flex-basis: 100%;
  color: var(--vp-c-text-2);
}
code {
  color: var(--vp-c-brand-1);
}
</style>
