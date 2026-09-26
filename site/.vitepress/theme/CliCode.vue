<script setup lang="ts">
import { cliMethod } from './cliMethod';
defineProps<{ variants: Record<string, string> }>();
</script>
<!-- HTML comes exclusively from the local Markdown renderer at build time. -->
<template><div class="cli-code" v-html="variants[cliMethod]" /></template>
