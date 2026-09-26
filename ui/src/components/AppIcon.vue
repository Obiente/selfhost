<script setup lang="ts">
import { computed, ref, watch } from 'vue';
const props = withDefaults(defineProps<{ src?: string; alt?: string }>(), { src: '', alt: '' });
const failed = ref(false);
watch(
  () => props.src,
  () => {
    failed.value = false;
  },
);
const source = computed(() =>
  failed.value || !props.src ? '/brand/selfhost-monogram.svg' : props.src,
);
</script>
<template>
  <img :src="source" :alt="alt" referrerpolicy="no-referrer" @error="failed = true" />
</template>
