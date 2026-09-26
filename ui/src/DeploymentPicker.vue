<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import BaseDialog from './components/BaseDialog.vue';
import { useAsyncTask } from './composables/useAsyncTask';
import AppVersionPicker, { type VersionSelection } from './AppVersionPicker.vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  app: string;
  open: boolean;
}>();
const emit = defineEmits<{ 'update:open': [value: boolean]; created: [id: string] }>();
const { busy, error, run } = useAsyncTask();
const methods = ref<any[]>([]),
  servers = ref<any[]>([]),
  method = ref(''),
  name = ref(''),
  server = ref('local'),
  inputs = ref<Record<string, any>>({}),
  acknowledgements = ref<string[]>([]),
  result = ref<any>(null);
const choices = computed(() => methods.value.filter((item) => item.app === props.app));
const selected = computed(() => choices.value.find((item) => item.id === method.value));
const version = ref<VersionSelection | null>(null),
  versionValid = ref(false);
watch(method, () => {
  version.value = null;
  versionValid.value = false;
  inputs.value = Object.fromEntries(
    Object.entries(selected.value?.configuration?.inputs || {}).map(
      ([key, value]: [string, any]) => [key, value.default],
    ),
  );
  acknowledgements.value = [];
});
watch(
  () => props.open,
  async (open) => {
    if (open) {
      result.value = null;
      name.value = props.app.charAt(0).toUpperCase() + props.app.slice(1);
      await run(load);
      method.value = (choices.value.find((item) => item.default) || choices.value[0])?.id || '';
    }
  },
);
async function load() {
  [methods.value, servers.value] = await Promise.all([
    props.api('/deployments'),
    props.api('/servers'),
  ]);
}
onMounted(async () => {
  if (props.open) {
    name.value = props.app.charAt(0).toUpperCase() + props.app.slice(1);
    await run(load);
    method.value = (choices.value.find((item) => item.default) || choices.value[0])?.id || '';
  }
});
async function create() {
  await run(async () => {
    result.value = await props.api('/deployments', 'POST', {
      app: props.app,
      method: method.value,
      name: name.value,
      server_id: server.value,
      inputs: inputs.value,
      acknowledgements: acknowledgements.value,
      ...(selected.value?.recipe ? { version: version.value } : {}),
    });
  });
}
</script>
<template>
  <BaseDialog
    :open="open"
    title="Choose your deployment"
    description="Pick how this app will run. You can review the complete configuration before starting it."
    :busy="busy"
    wide
    @update:open="emit('update:open', $event)"
    ><p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <form v-if="!result" @submit.prevent="create">
      <fieldset class="method-options">
        <legend>Deployment method</legend>
        <label
          v-for="item in choices"
          :key="item.id"
          class="method-option"
          :class="{ selected: method === item.id }"
          ><input
            v-model="method"
            type="radio"
            :value="item.id"
            name="deployment-method"
            required
          /><span
            ><strong>{{ item.name }}</strong
            ><small>{{ item.description }}</small></span
          ></label
        >
      </fieldset>
      <template v-if="selected"
        ><div class="deployment-fields">
          <label>Project name<input v-model="name" required maxlength="64" /></label
          ><label
            >Server<select v-model="server" required>
              <option
                v-for="s in servers.filter((s) => !s.read_only && s.provider !== 'proxmox_ssh')"
                :key="s.id"
                :value="s.id"
              >
                {{ s.name }}
              </option>
            </select></label
          ><label v-for="(field, key) in selected.configuration?.inputs || {}" :key="key"
            >{{ field.label
            }}<input
              v-if="field.kind === 'port'"
              v-model.number="inputs[key]"
              type="number"
              min="1024"
              max="65535"
              required /><input v-else v-model="inputs[key]" required
          /></label>
        </div>
        <AppVersionPicker
          v-if="selected.recipe"
          :key="method"
          v-model="version"
          :app="selected.recipe"
          :disabled="busy"
          @valid="versionValid = $event"
        />
        <fieldset
          v-if="selected.configuration?.requirements?.length"
          class="deployment-requirements"
        >
          <legend>Before you create this project</legend>
          <label v-for="requirement in selected.configuration.requirements" :key="requirement.id"
            ><input
              v-model="acknowledgements"
              type="checkbox"
              :value="requirement.id"
              :required="!acknowledgements.includes(requirement.id)"
            /><span>{{ requirement.label }}</span></label
          >
        </fieldset>
        <button class="button primary" :disabled="busy || (!!selected.recipe && !versionValid)">
          Create project
        </button></template
      >
    </form>
    <div v-else role="status">
      <h3>{{ result.project.name }} is ready to review</h3>
      <ol>
        <li v-for="instruction in result.instructions" :key="instruction">{{ instruction }}</li>
      </ol>
      <button
        class="button primary"
        @click="
          emit('created', result.project.id);
          emit('update:open', false);
        "
      >
        Open project
      </button>
    </div></BaseDialog
  >
</template>
<style scoped>
.method-options {
  border: 0;
  padding: 0;
  margin: 8px 0 24px;
  display: grid;
  gap: 12px;
}
.method-options legend {
  margin-bottom: 12px;
}
.method-option {
  display: flex;
  gap: 12px;
  padding: 18px;
  border: 1px solid var(--border);
  border-radius: 10px;
  cursor: pointer;
}
.method-option.selected {
  border-color: var(--green);
  background: #d8b87009;
}
.method-option input {
  width: 18px;
  height: 18px;
  margin: 1px 0;
  flex-shrink: 0;
}
.method-option small {
  display: block;
  color: var(--muted);
  line-height: 1.6;
  margin-top: 8px;
}
.deployment-fields {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}
.deployment-fields input,
.deployment-fields select {
  display: block;
  width: 100%;
  margin-top: 8px;
}
.deployment-requirements {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 18px;
  margin: 24px 0;
}
.deployment-requirements label {
  display: flex;
  gap: 12px;
  line-height: 1.7;
  font-size: 13px;
  margin: 12px 0;
}
.deployment-requirements input {
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  margin: 3px 0;
}
.button {
  margin-top: 20px;
}
li {
  line-height: 1.7;
  margin: 10px 0;
}
@media (max-width: 600px) {
  .deployment-fields {
    grid-template-columns: 1fr;
  }
}
</style>
