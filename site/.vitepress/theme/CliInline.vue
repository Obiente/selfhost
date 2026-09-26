<script setup lang="ts">
import { cliMethod } from './cliMethod';
defineProps<{ variants: Record<string, string> }>();
</script>
<template>
  <code>{{ variants[cliMethod] }}</code>
</template>
