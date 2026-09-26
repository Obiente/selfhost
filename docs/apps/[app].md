---
outline: false
---

<script setup>
import { useData } from 'vitepress';
const { params } = useData();
</script>

<AppGuide :key="params.app" :app-id="params.app" />
