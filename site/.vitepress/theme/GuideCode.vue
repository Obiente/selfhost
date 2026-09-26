<script setup lang="ts">
import { ref } from 'vue';
const props = defineProps<{ text: string }>();
const copied = ref('');
async function copy() {
  try {
    await navigator.clipboard.writeText(props.text);
    copied.value = 'Copied';
  } catch {
    copied.value = 'Select the text to copy it.';
  }
}
</script>
<template>
  <div class="guide-code">
    <pre tabindex="0"><code>{{ text }}</code></pre>
    <button type="button" @click="copy">Copy</button>
    <span class="copy-status" role="status">{{ copied }}</span>
  </div>
</template>
<style scoped>
.guide-code {
  position: relative;
  margin: 16px 0;
}
pre {
  padding: 44px 16px 18px;
  background: var(--vp-code-block-bg);
  overflow: auto;
  border-radius: 8px;
}
code {
  font-size: 13px;
  white-space: pre;
}
button {
  position: absolute;
  right: 8px;
  top: 8px;
  padding: 2px 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 5px;
  font-size: 12px;
}
button:focus-visible,
pre:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 2px;
}
.copy-status {
  font-size: 12px;
  color: var(--vp-c-text-2);
}
</style>
