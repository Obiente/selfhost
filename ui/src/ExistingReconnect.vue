<script setup lang="ts">
import { ref, watch } from 'vue';
import BaseDialog from './components/BaseDialog.vue';
import ContainerPicker from './components/ContainerPicker.vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  app: any;
  servers: any[];
}>();
const emit = defineEmits<{ close: []; saved: [] }>();
const server = ref(''),
  container = ref(''),
  plan = ref<any>(null),
  busy = ref(false),
  error = ref('');
watch(
  () => props.app,
  (app) => {
    server.value = app?.server_id || '';
    container.value = '';
    plan.value = null;
    error.value = '';
  },
  { immediate: true },
);
watch([server, container], () => (plan.value = null));
async function review() {
  busy.value = true;
  error.value = '';
  try {
    plan.value = await props.api(`/existing/${props.app.id}/reconnect/plan`, 'POST', {
      server_id: server.value,
      container: container.value,
    });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function apply() {
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/existing/${props.app.id}/reconnect/apply`, 'POST', {
      server_id: server.value,
      container: container.value,
      revision: plan.value.revision,
    });
    emit('saved');
    emit('close');
  } catch (e) {
    error.value = (e as Error).message;
    plan.value = null;
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <BaseDialog
    :open="!!app"
    title="Reconnect an app"
    description="Choose its current container. Selfhost checks its identity and reconnects it with read-only permissions."
    :busy="busy"
    @update:open="
      (value) => {
        if (!value) emit('close');
      }
    "
  >
    <form v-if="app" @submit.prevent="review">
      <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
      <p>
        <strong>{{ app.name }}</strong> · {{ app.url }}
      </p>
      <label
        >Server<select v-model="server" required :disabled="busy">
          <option value="">Choose a connected Docker server</option>
          <option
            v-for="item in servers.filter((s) => s.provider !== 'proxmox_ssh')"
            :key="item.id"
            :value="item.id"
          >
            {{ item.name }}
          </option>
        </select></label
      >
      <ContainerPicker
        :api="api"
        :server="server"
        :profile="app.profile.id"
        v-model="container"
        :disabled="busy"
      />
      <button class="button" :disabled="busy || !server || !container">Review replacement</button>
    </form>
    <section v-if="plan" aria-label="Review replacement">
      <h3>Connect {{ plan.replacement.name }}</h3>
      <p>{{ plan.replacement.image }} · {{ plan.replacement.status }}</p>
      <p>
        The app URL stays the same. Previously enabled management actions will be disabled. The
        containers and their data are not changed.
      </p>
      <button type="button" class="button primary" :disabled="busy" @click="apply">
        Reconnect read-only
      </button>
    </section>
  </BaseDialog>
</template>
