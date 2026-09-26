<script setup lang="ts">
import AppIcon from './components/AppIcon.vue';
import { sessionFetch } from './session';

import AppIntegration from './AppIntegration.vue';
import { computed, onBeforeUnmount, onMounted, onUnmounted, ref, watch } from 'vue';

import {
  Play,
  RefreshCw,
  Square,
  ArrowUpRight,
  Cpu,
  MemoryStick,
  ArrowLeftRight,
  HardDrive,
  Clock3,
} from 'lucide-vue-next';

type RecipeAction = {
  id: string;
  label: string;
  description: string;
  command: string[];
  confirm: boolean;
};

const props = defineProps<{
  url: string;
  writable: boolean;
  projectId: string;
  service: {
    app: string;
    image: string;
    port: number;
    definition: {
      name: string;
      icon: string;
      category: string;
      data_path?: string;
      actions?: RecipeAction[];
      integration?: unknown;
    };
  };
  available: boolean;
  busy: boolean;
  access: string;
  status: string;
  activities: { id: string; action: string; message: string; started_at: number; status: string }[];
}>();

const emit = defineEmits<{ close: []; changed: []; busy: [value: boolean] }>();

const view = ref('Overview');

const stats = ref<Record<string, string> | null>(null);

const sampledAt = ref(0);

const message = ref('');

const logs = ref('');

const output = ref('');

const error = ref('');

const loading = ref(false);

const pending = ref('');

const follow = ref(true);

const selectedAction = ref<{
  id: string;
  label: string;
  description: string;
  command?: string[];
  confirm?: boolean;
} | null>(null);

let generation = 0;

let requestInFlight = false;

const root = computed(() => `/api/projects/${props.projectId}/services/${props.service.app}`);

async function request(path: string, body?: unknown) {
  const response = await sessionFetch(root.value + path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: { ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const value = await response.json();

  if (!response.ok) throw new Error(value.error || 'Unable to read this service.');

  return value;
}

async function refresh() {
  if (requestInFlight || !props.available || ['Actions', 'App settings'].includes(view.value))
    return;

  const current = generation;

  const currentView = view.value;

  requestInFlight = true;
  loading.value = true;

  try {
    const value = await request(currentView === 'Overview' ? '/stats' : '/logs');

    if (current !== generation) return;

    error.value = '';

    if (currentView === 'Overview') {
      stats.value = value.sample;
      message.value = value.message || '';
      sampledAt.value = value.sampled_at || 0;
    } else logs.value = value.logs.replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '');
  } catch (e) {
    if (current === generation) {
      error.value = (e as Error).message;
      stats.value = null;
    }
  } finally {
    requestInFlight = false;
    loading.value = false;
  }
}

const actions = computed(() => [
  {
    id: 'start',
    label: 'Start',
    description: 'Start this service using its saved configuration.',
    icon: Play,
  },

  {
    id: 'stop',
    label: 'Stop',
    description: 'Stop this service and retain its application data.',
    confirm: true,
    icon: Square,
  },

  {
    id: 'restart',
    label: 'Restart',
    description: 'Restart this service without pulling an image.',
    confirm: true,
    icon: RefreshCw,
  },

  {
    id: 'update',
    label: 'Update image',
    description:
      'Pull the configured image tag and recreate this service if needed. A configuration snapshot is saved first.',
    confirm: true,
    icon: RefreshCw,
  },

  ...(props.service.definition.actions || []).map((action) => ({ ...action, icon: Play })),
]);

function choose(action: typeof selectedAction.value) {
  selectedAction.value = action;
  output.value = '';
}

async function execute() {
  if (!selectedAction.value || pending.value || props.busy) return;

  const action = selectedAction.value;

  pending.value = action.id;
  emit('busy', true);
  error.value = '';
  output.value = '';

  try {
    const result = await request('/actions', { action: action.id });
    output.value = result.output || 'Action completed.';
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    pending.value = '';
    emit('busy', false);
    emit('changed');
  }
}

watch([() => props.projectId, () => props.service.app, view], () => {
  generation++;
  stats.value = null;
  sampledAt.value = 0;
  message.value = '';
  logs.value = '';
  output.value = '';
  error.value = '';
  selectedAction.value = null;
  refresh();
});

let timer: ReturnType<typeof setInterval>;

watch(
  () => props.available,
  (available) => {
    if (available) refresh();
    else stats.value = null;
  },
);
onMounted(() => {
  refresh();
  timer = setInterval(() => {
    if (follow.value) refresh();
  }, 5000);
});

onBeforeUnmount(() => {
  if (pending.value) emit('busy', false);
});

onUnmounted(() => {
  generation++;
  clearInterval(timer);
});
</script>

<template>
  <section class="service-monitor" :aria-label="`${service.definition.name} details`">
    <header class="inspector-heading">
      <AppIcon :src="service.definition.icon" alt="" />
      <div>
        <div class="service-title">
          <h1>{{ service.definition.name }}</h1>
          <span class="status-pill" :class="{ running: status === 'Running' }"
            ><i></i>{{ status }}</span
          >
        </div>
        <p>
          {{ service.definition.category }}<span> / </span
          ><a v-if="url" :href="url" target="_blank" rel="noopener"
            >{{ url.replace('http://', '') }} <ArrowUpRight :size="14"
          /></a>
        </p>
      </div>
      <a v-if="url" class="button primary" :href="url" target="_blank" rel="noopener"
        >Open app<ArrowUpRight :size="17"
      /></a>
    </header>

    <nav class="tabs inspector-tabs" aria-label="Service details">
      <button
        v-for="name in [
          'Overview',
          'Logs',
          'Actions',
          ...(service.definition.integration ? ['App settings'] : []),
        ]"
        :key="name"
        :class="{ selected: view === name }"
        :disabled="!!pending"
        @click="view = name"
      >
        {{ name }}
      </button>
    </nav>

    <p v-if="!available" class="notice">Connect Docker to read this service or run an action.</p>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>

    <template v-if="view === 'Overview'">
      <div class="metric-grid">
        <div>
          <Cpu :size="22" />
          <section>
            <span>CPU</span><strong>{{ available ? stats?.CPUPerc || '-' : '-' }}</strong>
          </section>
        </div>

        <div>
          <MemoryStick :size="22" />
          <section>
            <span>Memory</span
            ><strong>{{ available ? stats?.MemUsage?.split(' / ')[0] || '-' : '-' }}</strong
            ><small v-if="stats && available"
              >{{ stats.MemPerc }} of {{ stats.MemUsage?.split(' / ')[1] }}</small
            >
          </section>
        </div>

        <div>
          <ArrowLeftRight :size="22" />
          <section>
            <span>Network (cumulative)</span
            ><strong>{{ available ? stats?.NetIO || '-' : '-' }}</strong
            ><small>In / Out</small>
          </section>
        </div>

        <div>
          <HardDrive :size="22" />
          <section>
            <span>Disk (cumulative)</span
            ><strong>{{ available ? stats?.BlockIO || '-' : '-' }}</strong
            ><small>Read / Write</small>
          </section>
        </div>
      </div>

      <div class="sample-line">
        <Clock3 :size="16" /><span>{{
          sampledAt && available
            ? `Sampled ${new Date(sampledAt * 1000).toLocaleTimeString()}`
            : loading
              ? 'Reading stats…'
              : message || 'No current sample'
        }}</span
        ><button
          class="text-button"
          :disabled="loading || !available"
          @click="refresh"
          aria-label="Refresh service stats"
        >
          <RefreshCw :size="15" :class="{ spin: loading }" />
        </button>
      </div>

      <p v-if="!service.port" class="form-help">This service has no published host port.</p>
      <p v-else-if="!url" class="form-help">
        This app is on a remote server. Use an SSH tunnel, or configure its network access and app
        hostname to open it here.
      </p>
      <section class="configuration-section">
        <h2 class="rule-heading">Service configuration</h2>
        <dl>
          <div>
            <dt>Image</dt>
            <dd>{{ service.image }}</dd>
          </div>
          <div>
            <dt>Host port</dt>
            <dd>{{ service.port || 'Not published' }}</dd>
          </div>
          <div>
            <dt>Access</dt>
            <dd>
              {{
                access === 'custom'
                  ? 'Defined in Compose'
                  : access === 'local'
                    ? 'Server only (loopback)'
                    : 'Local network'
              }}
            </dd>
          </div>
          <div>
            <dt>Data storage</dt>
            <dd>
              Docker volume
              <span v-if="service.definition.data_path">· {{ service.definition.data_path }}</span>
            </dd>
          </div>
        </dl>
      </section>
    </template>

    <template v-if="view === 'Logs'"
      ><div class="monitor-toolbar">
        <p>Latest 200 lines · timestamps included</p>
        <label><input v-model="follow" type="checkbox" />Auto refresh (5s)</label
        ><button class="button small" :disabled="loading || !available" @click="refresh">
          Refresh
        </button>
      </div>
      <pre class="service-output" tabindex="0" aria-label="Service logs">{{
        logs || (loading ? 'Reading logs…' : 'No log output.')
      }}</pre>
      <p class="form-help">
        Standard output and errors are grouped separately. Long output is capped.
      </p></template
    >

    <template v-else-if="view !== 'App settings'">
      <section class="service-actions">
        <h2 class="rule-heading">Actions</h2>
        <article
          v-for="item in actions.filter(
            (a) => view === 'Actions' || ['restart', 'update'].includes(a.id),
          )"
          :key="item.id"
          class="service-action-row"
        >
          <component :is="item.icon" :size="25" />
          <div>
            <h3>
              {{ item.label }}{{ ['start', 'stop', 'restart'].includes(item.id) ? ' service' : '' }}
            </h3>
            <p>{{ item.description }}</p>
          </div>
          <button class="button" :disabled="busy || !writable" @click="choose(item)">
            {{ item.label }}
          </button>
        </article>
      </section>

      <section v-if="selectedAction" class="action-review">
        <h3>{{ selectedAction.label }}</h3>
        <p>{{ selectedAction.description }}</p>
        <pre v-if="selectedAction.command" class="service-command">{{
          selectedAction.command.map((arg) => JSON.stringify(arg)).join(' ')
        }}</pre>
        <div class="project-utilities">
          <button class="button primary" :disabled="busy || !writable" @click="execute">
            {{
              pending
                ? 'Running…'
                : selectedAction.confirm
                  ? `Confirm ${selectedAction.label.toLowerCase()}`
                  : `Run ${selectedAction.label.toLowerCase()}`
            }}</button
          ><button class="button" :disabled="busy" @click="selectedAction = null">Cancel</button>
        </div>
      </section>
      <pre v-if="output" class="service-output" tabindex="0" aria-label="Action output">{{
        output
      }}</pre>
    </template>

    <AppIntegration
      v-if="view === 'App settings'"
      :key="projectId + service.app"
      :project-id="projectId"
      :service="service.app"
      :writable="writable"
    />
    <section v-if="view === 'Overview'" class="service-events">
      <h2 class="rule-heading">Recent project events</h2>
      <p v-if="!activities.length" class="form-help">No project activity yet.</p>
      <div v-for="event in activities.slice(0, 2)" :key="event.id">
        <i :class="{ online: event.status === 'succeeded' }"></i
        ><time>{{ new Date(event.started_at * 1000).toLocaleString() }}</time
        ><span>{{ event.message || event.action }}</span>
      </div>
    </section>
  </section>
</template>
