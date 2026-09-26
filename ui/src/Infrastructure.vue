<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref } from 'vue';
import { Server as ServerIcon, Network, RefreshCw, Plus, ShieldCheck } from 'lucide-vue-next';
type Server = {
  id: string;
  name: string;
  provider: string;
  endpoint: string;
  group: string;
  read_only: boolean;
  app_host: string;
};
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
  projects: { id: string; name: string; server_id: string }[];
}>();
const emit = defineEmits<{ changed: [] }>();
const servers = ref<Server[]>([]),
  selected = ref('local'),
  busy = ref(false),
  error = ref(''),
  adding = ref(false);
const editing = ref('');
function editConnection() {
  if (!current.value) return;
  Object.assign(form, current.value);
  editing.value = current.value.id;
  adding.value = true;
}
function newConnection() {
  editing.value = '';
  Object.assign(form, {
    id: '',
    name: '',
    provider: 'docker_ssh',
    endpoint: '',
    group: '',
    read_only: true,
    app_host: '',
  });
  adding.value = true;
}
const inventory = ref<any>(null),
  plan = ref<any>(null);
const form = reactive({
  id: '',
  name: '',
  provider: 'docker_ssh',
  endpoint: '',
  group: '',
  read_only: true,
  app_host: '',
});
const jobs = ref<any[]>([]),
  acknowledged = ref(false),
  guestPlan = ref<any>(null),
  guestTask = ref<any>(null),
  guestStatus = ref<any>(null);
const resourceQuery = ref(''),
  resourceNode = ref(''),
  resourceKind = ref('all'),
  visibleCount = ref(20);
const clusterNodes = computed(() =>
  (inventory.value?.resources || []).filter((r: any) => r.type === 'node'),
);
const filteredResources = computed(() =>
  (inventory.value?.resources || []).filter(
    (r: any) =>
      (!resourceNode.value || r.node === resourceNode.value) &&
      (resourceKind.value === 'all' ||
        (resourceKind.value === 'guests' && ['qemu', 'lxc'].includes(r.type)) ||
        resourceKind.value === r.type) &&
      `${r.name || ''} ${r.Names || ''} ${r.id || ''} ${r.vmid || ''}`
        .toLowerCase()
        .includes(resourceQuery.value.toLowerCase()),
  ),
);
const guest = reactive({ source_node: '', target_node: '', kind: 'qemu', vmid: 0 });
let poll: ReturnType<typeof setInterval> | undefined;
let polling = false;
async function pollJobs() {
  if (polling) return;
  polling = true;
  try {
    jobs.value = await props.api('/moves');
    if (guestTask.value && guestStatus.value?.status !== 'stopped') {
      const task = guestTask.value;
      try {
        const status = await props.api(`/servers/${task.server_id}/guest-task`, 'POST', task);
        if (guestTask.value?.task === task.task) guestStatus.value = status;
      } catch {
        if (guestTask.value?.task === task.task) guestStatus.value = { status: 'unknown' };
      }
    }
  } catch {
  } finally {
    polling = false;
  }
}
async function startMove() {
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/projects/${plan.value.project_id}/move`, 'POST', {
      server_id: plan.value.destination_id,
    });
    plan.value = null;
    await pollJobs();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function recover(job: any) {
  if (
    !confirm(
      'Stop the destination and restore the source services? Destination data will be retained.',
    )
  )
    return;
  busy.value = true;
  try {
    await props.api(`/moves/${job.id}/recover`, 'POST');
    await pollJobs();
    emit('changed');
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
function chooseGuest(r: any) {
  guest.source_node = r.node;
  guest.kind = r.type;
  guest.vmid = r.vmid;
  guest.target_node = '';
  guestPlan.value = null;
  guestStatus.value = null;
  nextTick(() =>
    document.querySelector('.guest-move')?.scrollIntoView({ block: 'center', behavior: 'smooth' }),
  );
}
async function reviewGuest() {
  busy.value = true;
  guestPlan.value = null;
  error.value = '';
  try {
    guestPlan.value = await props.api(`/servers/${selected.value}/guest-move-plan`, 'POST', guest);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function moveGuest() {
  busy.value = true;
  error.value = '';
  try {
    guestTask.value = await props.api(`/servers/${selected.value}/guest-move`, 'POST', guest);
    guestPlan.value = null;
    guestStatus.value = null;
    await pollJobs();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const move = reactive({ project_id: '', server_id: '' });
const current = computed(() => servers.value.find((s) => s.id === selected.value));
const groups = computed(() => [...new Set(servers.value.map((s) => s.group || 'Ungrouped'))]);
const provider = (kind: string) =>
  ({
    local: 'Docker on this computer',
    docker_ssh: 'Docker over SSH',
    docker_context: 'Docker context',
    proxmox_ssh: 'Proxmox over SSH',
  })[kind] || kind;
async function load() {
  servers.value = await props.api('/servers');
}
async function inspect(id: string) {
  selected.value = id;
  guest.vmid = 0;
  guestPlan.value = null;
  inventory.value = null;
  resourceQuery.value = '';
  resourceNode.value = '';
  visibleCount.value = 20;
  error.value = '';
  busy.value = true;
  try {
    inventory.value = await props.api(`/servers/${id}/inventory`);
    resourceKind.value = inventory.value.provider === 'proxmox_ssh' ? 'guests' : 'all';
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function save() {
  busy.value = true;
  error.value = '';
  try {
    const s = await props.api(
      editing.value ? `/servers/${editing.value}` : '/servers',
      editing.value ? 'PUT' : 'POST',
      form,
    );
    await load();
    adding.value = false;
    emit('changed');
    await inspect(s.id);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function review() {
  busy.value = true;
  error.value = '';
  plan.value = null;
  acknowledged.value = false;
  try {
    plan.value = await props.api(`/projects/${move.project_id}/relocation`, 'POST', {
      server_id: move.server_id,
    });
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function assign() {
  busy.value = true;
  error.value = '';
  try {
    await props.api(`/projects/${plan.value.project_id}/placement`, 'PUT', {
      server_id: plan.value.destination_id,
    });
    plan.value = null;
    emit('changed');
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
onUnmounted(() => {
  if (poll) clearInterval(poll);
});
onMounted(async () => {
  poll = setInterval(pollJobs, 5000);
  try {
    await load();
    const previous = await props.api('/guest-moves');
    guestTask.value = previous[0] || null;
    await pollJobs();
    await inspect('local');
  } catch (e) {
    error.value = String(e);
  }
});
</script>

<template>
  <div class="infrastructure">
    <div class="section-heading">
      <h2><Network :size="20" />Servers and clusters</h2>
      <button class="button primary" @click="newConnection">
        <Plus :size="17" />Connect server
      </button>
    </div>
    <p class="form-help">
      Group your hosts, inspect Docker clusters, and browse Proxmox nodes, virtual machines and
      containers.
    </p>
    <form v-if="adding" class="panel connection-form" @submit.prevent="save">
      <h2>{{ editing ? 'Edit connection' : 'Connect a server' }}</h2>
      <label
        >Name<input v-model="form.name" required maxlength="64" placeholder="Production host"
      /></label>
      <label
        >Connection<select v-model="form.provider" :disabled="!!editing">
          <option value="docker_ssh">Docker over SSH</option>
          <option value="docker_context">Existing Docker context</option>
          <option value="proxmox_ssh">Proxmox over SSH</option>
        </select></label
      >
      <label
        >{{ form.provider === 'docker_context' ? 'Docker context name' : 'SSH host alias'
        }}<input
          v-model="form.endpoint"
          :disabled="!!editing"
          required
          pattern="[a-zA-Z0-9_.-]+"
          maxlength="128"
          placeholder="my-server"
      /></label>
      <p class="form-help">
        Uses your existing OpenSSH configuration and keys. Verify the host key with SSH first.
        Proxmox discovery needs permission to run pvesh on a cluster node. Connect Docker inside a
        VM or LXC using that guest’s SSH alias.
      </p>
      <label
        >Group<input
          v-model="form.group"
          maxlength="64"
          placeholder="Production, homelab, or a location"
      /></label>
      <label v-if="form.provider !== 'proxmox_ssh'"
        >App hostname (optional)<input v-model="form.app_host" placeholder="apps.example.net"
      /></label>
      <label class="check-line"
        ><input type="checkbox" v-model="form.read_only" />Read-only connection</label
      >
      <p class="form-help">
        Read-only connections allow discovery, stats and logs. Deployment and service actions are
        blocked by the server.
      </p>
      <div class="project-utilities">
        <button class="button primary" :disabled="busy">Save connection</button
        ><button type="button" class="button" @click="adding = false">Cancel</button>
      </div>
    </form>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <div class="infrastructure-grid">
      <section class="server-list">
        <div v-for="group in groups" :key="group">
          <h3>{{ group }}</h3>
          <button
            v-for="s in servers.filter((s) => (s.group || 'Ungrouped') === group)"
            :key="s.id"
            class="server-choice"
            :class="{ selected: selected === s.id }"
            :disabled="busy"
            @click="inspect(s.id)"
          >
            <ServerIcon :size="22" /><span
              ><strong>{{ s.name }}</strong
              ><small>{{ provider(s.provider) }}</small
              ><small v-if="s.read_only">Read-only</small></span
            >
          </button>
        </div>
      </section>
      <section class="panel inventory-panel" :aria-busy="busy">
        <div class="section-heading">
          <div>
            <h2>{{ current?.name }}</h2>
            <p class="form-help">{{ current?.endpoint || 'Local Docker engine' }}</p>
          </div>
          <button
            class="icon-button"
            aria-label="Refresh server inventory"
            :disabled="busy"
            @click="inspect(selected)"
          >
            <RefreshCw :size="19" :class="{ spin: busy }" />
          </button>
        </div>
        <p v-if="busy" class="form-help">Reading server inventory…</p>
        <button
          v-if="!inventory && current?.id !== 'local'"
          class="text-button"
          @click="editConnection"
        >
          Edit connection</button
        ><template v-if="inventory">
          <button v-if="current?.id !== 'local'" class="text-button" @click="editConnection">
            Edit connection
          </button>
          <p class="form-help" v-if="inventory.version">
            Docker {{ inventory.version }} · {{ inventory.architecture }} · {{ inventory.os }}
          </p>
          <p class="form-help" v-if="inventory.swarm?.LocalNodeState === 'active'">
            Swarm {{ inventory.swarm.ControlAvailable ? 'manager' : 'worker' }}.
            {{ inventory.nodes?.length || 0 }} visible cluster nodes. Projects deploy to this Docker
            engine; they are not Swarm services.
          </p>
          <div v-if="inventory.nodes?.length">
            <h3>Cluster nodes</h3>
            <div class="resource-row" v-for="node in inventory.nodes" :key="node.ID">
              <strong>{{ node.Hostname }}</strong
              ><span>{{ node.Status }} · {{ node.Availability }}</span
              ><small>{{ node.ManagerStatus }}</small>
            </div>
          </div>
          <div v-if="inventory.services?.length">
            <h3>Swarm services</h3>
            <div class="resource-row" v-for="s in inventory.services" :key="s.ID">
              <strong>{{ s.Name }}</strong
              ><span>{{ s.Replicas }}</span
              ><small>{{ s.Image }}</small>
            </div>
          </div>
          <div v-if="clusterNodes.length" class="cluster-nodes">
            <button
              v-for="n in clusterNodes"
              :key="n.id"
              class="node-card"
              :class="{ selected: resourceNode === n.node }"
              @click="resourceNode = resourceNode === n.node ? '' : n.node"
            >
              <strong>{{ n.node }}</strong
              ><span><i :class="{ online: n.status === 'online' }"></i>{{ n.status }}</span
              ><small v-if="n.maxmem">Memory {{ Math.round((n.mem / n.maxmem) * 100) }}%</small
              ><small v-if="n.cpu !== null">CPU {{ (n.cpu * 100).toFixed(1) }}%</small>
            </button>
          </div>
          <h3>{{ inventory.provider === 'proxmox_ssh' ? 'Cluster resources' : 'Containers' }}</h3>
          <div class="resource-filters">
            <label>Search resources<input v-model="resourceQuery" placeholder="Name or ID" /></label
            ><label v-if="inventory.provider === 'proxmox_ssh'"
              >Resource type<select v-model="resourceKind">
                <option value="guests">VMs and LXCs</option>
                <option value="qemu">VMs</option>
                <option value="lxc">LXCs</option>
                <option value="storage">Storage</option>
                <option value="network">Networks</option>
                <option value="all">All resources</option>
              </select></label
            >
          </div>
          <p class="form-help">
            {{ filteredResources.length }} resources{{ resourceNode ? ' on ' + resourceNode : '' }}
          </p>
          <p v-if="!inventory.resources?.length" class="form-help">No resources reported.</p>
          <div
            class="resource-row"
            v-for="r in filteredResources.slice(0, visibleCount)"
            :key="r.id || r.ID"
          >
            <strong>{{ r.name || r.Names || r.id || r.node }}</strong
            ><span
              >{{
                r.template
                  ? 'Template'
                  : r.type === 'qemu'
                    ? 'VM'
                    : r.type === 'lxc'
                      ? 'LXC'
                      : r.type || 'Docker'
              }}<template v-if="r.vmid"> · {{ r.vmid }}</template></span
            ><span>{{ r.status || r.State }}</span
            ><small>{{ r.node || r.Image }}</small
            ><button
              v-if="['qemu', 'lxc'].includes(r.type)"
              class="button small"
              @click="chooseGuest(r)"
            >
              Move
            </button>
          </div>
          <button
            v-if="filteredResources.length > visibleCount"
            class="text-button"
            @click="visibleCount += 20"
          >
            Show 20 more
          </button>
          <section v-if="guest.vmid" class="move-review guest-move">
            <h3>Move {{ guest.kind === 'qemu' ? 'VM' : 'LXC' }} {{ guest.vmid }}</h3>
            <form @submit.prevent="reviewGuest">
              <label
                >Destination node<select
                  v-model="guest.target_node"
                  required
                  @change="guestPlan = null"
                >
                  <option disabled value="">Choose node</option>
                  <option
                    v-for="r in inventory.resources.filter(
                      (r: any) => r.type === 'node' && r.node !== guest.source_node,
                    )"
                    :key="r.node"
                    :value="r.node"
                  >
                    {{ r.node }}
                  </option>
                </select></label
              ><button class="button" :disabled="busy || !guest.target_node">
                Check guest move
              </button>
            </form>
            <div v-if="guestPlan">
              <p class="form-help">{{ guestPlan.note }}</p>
              <div class="resource-row" v-for="disk in guestPlan.disks" :key="disk.device">
                <strong>{{ disk.device }}</strong
                ><span>{{ disk.volume }}</span>
              </div>
              <div v-if="guestPlan.blockers.length" class="monitor-error">
                <ul>
                  <li v-for="b in guestPlan.blockers" :key="b">{{ b }}</li>
                </ul>
              </div>
              <button
                v-if="guestPlan.can_migrate"
                class="button primary"
                :disabled="busy"
                @click="moveGuest"
              >
                Migrate stopped guest
              </button>
            </div>
          </section>
          <div v-if="guestTask" class="move-review">
            <h3>Proxmox migration</h3>
            <p class="form-help">
              {{
                guestStatus?.status === 'stopped'
                  ? guestStatus.exitstatus === 'OK'
                    ? 'Migration completed'
                    : `Migration failed: ${guestStatus.exitstatus}`
                  : guestStatus?.status === 'unknown'
                    ? 'Task status unavailable. Check the connection before retrying.'
                    : 'Migration task running'
              }}
            </p>
            <code>{{ guestTask.task }}</code>
            <p class="form-help">
              Guest {{ guestTask.vmid }} · {{ guestTask.node }} → {{ guestTask.destination }}
            </p>
          </div>
          <p class="form-help">
            Inventory is read-only. Existing workloads are not adopted or changed.
          </p>
        </template>
      </section>
    </div>
    <section class="panel relocation-panel">
      <h2>Move a project</h2>
      <p>Review its destination, volumes and ports before changing where it runs.</p>
      <form @submit.prevent="review">
        <label
          >Project<select v-model="move.project_id" required @change="plan = null">
            <option disabled value="">Choose project</option>
            <option v-for="p in projects" :key="p.id" :value="p.id">
              {{ p.name }} ·
              {{ servers.find((s) => s.id === p.server_id)?.name || 'This computer' }}
            </option>
          </select></label
        ><label
          >Destination<select v-model="move.server_id" required @change="plan = null">
            <option disabled value="">Choose server</option>
            <option
              v-for="s in servers.filter(
                (s) =>
                  s.provider !== 'proxmox_ssh' &&
                  s.id !== projects.find((p) => p.id === move.project_id)?.server_id,
              )"
              :key="s.id"
              :value="s.id"
            >
              {{ s.name }}{{ s.read_only ? ' (read-only)' : '' }}
            </option>
          </select></label
        ><button class="button" :disabled="busy || !move.project_id || !move.server_id">
          <ShieldCheck :size="17" />Check move
        </button>
      </form>
      <div v-if="plan" class="move-review">
        <h3>{{ plan.source }} → {{ plan.destination }}</h3>
        <p class="form-help">{{ plan.note }}</p>
        <h3>Volumes</h3>
        <div v-for="v in plan.volumes" :key="v.name" class="resource-row">
          <code>{{ v.name }}</code
          ><span>{{ v.path }}</span>
        </div>
        <p class="form-help">Host ports: {{ plan.ports.join(', ') }}</p>
        <ol>
          <li v-for="step in plan.steps" :key="step">{{ step }}</li>
        </ol>
        <div v-if="plan.blockers.length" class="monitor-error">
          <strong>Resolve before moving</strong>
          <ul>
            <li v-for="b in plan.blockers" :key="b">{{ b }}</li>
          </ul>
        </div>
        <label v-if="plan.can_migrate" class="check-line"
          ><input type="checkbox" v-model="acknowledged" />I understand that services will stop
          during transfer. Each archive is limited to 20 GiB and uses local temporary disk
          space.</label
        ><button
          v-if="plan.can_migrate"
          class="button primary"
          :disabled="busy || !acknowledged"
          @click="startMove"
        >
          Stop source and move data</button
        ><button v-if="plan.can_assign" class="button primary" :disabled="busy" @click="assign">
          Assign undeployed project
        </button>
      </div>
    </section>
    <section v-if="jobs.length" class="panel">
      <h2>Migration activity</h2>
      <article v-for="job in jobs" :key="job.id" class="resource-row">
        <div>
          <strong>{{
            projects.find((p) => p.id === job.project_id)?.name || job.project_id
          }}</strong>
          <p class="form-help">
            {{ job.stage.replaceAll('_', ' ') }} · {{ job.status.replaceAll('_', ' ') }}
          </p>
          <p class="form-help">{{ job.message }}</p>
        </div>
        <button
          v-if="!['succeeded', 'recovered'].includes(job.status)"
          class="button small"
          :disabled="busy"
          @click="recover(job)"
        >
          Recover source
        </button>
      </article>
    </section>
  </div>
</template>
