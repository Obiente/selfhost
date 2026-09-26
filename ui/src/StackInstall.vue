<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  category?: string;
}>();
const emit = defineEmits<{ created: [id: string] }>();
const stacks = ref<any[]>([]),
  servers = ref<any[]>([]),
  selected = ref(''),
  name = ref(''),
  server = ref('local'),
  inputs = ref<Record<string, any>>({}),
  error = ref(''),
  busy = ref(false),
  result = ref<any>(null);
const acknowledgements = ref<string[]>([]);
const identityOnly = computed(() => props.category === 'Identity');
const blueprint = computed(() => stacks.value.find((s) => s.id === selected.value));
watch(selected, () => {
  acknowledgements.value = [];
  inputs.value = Object.fromEntries(
    Object.entries(blueprint.value?.inputs || {}).map(([key, v]: [string, any]) => [
      key,
      v.default,
    ]),
  );
  name.value = blueprint.value?.name || '';
  result.value = null;
});
onMounted(async () => {
  try {
    [stacks.value, servers.value] = await Promise.all([
      props.api('/stacks'),
      props.api('/servers'),
    ]);
  } catch (e) {
    error.value = (e as Error).message;
  }
});
async function create() {
  busy.value = true;
  error.value = '';
  try {
    result.value = await props.api('/stacks', 'POST', {
      blueprint: selected.value,
      name: name.value,
      server_id: server.value,
      inputs: inputs.value,
      acknowledgements: acknowledgements.value,
    });
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <section class="panel stack-install">
    <details class="host-new">
      <summary>{{ identityOnly ? 'Host a new identity provider' : 'Deploy an app stack' }}</summary>
      <h2>{{ identityOnly ? 'Create an identity project' : 'Create a stack project' }}</h2>
      <p>
        Create an editable project with its database, persistent storage and generated credentials.
      </p>
      <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
      <form v-if="!result" @submit.prevent="create">
        <label
          >{{ identityOnly ? 'Identity provider' : 'App stack'
          }}<select v-model="selected" required>
            <option value="" disabled>Choose a stack</option>
            <option
              v-for="s in stacks.filter((s) => !category || s.category === category)"
              :key="s.id"
              :value="s.id"
            >
              {{ s.name }}
            </option>
          </select></label
        ><template v-if="blueprint"
          ><p>{{ blueprint.description }}</p>
          <div class="fields">
            <label>Project name<input v-model="name" required /></label
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
            ><label v-for="(field, key) in blueprint.inputs" :key="key"
              >{{ field.label
              }}<input
                v-if="field.kind === 'port'"
                v-model.number="inputs[key]"
                type="number"
                min="1024"
                max="65535"
                required /><input
                v-else
                v-model="inputs[key]"
                :type="field.kind === 'email' ? 'email' : 'text'"
                required
            /></label>
          </div>
          <fieldset v-if="blueprint.requirements?.length">
            <legend>Review deployment requirements</legend>
            <label
              v-for="requirement in blueprint.requirements"
              :key="requirement.id"
              class="requirement"
            >
              <input
                v-model="acknowledgements"
                type="checkbox"
                :value="requirement.id"
                required
              />{{ requirement.label }}
            </label>
          </fieldset>
          <p>
            The project is created stopped so you can review its configuration and connect your TLS
            proxy before starting it.
          </p>
          <button
            class="button primary"
            :disabled="
              busy ||
              (blueprint.requirements || []).some(
                (item: any) => !acknowledgements.includes(item.id),
              )
            "
          >
            Create project
          </button></template
        >
      </form>
      <div v-else>
        <h3>{{ result.project.name }} is ready to configure</h3>
        <ol>
          <li v-for="step in result.instructions" :key="step">{{ step }}</li>
        </ol>
        <button class="button primary" @click="emit('created', result.project.id)">
          Open project</button
        ><button class="button" @click="result = null">Create another</button>
      </div>
    </details>
  </section>
</template>
<style scoped>
summary {
  cursor: pointer;
  font-size: 18px;
  font-weight: 600;
  padding: 8px 0;
  color: var(--green);
}
summary:focus-visible {
  outline: 2px solid var(--green);
  outline-offset: 4px;
}
h2 {
  margin-top: 22px;
}
.stack-install {
  max-width: 900px;
  margin-bottom: 24px;
}
.fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0 24px;
}
label {
  display: block;
  margin: 16px 0;
}
input,
select {
  display: block;
  width: 100%;
  margin-top: 8px;
  box-sizing: border-box;
}
li {
  margin: 12px 0;
  line-height: 1.6;
}
.button {
  margin-right: 10px;
}
.requirement {
  display: flex;
  align-items: start;
  gap: 10px;
}
.requirement input {
  width: auto;
  flex-shrink: 0;
}
@media (max-width: 700px) {
  .fields {
    grid-template-columns: 1fr;
  }
}
</style>
