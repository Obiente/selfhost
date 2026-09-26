<script setup lang="ts">
import { sessionFetch } from './session';
import ExistingApps from './ExistingApps.vue';
import SignIn from './SignIn.vue';
import UpdateCenter from './UpdateCenter.vue';
import DeploymentPicker from './DeploymentPicker.vue';
import RemovalDialog from './RemovalDialog.vue';
import ArchivedProjects from './ArchivedProjects.vue';
import BaseDialog from './components/BaseDialog.vue';
import { usePolling } from './composables/usePolling';

import AccountMenu, { type Account } from './AccountMenu.vue';
import ServiceMonitor from './ServiceMonitor.vue';
import AppOnboarding from './AppOnboarding.vue';
import AppVersionPicker, { type VersionSelection } from './AppVersionPicker.vue';
import AppVersionManager from './AppVersionManager.vue';
import TaskManager from './TaskManager.vue';
import Infrastructure from './Infrastructure.vue';
import SetupEditor from './SetupEditor.vue';
import Databases from './Databases.vue';
import AccessSettings from './AccessSettings.vue';
import DashboardHosting from './DashboardHosting.vue';
import Networking from './Networking.vue';
import StackInstall from './StackInstall.vue';
import AppIcon from './components/AppIcon.vue';

import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';

import {
  ArrowDownToLine,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  Bell,
  Box,
  Check,
  CheckCheck,
  ChevronRight,
  CircleHelp,
  Clock3,
  Database,
  Folder,
  FolderPlus,
  HardDrive,
  LayoutGrid,
  LoaderCircle,
  Menu,
  MoreHorizontal,
  Network,
  Play,
  Plus,
  RefreshCw,
  Search,
  Server,
  Settings2,
  ShieldCheck,
  Square,
  Terminal,
  X,
} from 'lucide-vue-next';

type AppInfo = {
  deployment_only?: boolean;
  id: string;
  name: string;
  description: string;
  category: string;
  image: string;
  port: number;
  docs: string;
  icon: string;
  data_path?: string;
  dashboard?: { auth_header: string };
  actions?: {
    id: string;
    label: string;
    description: string;
    command: string[];
    confirm: boolean;
  }[];
};

type Service = { app: string; image: string; port: number; definition: AppInfo };

type Project = {
  custom?: boolean;
  server_id: string;
  id: string;
  name: string;
  services: Service[];
  access: string;
  created_at: number;
};

type Activity = {
  id: string;
  project_id: string;
  project_name: string;
  action: string;
  status: string;
  message: string;
  started_at: number;
  read: boolean;
};

type Snapshot = { id: string; project_id: string; project_name: string; created_at: number };

type Schedule = {
  project_id: string;
  action: string;
  interval_hours: number;
  enabled: boolean;
  next_run: number;
};

type Container = { Names: string; State: string; Status: string; Labels: string };

const data = reactive({
  projects: [] as Project[],
  activities: [] as Activity[],
  snapshots: [] as Snapshot[],
  schedules: [] as Schedule[],
});

const catalog = ref<AppInfo[]>([]);
const servers = ref<
  { id: string; name: string; provider: string; read_only: boolean; app_host: string }[]
>([]);
const activeServer = computed(() =>
  servers.value.find((s) => s.id === (project.value?.server_id || 'local')),
);
const writable = computed(() => engine.available && !activeServer.value?.read_only);
function appUrl(p: Project, port: number) {
  if (!port) return '';
  const s = servers.value.find((s) => s.id === p.server_id);
  if (p.server_id === 'local') return `http://localhost:${port}`;
  return p.access === 'lan' && s?.app_host ? `http://${s.app_host}:${port}` : '';
}

const engine = reactive({
  available: false,
  checked: false,
  message: '',
  containers: [] as Container[],
});

const page = ref('Overview');

const nav = [
  { name: 'Overview', icon: LayoutGrid },
  { name: 'Projects', icon: Folder },
  { name: 'Existing apps', icon: Network },
  { name: 'Infrastructure', icon: Server },
  { name: 'Networking', icon: Network },
  { name: 'Access', icon: ShieldCheck },
  { name: 'App catalog', icon: Box },
  { name: 'Databases', icon: Database },
  { name: 'Schedules', icon: Clock3 },
  { name: 'Tasks & triggers', icon: RefreshCw },
  { name: 'Snapshots', icon: Database },
  { name: 'Activity', icon: Bell },
];

const selectedId = ref<string | null>(null);

const project = computed(() => data.projects.find((p) => p.id === selectedId.value));

const projectTab = ref('Services');

const activeService = ref('');

const search = ref('');

const showCreate = ref(false);
const deploymentApp = ref(''),
  deploymentOpen = ref(false);
const deploymentMethods = ref<{ app: string; id: string; name: string }[]>([]);
const removalTarget = ref<{ id: string; name: string; service?: string; readOnly: boolean } | null>(
  null,
);
const showArchives = ref(false);
function removeTarget(p: Project, service?: string) {
  removalTarget.value = {
    id: p.id,
    name: p.name,
    service,
    readOnly: !!servers.value.find((s) => s.id === p.server_id)?.read_only,
  };
}
async function removed() {
  removalTarget.value = null;
  selectedId.value = null;
  await load();
  page.value = 'Projects';
  showArchives.value = true;
}
function chooseDeployment(app: string) {
  showCreate.value = false;
  deploymentApp.value = app;
  deploymentOpen.value = true;
}
async function deploymentCreated(id: string) {
  await load();
  const created = data.projects.find((p) => p.id === id);
  if (created) openProject(created);
}

const showCustom = ref(false);
const editSetup = ref(false);

const mobileNav = ref(false);

const loading = ref(true);

const error = ref('');

const toast = ref('');

const creating = ref(false);

const working = reactive<Record<string, string>>({});

const form = reactive({ server_id: 'local', name: '', apps: [] as string[] });
const createVersions = reactive<Record<string, VersionSelection>>({});
const createVersionValid = reactive<Record<string, boolean>>({});
const allVersionsValid = computed(() => form.apps.every((app) => createVersionValid[app]));

const editor = reactive({
  name: '',
  access: 'local',
  services: [] as { app: string; image: string; port: number }[],
});

const scheduleForm = reactive({
  project_id: '',
  action: 'snapshot',
  interval_hours: 24,
  enabled: true,
  next_run: 0,
});

const key = ref('');

const syncTarget = ref('');

const showSync = ref(false);

const plan = ref('');

const showPlan = ref(false);

const engineDetails = ref(false);

let currentToken = new URLSearchParams(location.hash.slice(1)).get('token');
const oidcProof = new URLSearchParams(location.hash.slice(1)).get('client');
if (oidcProof && /^[a-f0-9]{64}$/.test(oidcProof))
  sessionStorage.setItem('selfhost-oidc-client', oidcProof);
if (currentToken || oidcProof) history.replaceState(null, '', location.pathname + location.search);
// Migrate away from the old process-wide bearer. It is never sent to the API.
sessionStorage.removeItem('selfhost-token');
const authenticated = ref(false);
const account = ref<Account | null>(null);
const accountBusy = ref(false),
  accountError = ref('');
const loginProviders = ref<{ id: string; name: string; login_url: string }[]>([]);
async function refreshLoginProviders() {
  try {
    const response = await fetch('/auth/info');
    if (!response.ok) throw new Error('Unable to load sign-in options. Try again.');
    loginProviders.value = (await response.json()).providers;
  } catch (e) {
    accountError.value = (e as Error).message;
  }
}
async function loadLogin() {
  loading.value = true;
  accountError.value = '';
  if (currentToken) {
    const token = currentToken;
    currentToken = null;
    try {
      const response = await fetch('/auth/local', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token }),
      });
      const value = await response.json();
      if (!response.ok) throw new Error(value.error || 'Unable to sign in.');
      sessionStorage.setItem('selfhost-client', value.client_key);
    } catch (e) {
      accountError.value = (e as Error).message;
    }
  }
  await refreshLoginProviders();
  await refreshAccount();
  loading.value = false;
}
async function refreshAccount() {
  try {
    account.value = await api('/account');
  } catch (e) {
    if (authenticated.value) accountError.value = (e as Error).message;
  }
}
async function signOut(all = false) {
  accountBusy.value = true;
  accountError.value = '';
  try {
    await api('/login/logout', 'POST', { all });
    sessionStorage.removeItem('selfhost-client');
    sessionStorage.removeItem('selfhost-oidc-client');
    location.reload();
  } catch (e) {
    accountError.value = (e as Error).message;
  } finally {
    accountBusy.value = false;
  }
}
function recoveryHashChanged() {
  if (['token', 'client'].some((key) => new URLSearchParams(location.hash.slice(1)).has(key)))
    location.reload();
}
function sessionEnded() {
  const active = authenticated.value;
  authenticated.value = false;
  account.value = null;
  if (active) location.reload();
}
let lastInteraction = Date.now();
function interacted() {
  lastInteraction = Date.now();
}
let heartbeat: ReturnType<typeof setInterval>;
async function sessionHeartbeat() {
  if (!authenticated.value) return;
  try {
    if (document.visibilityState === 'visible' && Date.now() - lastInteraction < 60_000)
      await api('/account/activity', 'POST', {});
    await refreshAccount();
  } catch {
    /* api handles expired sessions */
  }
}

const notifications = computed(
  () => data.activities.filter((a) => !a.read && a.status !== 'running').length,
);

const activities = computed(() => [...data.activities].reverse());

const running = computed(() => engine.containers.filter((c) => c.State === 'running').length);

const totalServices = computed(() => data.projects.reduce((sum, p) => sum + p.services.length, 0));

const filteredProjects = computed(() =>
  data.projects.filter((p) => p.name.toLowerCase().includes(search.value.toLowerCase())),
);

const filteredCatalog = computed(() =>
  catalog.value.filter((a) =>
    `${a.name} ${a.category} ${a.description}`.toLowerCase().includes(search.value.toLowerCase()),
  ),
);

const nextSchedule = computed(
  () => data.schedules.filter((s) => s.enabled).sort((a, b) => a.next_run - b.next_run)[0],
);

const actionNames: Record<string, string> = {
  start: 'Start services',
  stop: 'Stop services',
  restart: 'Restart services',
  refresh: 'Refresh images',
  snapshot: 'Save configuration',
};

function date(value: number) {
  return new Date(value * 1000).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function go(name: string) {
  showCustom.value = false;
  editSetup.value = false;
  page.value = name;
  selectedId.value = null;
  search.value = '';
  mobileNav.value = false;
}

function openProject(p: Project) {
  showCustom.value = false;
  editSetup.value = !!p.custom;
  activeService.value = p.services[0]?.app || '';
  selectedId.value = p.id;
  page.value = 'Projects';
  projectTab.value = 'Services';
  resetEditor(p);
}

function resetEditor(p: Project) {
  editor.name = p.name;
  editor.access = p.access;
  editor.services = p.services.map((s) => ({ app: s.app, image: s.image, port: s.port }));
}

function startCreate(app?: string) {
  if (
    app &&
    (catalog.value.find((item) => item.id === app)?.deployment_only ||
      deploymentMethods.value.filter((m) => m.app === app).length > 1)
  ) {
    chooseDeployment(app);
    return;
  }
  form.server_id = 'local';
  form.name = '';
  form.apps = app ? [app] : [];
  for (const key of Object.keys(createVersions)) delete createVersions[key];
  for (const key of Object.keys(createVersionValid)) delete createVersionValid[key];
  showCreate.value = true;
}

async function exportSetup(id: string) {
  const response = await sessionFetch(`/api/projects/${id}/export`);
  if (!response.ok) throw new Error((await response.json()).error);
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = url;
  link.download = `${id}-setup.tar`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
async function exportDatabaseBackup(id: string, backup: string) {
  const response = await sessionFetch(`/api/projects/${id}/database/backups/${backup}/download`);
  if (!response.ok) throw new Error((await response.json()).error);
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = url;
  link.download = `${id}-${backup}.dump`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
async function setupSaved(p: Project) {
  await load();
  if (!selectedId.value) {
    openProject(p);
    projectTab.value = 'Configuration';
    editSetup.value = true;
  }
}

async function api(path: string, method = 'GET', body?: unknown) {
  const response = await sessionFetch(`/api${path}`, {
    method,
    headers: { ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const value = await response.json();

  if (!response.ok) throw new Error(value.error || 'The request could not be completed.');
  authenticated.value = true;

  return value;
}

let firstLoad = true;

function serviceStatus(app: string) {
  const found = engine.containers.find(
    (c) =>
      c.Labels.split(',').includes(`com.docker.compose.project=selfhost-${selectedId.value}`) &&
      c.Labels.split(',').includes(`com.docker.compose.service=${app}`),
  );
  return !engine.checked
    ? 'Checking'
    : !engine.available
      ? 'Offline'
      : found?.State === 'running'
        ? 'Running'
        : found
          ? 'Stopped'
          : 'Not started';
}

async function load() {
  try {
    const response = await api('/state');
    Object.assign(data, response.data);
    catalog.value = response.catalog;
    servers.value = [
      {
        id: 'local',
        name: 'This computer',
        provider: 'local',
        read_only: false,
        app_host: 'localhost',
      },
      ...(response.data.servers || []),
    ];
    if (firstLoad) {
      firstLoad = false;
      if (data.projects[0]) openProject(data.projects[0]);
    }
    if (!scheduleForm.project_id && data.projects[0]) scheduleForm.project_id = data.projects[0].id;
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    loading.value = false;
  }
}

watch(
  () => project.value?.server_id || 'local',
  () => {
    engine.available = false;
    engine.checked = false;
    engine.containers = [];
    checkEngine();
  },
);
let checkingEngine = false;

async function checkEngine() {
  if (checkingEngine) return;

  checkingEngine = true;
  const requestedServer = project.value?.server_id || 'local';

  try {
    const result = await api(`/servers/${requestedServer}/docker`);
    if (requestedServer === (project.value?.server_id || 'local'))
      Object.assign(engine, result, { checked: true });
  } catch {
    if (requestedServer === (project.value?.server_id || 'local')) {
      engine.available = false;
      engine.checked = true;
    }
  } finally {
    checkingEngine = false;
    if (requestedServer !== (project.value?.server_id || 'local')) checkEngine();
  }
}

function notify(message: string) {
  toast.value = message;
  setTimeout(() => {
    if (toast.value === message) toast.value = '';
  }, 5000);
}

async function createProject() {
  creating.value = true;

  try {
    const p = await api('/projects', 'POST', {
      ...form,
      versions: Object.fromEntries(form.apps.map((app) => [app, createVersions[app]])),
    });
    await load();
    showCreate.value = false;
    openProject(p);
    notify('Project created. Review your apps, then start them when you’re ready.');
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    creating.value = false;
  }
}

async function action(p: Project, name: string) {
  if (working[p.id]) return;

  if (
    name === 'stop' &&
    !confirm(`Stop the services in ${p.name}? Your application data will be kept.`)
  )
    return;

  working[p.id] = name;

  try {
    await api(`/projects/${p.id}/actions`, 'POST', { action: name });
    notify(`${actionNames[name]} completed for ${p.name}.`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    delete working[p.id];
    await load();
    await checkEngine();
  }
}

async function saveConfig() {
  if (!project.value) return;

  try {
    await api(`/projects/${project.value.id}`, 'PUT', editor);
    await load();
    notify('Configuration saved. Start the project to apply changes.');
  } catch (e) {
    error.value = (e as Error).message;
  }
}

async function saveSchedule(schedule: Schedule = scheduleForm) {
  try {
    await api('/schedules', 'POST', schedule);
    await load();
    notify('Schedule saved.');
  } catch (e) {
    error.value = (e as Error).message;
  }
}

async function restore(snapshot: Snapshot) {
  if (
    !confirm(
      `Restore the saved configuration for ${snapshot.project_name}? A snapshot of the current configuration will be kept. Start the project afterwards to apply it.`,
    )
  )
    return;

  try {
    await api(`/snapshots/${snapshot.id}/restore`, 'POST', {});
    await load();
    notify('Configuration restored.');
  } catch (e) {
    error.value = (e as Error).message;
  }
}

async function markRead() {
  try {
    await api('/notifications/read', 'POST', {});
    await load();
  } catch (e) {
    error.value = (e as Error).message;
  }
}

async function sync() {
  if (!project.value) return;

  try {
    const result = await api(`/projects/${project.value.id}/sync`, 'POST', {
      target: syncTarget.value,
      key: key.value,
    });
    notify(`Added ${result.added} apps to your dashboard.`);
    showSync.value = false;
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    key.value = '';
  }
}

async function viewPlan(p: Project) {
  try {
    plan.value = JSON.stringify(await api(`/projects/${p.id}/plan`), null, 2);
    showPlan.value = true;
  } catch (e) {
    error.value = (e as Error).message;
  }
}

function containers(p: Project) {
  return engine.containers.filter((c) =>
    c.Labels.split(',').includes(`com.docker.compose.project=selfhost-${p.id}`),
  );
}

function status(p: Project) {
  if (p.server_id !== (project.value?.server_id || 'local')) return 'Open to check';
  if (working[p.id]) return 'Working';

  if (!engine.checked) return 'Checking';

  if (!engine.available) return 'Docker offline';

  const found = containers(p);
  if (!found.length) return 'Ready to start';

  if (found.length === p.services.length && found.every((c) => c.State === 'running'))
    return 'Running';

  if (found.some((c) => c.State === 'running')) return 'Partly running';
  return 'Stopped';
}

usePolling(refreshLoginProviders, 15000, () => !authenticated.value && !loading.value);
usePolling(load, 4000, () => authenticated.value);
usePolling(checkEngine, 12000, () => authenticated.value);
onMounted(async () => {
  window.addEventListener('selfhost-session-ended', sessionEnded);
  window.addEventListener('hashchange', recoveryHashChanged);
  document.addEventListener('pointerdown', interacted);
  document.addEventListener('keydown', interacted);
  heartbeat = setInterval(sessionHeartbeat, 30_000);
  await loadLogin();
  if (authenticated.value) {
    load();
    checkEngine();
    try {
      deploymentMethods.value = await api('/deployments');
    } catch (e) {
      error.value = (e as Error).message;
    }
  }
});

onUnmounted(() => {
  window.removeEventListener('hashchange', recoveryHashChanged);
  window.removeEventListener('selfhost-session-ended', sessionEnded);
  clearInterval(heartbeat);
  document.removeEventListener('pointerdown', interacted);
  document.removeEventListener('keydown', interacted);
});
</script>

<template>
  <SignIn
    v-if="!authenticated"
    :providers="loginProviders"
    :loading="loading"
    :error="accountError"
    @refresh="loadLogin"
  />
  <div v-else class="shell">
    <a class="skip-link" href="#main-content">Skip to content</a>

    <aside class="rail" aria-label="Main navigation">
      <a href="#" aria-label="selfhost overview" @click.prevent="go('Overview')"
        ><img src="/brand/selfhost-monogram.svg" alt="selfhost"
      /></a>
      <nav>
        <button
          v-for="item in nav"
          :key="item.name"
          :title="item.name"
          :aria-label="item.name"
          :aria-current="page === item.name ? 'page' : undefined"
          :class="{ active: page === item.name }"
          @click="go(item.name)"
        >
          <component :is="item.icon" :size="22" />
        </button>
      </nav>
    </aside>

    <aside class="sidebar" :class="{ open: mobileNav }">
      <a href="#" class="brand" @click.prevent="go('Overview')"
        ><img src="/brand/selfhost-wordmark.svg" alt="selfhost"
      /></a>

      <template v-if="project"
        ><label class="project-picker"
          ><span class="sr-only">Select project</span
          ><select
            :value="project.id"
            @change="
              openProject(
                data.projects.find((p) => p.id === ($event.target as HTMLSelectElement).value)!,
              )
            "
          >
            <option v-for="p in data.projects" :value="p.id">{{ p.name }}</option>
          </select></label
        >
        <p class="sidebar-caption">
          {{ activeServer?.name || 'This computer' }} ·
          {{
            project.custom
              ? 'Compose networking'
              : project.access === 'local'
                ? 'Loopback'
                : 'Network'
          }}
        </p>
        <nav class="service-navigation" aria-label="Project services">
          <button
            v-for="service in project.services"
            :key="service.app"
            :class="{ selected: activeService === service.app && projectTab === 'Services' }"
            :disabled="!!working[project.id]"
            @click="
              activeService = service.app;
              projectTab = 'Services';
              mobileNav = false;
            "
          >
            <AppIcon :src="service.definition.icon" alt="" /><span
              ><strong>{{ service.definition.name }}</strong
              ><small
                ><i :class="{ online: serviceStatus(service.app) === 'Running' }"></i
                >{{ serviceStatus(service.app) }}</small
              ></span
            >
          </button>
        </nav>
        <button
          class="text-button project-settings"
          @click="
            projectTab = 'Configuration';
            resetEditor(project);
            editSetup = !!project.custom;
            mobileNav = false;
          "
        >
          <Settings2 :size="17" />Project settings
        </button></template
      >

      <template v-else
        ><h2>{{ page }}</h2>
        <p class="sidebar-caption">Your local workspace</p>
        <nav class="service-navigation">
          <button v-for="p in data.projects" :key="p.id" @click="openProject(p)">
            <Folder :size="22" /><span
              ><strong>{{ p.name }}</strong
              ><small>{{ p.services.length }} services</small></span
            >
          </button>
        </nav></template
      >

      <div class="sidebar-bottom">
        <button class="text-button" @click="go('App catalog')">
          <Plus :size="20" />Browse services</button
        ><button class="engine-summary" @click="checkEngine" aria-label="Refresh Docker connection">
          <Server :size="20" /><span
            >{{
              !engine.checked
                ? 'Connecting'
                : engine.available
                  ? 'Docker connected'
                  : 'Docker offline'
            }}<small>{{ activeServer?.name || 'This computer' }}</small></span
          ><i :class="{ online: engine.available }"></i>
        </button>
      </div>
    </aside>

    <div class="main-wrap">
      <header class="topbar">
        <div class="breadcrumbs">
          <button
            class="icon-button mobile-menu"
            aria-label="Open navigation"
            @click="mobileNav = !mobileNav"
          >
            <Menu :size="20" /></button
          ><template v-if="project"
            ><span>{{ project.name }}</span
            ><ChevronRight :size="14" /><strong>{{
              projectTab === 'Configuration'
                ? 'Project settings'
                : project.services.find((s) => s.app === activeService)?.definition.name
            }}</strong></template
          ><template v-else
            ><span>Workspace</span><ChevronRight :size="14" /><strong>{{ page }}</strong></template
          >
        </div>
        <div class="top-actions">
          <UpdateCenter :api="api" /><span class="connection"
            ><i :class="{ online: engine.available }"></i
            >{{
              !engine.checked
                ? 'Connecting'
                : engine.available
                  ? 'Docker connected'
                  : 'Docker offline'
            }}</span
          ><button
            class="icon-button notification-button"
            aria-label="View notifications"
            @click="go('Activity')"
          >
            <Bell :size="19" /><i v-if="notifications"></i></button
          ><AccountMenu
            v-if="account"
            :account="account"
            :busy="accountBusy"
            :error="accountError"
            @access="go('Access')"
            @signout="signOut"
            @refresh="refreshAccount"
          />
        </div>
      </header>

      <main id="main-content" tabindex="-1">
        <template v-if="authenticated">
          <div
            v-if="(!project || projectTab !== 'Services') && page !== 'Existing apps'"
            class="page-heading"
          >
            <div>
              <button v-if="project" class="text-button back" @click="selectedId = null">
                <ArrowLeft :size="15" />All projects
              </button>
              <h1>
                {{ project ? project.name : page === 'App catalog' ? 'Find your next app.' : page }}
              </h1>
              <p>
                {{
                  project
                    ? `${activeServer?.name || 'This computer'} · ${project.services.length} apps · ${project.custom ? 'Compose networking' : project.access === 'local' ? 'Server only (loopback)' : 'Available on your network'}`
                    : page === 'Overview'
                      ? 'A home for everything you run.'
                      : page === 'Projects'
                        ? 'Keep your services together, on your terms.'
                        : page === 'App catalog'
                          ? 'Find an app and choose how to run it.'
                          : page === 'Databases'
                            ? 'Choose where your application data lives.'
                            : page === 'Tasks & triggers'
                              ? 'Keep service links up to date and run the maintenance actions you choose.'
                              : page === 'Schedules'
                                ? 'Let the routine take care of itself.'
                                : page === 'Infrastructure'
                                  ? 'Your servers, clusters and workload placement.'
                                  : page === 'Networking'
                                    ? 'Connect your proxies, private networks and services.'
                                    : page === 'Access'
                                      ? 'Choose how you sign in to Selfhost.'
                                      : page === 'Existing apps'
                                        ? 'Connect the services you already run.'
                                        : page === 'Snapshots'
                                          ? 'Keep a copy of your project configuration.'
                                          : 'What happened across your projects.'
                }}
              </p>
            </div>
            <button
              v-if="['Overview', 'Projects', 'App catalog'].includes(page) && !project"
              class="button primary"
              @click="startCreate()"
            >
              <Plus :size="18" />New project</button
            ><button
              v-if="project"
              class="button primary"
              :disabled="!!working[project.id] || !writable"
              @click="action(project, 'start')"
            >
              <LoaderCircle v-if="working[project.id]" class="spin" :size="17" /><Play
                v-else
                :size="16"
              />{{ working[project.id] ? 'Working…' : 'Start project' }}</button
            ><button v-if="page === 'Activity'" class="button" @click="markRead">
              <CheckCheck :size="17" />Mark all read
            </button>
          </div>

          <div v-if="engine.checked && !engine.available" class="engine-notice">
            <div class="notice-icon"><Server :size="19" /></div>
            <div>
              <strong>Docker is not running</strong>
              <p>
                You can set up projects now. Connect Docker when you’re ready to start your apps.
              </p>
              <pre v-if="engineDetails">{{ engine.message }}</pre>
            </div>
            <button class="text-button" @click="engineDetails = !engineDetails">
              {{ engineDetails ? 'Hide details' : 'Details' }}</button
            ><button class="icon-button" aria-label="Retry Docker connection" @click="checkEngine">
              <RefreshCw :size="16" />
            </button>
          </div>

          <div v-if="page === 'Overview'" class="stats">
            <div class="stat">
              <span
                ><span class="stat-icon green"><Network :size="18" /></span>Locally running
                services</span
              >
              <div>
                <strong>{{ running }}</strong
                ><small>/ {{ totalServices }} configured</small>
              </div>
            </div>
            <div class="stat">
              <span
                ><span class="stat-icon sand"><Folder :size="18" /></span>Your projects</span
              >
              <div>
                <strong>{{ data.projects.length }}</strong
                ><small>{{ totalServices }} apps in total</small>
              </div>
            </div>
            <div class="stat">
              <span
                ><span class="stat-icon blue"><Clock3 :size="18" /></span>Next scheduled task</span
              >
              <div class="next-task">
                <strong>{{
                  nextSchedule ? date(nextSchedule.next_run) : 'Nothing scheduled'
                }}</strong
                ><small>{{
                  nextSchedule
                    ? actionNames[nextSchedule.action] || nextSchedule.action
                    : 'Make time for something else.'
                }}</small>
              </div>
            </div>
          </div>

          <ExistingApps v-if="page === 'Existing apps'" :api="api" />
          <Infrastructure
            v-else-if="page === 'Infrastructure'"
            :api="api"
            :projects="data.projects"
            @changed="load"
          />
          <Networking v-else-if="page === 'Networking'" :api="api" :projects="data.projects" />
          <section v-else-if="page === 'Access'">
            <DashboardHosting :api="api" /><AccessSettings :api="api" /><StackInstall
              category="Identity"
              :api="api"
              @created="
                async (id: string) => {
                  await load();
                  selectedId = id;
                  page = 'Projects';
                }
              "
            />
          </section>
          <template v-else-if="project">
            <div class="project-lifecycle">
              <button
                class="text-button"
                :disabled="!!working[project.id]"
                @click="removeTarget(project)"
              >
                Remove project</button
              ><button
                v-if="projectTab === 'Services' && project.services.length > 1"
                class="text-button"
                :disabled="!!working[project.id]"
                @click="removeTarget(project, activeService)"
              >
                Remove this app
              </button>
            </div>

            <template v-if="projectTab === 'Services'">
              <template
                v-for="service in project.services.filter((s) => s.app === activeService)"
                :key="project.id + ':' + service.app"
                ><ServiceMonitor
                  :url="appUrl(project, service.port)"
                  :writable="writable"
                  :project-id="project.id"
                  :service="service"
                  :access="project.access"
                  :status="serviceStatus(service.app)"
                  :activities="activities.filter((a) => a.project_id === project?.id)"
                  :available="engine.available"
                  :busy="!!working[project.id]"
                  @busy="
                    (value) => {
                      if (value && project) working[project.id] = 'service';
                      else if (project) delete working[project.id];
                    }
                  "
                  @changed="
                    load();
                    checkEngine();
                  " /><AppOnboarding
                  :project-id="project.id"
                  :service="service.app"
                  :writable="writable"
                  @changed="load" /><AppVersionManager
                  :project-id="project.id"
                  :service="service.app"
                  :writable="writable && !working[project.id]"
                  @changed="load"
              /></template>

              <div
                v-for="service in project.services.filter(
                  (s) =>
                    project?.server_id === 'local' &&
                    s.app === activeService &&
                    s.definition.dashboard,
                )"
                class="integration-card"
              >
                <Network :size="22" />
                <div>
                  <h3>Connect your apps</h3>
                  <p>Add this project’s app links to {{ service.definition.name }}.</p>
                </div>
                <button
                  class="button"
                  @click="
                    syncTarget = service.app;
                    showSync = true;
                  "
                >
                  Connect apps<ArrowRight :size="15" />
                </button>
              </div>
            </template>

            <SetupEditor
              v-else-if="projectTab === 'Configuration' && editSetup"
              :key="project.id"
              :project="project"
              :servers="servers"
              :api="api"
              :download="exportSetup"
              :download-backup="exportDatabaseBackup"
              @saved="setupSaved"
              @cancel="editSetup = false"
            />
            <form
              v-else-if="projectTab === 'Configuration'"
              class="settings-form"
              @submit.prevent="saveConfig"
            >
              <section>
                <div class="project-utilities">
                  <button type="button" class="button small" @click="editSetup = true">
                    Environment, files &amp; database</button
                  ><button
                    type="button"
                    class="button small"
                    @click="exportSetup(project.id).catch((e) => (error = e.message))"
                  >
                    Export setup</button
                  ><button type="button" class="button small" @click="viewPlan(project)">
                    Compose file</button
                  ><button type="button" class="button small" @click="action(project, 'snapshot')">
                    Save snapshot</button
                  ><button
                    type="button"
                    class="button small"
                    :disabled="!writable || !!working[project.id]"
                    @click="action(project, 'stop')"
                  >
                    Stop project
                  </button>
                </div>
                <h2>Project settings</h2>
                <label>Project name<input v-model="editor.name" required maxlength="64" /></label
                ><label
                  >Network access<select v-model="editor.access">
                    <option value="local">Server only (loopback)</option>
                    <option value="lan">Local network (all interfaces)</option>
                  </select></label
                >
                <p class="form-help">
                  Network access controls where the apps can be reached. The selfhost dashboard
                  stays local.
                </p>
              </section>
              <section v-for="service in editor.services">
                <h2>{{ project.services.find((s) => s.app === service.app)?.definition.name }}</h2>
                <label
                  >Saved container image<input :value="service.image" readonly class="mono"
                /></label>
                <button
                  type="button"
                  class="button small"
                  @click="
                    activeService = service.app;
                    projectTab = 'Services';
                  "
                >
                  Review app version
                </button>
                <label
                  >Host port<input
                    v-model.number="service.port"
                    type="number"
                    min="1024"
                    max="65535"
                    required
                /></label>
              </section>
              <div class="form-footer">
                <p>Saved changes take effect the next time you start the project.</p>
                <button class="button primary" type="submit">
                  <Check :size="17" />Save configuration
                </button>
              </div>
            </form>

            <div v-else class="activity-list">
              <article
                v-for="activity in activities.filter((a) => a.project_id === project?.id)"
                class="activity-row"
              >
                <span class="activity-icon" :class="activity.status"
                  ><Check v-if="activity.status === 'succeeded'" :size="16" /><X
                    v-else-if="activity.status === 'failed'"
                    :size="16" /><LoaderCircle v-else :size="16" class="spin"
                /></span>
                <div>
                  <strong>{{ actionNames[activity.action] || activity.action }}</strong>
                  <p>{{ activity.message || 'In progress' }}</p>
                </div>
                <time>{{ date(activity.started_at) }}</time>
              </article>
              <div
                v-if="!activities.some((a) => a.project_id === project?.id)"
                class="empty small-empty"
              >
                <Clock3 :size="26" />
                <h3>Nothing here yet</h3>
                <p>Actions for this project will appear here.</p>
              </div>
            </div>
          </template>

          <template v-else-if="page === 'Overview' || page === 'Projects'">
            <div class="section-heading">
              <h2>
                Your projects <span>{{ data.projects.length }}</span>
              </h2>
              <button
                v-if="page === 'Overview' && data.projects.length"
                class="text-button"
                @click="go('Projects')"
              >
                View all<ArrowRight :size="16" /></button
              ><label v-if="page === 'Projects' && data.projects.length" class="search"
                ><Search :size="16" /><input
                  v-model="search"
                  aria-label="Search projects"
                  placeholder="Find a project"
              /></label>
            </div>

            <div v-if="loading" class="empty">
              <LoaderCircle class="spin" :size="25" />
              <p>Opening your workspace…</p>
            </div>

            <div v-else-if="!data.projects.length" class="welcome-card">
              <div class="welcome-copy">
                <span class="welcome-icon"><FolderPlus :size="25" /></span>
                <h2>Start with a project.</h2>
                <p>
                  A dashboard for your homelab. Monitoring for your server. Group the apps you want
                  to run, and give them a home.
                </p>
                <button class="button primary" @click="startCreate()">
                  Create your first project<ArrowRight :size="17" /></button
                ><span class="quiet-note"
                  ><ShieldCheck :size="14" />Private by default. You choose what to share.</span
                >
              </div>
              <div class="welcome-art" aria-hidden="true">
                <div class="orbit orbit-one"></div>
                <div class="orbit orbit-two"></div>
                <div class="orbit orbit-three"></div>
                <span class="art-block art-main"
                  ><span class="brand-mark"><span></span><span></span><span></span></span></span
                ><span class="art-block art-box"><Box :size="28" /></span
                ><span class="art-block art-database"><Database :size="26" /></span
                ><span class="art-block art-monitor"><Network :size="26" /></span
                ><span class="art-dot one"></span><span class="art-dot two"></span>
                <div class="art-label"><i></i>A place of your own</div>
              </div>
            </div>

            <div v-else class="project-grid">
              <button v-for="p in filteredProjects" class="project-card" @click="openProject(p)">
                <div class="project-card-top">
                  <span class="project-symbol"><Folder :size="23" /></span
                  ><span class="status-pill" :class="{ running: status(p) === 'Running' }"
                    ><i></i>{{ status(p) }}</span
                  >
                </div>
                <h3>{{ p.name }}</h3>
                <p>
                  {{ p.services.length }} apps <span>·</span>
                  {{
                    p.custom
                      ? 'Compose networking'
                      : p.access === 'local'
                        ? 'This computer'
                        : 'Local network'
                  }}
                </p>
                <div class="project-card-bottom">
                  <div class="app-stack">
                    <span v-for="s in p.services"
                      ><AppIcon :src="s.definition.icon" :alt="s.definition.name"
                    /></span>
                  </div>
                  <ArrowUpRight :size="19" />
                </div></button
              ><button class="project-card add-project" @click="startCreate()">
                <span><Plus :size="24" /></span><strong>Room for another?</strong>
                <p>Create a project</p>
              </button>
            </div>

            <div v-if="page === 'Overview'" class="overview-bottom">
              <section>
                <div class="section-heading">
                  <h2>Make yourself at home</h2>
                  <button class="text-button" @click="go('App catalog')">
                    Browse catalog<ArrowRight :size="15" />
                  </button>
                </div>
                <div class="recommended">
                  <button v-for="app in catalog.slice(0, 2)" @click="startCreate(app.id)">
                    <span class="app-icon"><AppIcon :src="app.icon" alt="" /></span
                    ><span
                      ><strong>{{ app.name }}</strong
                      ><small>{{ app.category }}</small></span
                    ><Plus :size="17" />
                  </button>
                </div>
                <div class="contribution-note">
                  <Box :size="16" />
                  <p>
                    Have a favorite app?
                    <a href="https://github.com/obiente/selfhost" target="_blank" rel="noopener"
                      >Share a recipe.</a
                    >
                  </p>
                </div>
              </section>
              <section>
                <div class="section-heading">
                  <h2>Recent activity</h2>
                  <button class="text-button" @click="go('Activity')">
                    <ArrowRight :size="16" /><span class="sr-only">View activity</span>
                  </button>
                </div>
                <div v-if="!activities.length" class="quiet-activity">
                  <span><Clock3 :size="22" /></span>
                  <div>
                    <strong>A fresh start.</strong>
                    <p>Your project activity will show up here.</p>
                  </div>
                </div>
                <div v-else class="recent-activities">
                  <div v-for="a in activities.slice(0, 3)">
                    <span class="activity-icon" :class="a.status"
                      ><Check v-if="a.status === 'succeeded'" :size="14" /><Clock3
                        v-else
                        :size="14"
                    /></span>
                    <div>
                      <strong>{{ actionNames[a.action] || a.action }}</strong
                      ><small>{{ a.project_name }} · {{ date(a.started_at) }}</small>
                    </div>
                  </div>
                </div>
              </section>
            </div>
          </template>

          <template v-else-if="page === 'App catalog'"
            ><div class="catalog-tabs">
              <button class="button" @click="showCustom = !showCustom">Custom setup</button>
            </div>
            <StackInstall
              :api="api"
              @created="
                async (id) => {
                  await load();
                  selectedId = id;
                  page = 'Projects';
                }
              "
            />
            <SetupEditor
              v-if="showCustom"
              :servers="servers"
              :api="api"
              :download="exportSetup"
              :download-backup="exportDatabaseBackup"
              @saved="setupSaved"
              @cancel="showCustom = false"
            />
            <div class="catalog-toolbar">
              <label class="search"
                ><Search :size="17" /><input
                  v-model="search"
                  aria-label="Search apps"
                  placeholder="Search for an app or a category" /></label
              ><span>{{ filteredCatalog.length }} community recipes</span>
            </div>
            <div class="catalog-grid">
              <article v-for="app in filteredCatalog" class="catalog-card">
                <div class="catalog-card-top">
                  <span class="app-icon large"><AppIcon :src="app.icon" alt="" /></span
                  ><span class="category">{{ app.category }}</span>
                </div>
                <h2>{{ app.name }}</h2>
                <p>{{ app.description }}</p>
                <div>
                  <button class="button" @click="startCreate(app.id)">
                    <Plus :size="16" />Add to a project</button
                  ><a
                    class="icon-button"
                    :href="app.docs"
                    target="_blank"
                    rel="noopener"
                    :aria-label="`${app.name} documentation`"
                    ><ArrowUpRight :size="18"
                  /></a>
                </div>
              </article>
            </div>
            <div v-if="!filteredCatalog.length" class="empty">
              <Search :size="28" />
              <h3>No apps match your search</h3>
              <button class="text-button" @click="search = ''">Clear search</button>
            </div></template
          >

          <TaskManager
            v-else-if="page === 'Tasks & triggers'"
            :api="api"
            :projects="data.projects"
          />
          <template v-else-if="page === 'Schedules'"
            ><div class="schedule-layout">
              <section class="panel">
                <h2>Add a routine</h2>
                <p>Schedules run while <code>selfhost serve</code> is open.</p>
                <form @submit.prevent="saveSchedule()">
                  <label
                    >Project<select v-model="scheduleForm.project_id" required>
                      <option disabled value="">Choose a project</option>
                      <option v-for="p in data.projects" :value="p.id">{{ p.name }}</option>
                    </select></label
                  ><label
                    >Action<select v-model="scheduleForm.action">
                      <option value="snapshot">Save configuration snapshot</option>
                      <option value="refresh">Refresh configured image tags</option>
                      <template
                        v-for="service in data.projects.find(
                          (p) => p.id === scheduleForm.project_id,
                        )?.services"
                        :key="service.app"
                        ><option
                          v-for="a in (service.definition as any).integration?.actions?.filter(
                            (a: any) => a.schedulable,
                          ) || []"
                          :key="a.id"
                          :value="`app:${service.app}:${a.id}`"
                        >
                          {{ service.definition.name }}: {{ a.label }}
                        </option></template
                      >
                    </select></label
                  ><label
                    >Every<input
                      v-model.number="scheduleForm.interval_hours"
                      type="number"
                      min="1"
                      max="8760"
                      required
                    /><small>hours</small></label
                  ><button class="button primary" :disabled="!data.projects.length">
                    <Plus :size="16" />Save schedule
                  </button>
                </form>
              </section>
              <section class="panel">
                <h2>Your routines</h2>
                <div v-if="!data.schedules.length" class="empty small-empty">
                  <Clock3 :size="28" />
                  <h3>Leave it to a routine</h3>
                  <p>Create a schedule to handle recurring tasks.</p>
                </div>
                <article v-for="s in data.schedules" class="schedule-row">
                  <div>
                    <strong>{{ actionNames[s.action] || s.action }}</strong>
                    <p>
                      {{ data.projects.find((p) => p.id === s.project_id)?.name }} · Every
                      {{ s.interval_hours }} hours
                    </p>
                    <small>{{ s.enabled ? `Next: ${date(s.next_run)}` : 'Paused' }}</small>
                  </div>
                  <button
                    class="switch"
                    :class="{ enabled: s.enabled }"
                    role="switch"
                    :aria-checked="s.enabled"
                    :aria-label="`${s.enabled ? 'Pause' : 'Enable'} ${actionNames[s.action] || s.action} schedule`"
                    @click="saveSchedule({ ...s, enabled: !s.enabled })"
                  >
                    <span></span>
                  </button>
                </article>
              </section></div
          ></template>

          <Databases
            v-else-if="page === 'Databases'"
            :api="api"
            :servers="servers"
            @changed="load"
          /><template v-else-if="page === 'Snapshots'"
            ><div class="info-strip">
              <ShieldCheck :size="20" />
              <p>
                Snapshots include project settings and generated secrets.
                <strong>Application data in Docker volumes is not included.</strong>
              </p>
            </div>
            <div class="snapshot-table">
              <div class="table-header">
                <span>Project</span><span>Saved</span><span>Contents</span><span></span>
              </div>
              <article v-for="s in [...data.snapshots].reverse()">
                <span
                  ><Database :size="18" /><strong>{{ s.project_name }}</strong></span
                ><time>{{ date(s.created_at) }}</time
                ><span>Configuration</span
                ><button class="button small" @click="restore(s)">
                  Restore<ArrowDownToLine :size="14" />
                </button>
              </article>
              <div v-if="!data.snapshots.length" class="empty">
                <Database :size="32" />
                <h3>Keep a moment to return to</h3>
                <p>Open a project and choose Save configuration to take your first snapshot.</p>
                <button class="button" @click="go('Projects')">
                  View projects<ArrowRight :size="15" />
                </button>
              </div></div
          ></template>

          <template v-else-if="page === 'Activity'"
            ><div class="activity-list">
              <article v-for="a in activities" class="activity-row">
                <span class="activity-icon" :class="a.status"
                  ><Check v-if="a.status === 'succeeded'" :size="17" /><X
                    v-else-if="a.status === 'failed'"
                    :size="17" /><LoaderCircle v-else class="spin" :size="17"
                /></span>
                <div>
                  <strong
                    >{{ actionNames[a.action] || a.action }}
                    <span>· {{ a.project_name }}</span></strong
                  >
                  <p>{{ a.message || 'In progress' }}</p>
                </div>
                <time>{{ date(a.started_at) }}</time
                ><i v-if="!a.read" class="unread-dot"></i>
              </article>
              <div v-if="!activities.length" class="empty">
                <Bell :size="30" />
                <h3>All quiet here</h3>
                <p>Completed actions and anything needing attention will appear here.</p>
              </div>
            </div></template
          >

          <section v-if="page === 'Projects' && !project" class="project-archives">
            <button
              class="text-button"
              :aria-expanded="showArchives"
              @click="showArchives = !showArchives"
            >
              {{ showArchives ? 'Hide archived projects' : 'View archived projects' }}</button
            ><ArchivedProjects v-if="showArchives" :api="api" @restored="load" />
          </section>
          <footer v-if="!project" class="page-footer">
            <span><span class="tiny-mark">▦</span>selfhost by Obiente</span
            ><a href="https://github.com/obiente/selfhost" target="_blank" rel="noopener"
              >Source code<ArrowUpRight :size="13"
            /></a>
          </footer>
        </template>
      </main>
    </div>

    <div v-if="error" class="error-toast" role="alert">
      <div>
        <strong>Something needs attention</strong>
        <p>{{ error }}</p>
      </div>
      <button class="icon-button" aria-label="Dismiss error" @click="error = ''">
        <X :size="18" />
      </button>
    </div>

    <div v-if="toast" class="toast" role="status"><Check :size="18" />{{ toast }}</div>

    <DeploymentPicker
      :open="deploymentOpen"
      :app="deploymentApp"
      :api="api"
      @update:open="deploymentOpen = $event"
      @created="deploymentCreated"
    />
    <RemovalDialog
      v-if="removalTarget"
      :open="!!removalTarget"
      :project-id="removalTarget.id"
      :project-name="removalTarget.name"
      :service="removalTarget.service"
      :read-only="removalTarget.readOnly"
      :api="api"
      @update:open="
        (value) => {
          if (!value) removalTarget = null;
        }
      "
      @removed="removed"
    />
    <BaseDialog
      v-model:open="showCreate"
      title="Create a project"
      description="Choose a name and the services you want to run."
      :busy="creating"
      ><form @submit.prevent="createProject">
        <label
          >Project name<input
            v-model="form.name"
            required
            maxlength="64"
            placeholder="e.g. Homelab"
            autofocus /></label
        ><label
          >Server<select v-model="form.server_id">
            <option
              v-for="s in servers.filter((s) => s.provider !== 'proxmox_ssh')"
              :key="s.id"
              :value="s.id"
            >
              {{ s.name }}{{ s.read_only ? ' (read-only)' : '' }}
            </option>
          </select></label
        ><label class="label">Add your apps</label>
        <div v-for="app in catalog" :key="app.id" class="project-app-choice">
          <label class="app-choice" :class="{ chosen: form.apps.includes(app.id) }"
            ><span class="app-icon"><AppIcon :src="app.icon" alt="" /></span
            ><span
              ><strong>{{ app.name }}</strong
              ><small>{{ app.description }}</small></span
            ><input
              v-if="!app.deployment_only"
              v-model="form.apps"
              type="checkbox"
              :value="app.id"
            /><span v-else>Full setup required</span></label
          ><button
            v-if="
              app.deployment_only || deploymentMethods.filter((m) => m.app === app.id).length > 1
            "
            type="button"
            class="text-button deployment-choice-link"
            @click="chooseDeployment(app.id)"
          >
            Choose deployment method
          </button>
          <AppVersionPicker
            v-if="form.apps.includes(app.id)"
            v-model="createVersions[app.id]"
            :app="app.id"
            :disabled="creating"
            @valid="createVersionValid[app.id] = $event"
          />
        </div>
        <div class="modal-note">
          <ShieldCheck :size="17" />
          <p>
            Apps initially bind to the selected server’s loopback interface. You can change network
            access later.
          </p>
        </div>
        <button
          class="button primary wide"
          type="submit"
          :disabled="creating || !form.name.trim() || !form.apps.length || !allVersionsValid"
        >
          <LoaderCircle v-if="creating" class="spin" :size="17" /><Plus v-else :size="17" />{{
            creating ? 'Creating…' : 'Create project'
          }}
        </button>
      </form></BaseDialog
    >

    <BaseDialog
      :open="showSync"
      @update:open="
        (value) => {
          showSync = value;
          if (!value) key = '';
        }
      "
      title="Connect your apps"
      description="Use the private API connection created during app setup, or provide a dashboard API key for this sync."
      ><form @submit.prevent="sync">
        <label
          >Dashboard API key (optional after automatic setup)<input
            v-model="key"
            type="password"
            autocomplete="off"
        /></label>
        <p class="form-help">
          Existing links are kept. App links use localhost and are intended for this computer.
        </p>
        <button class="button primary wide" type="submit">
          <Network :size="17" />Sync app links
        </button>
      </form></BaseDialog
    >

    <BaseDialog
      v-model:open="showPlan"
      title="Compose configuration"
      description="Secrets are referenced from the project environment file."
      wide
    >
      <pre class="compose-preview" tabindex="0">{{ plan }}</pre>
    </BaseDialog>
  </div>
</template>
