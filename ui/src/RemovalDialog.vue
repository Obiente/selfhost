<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import {
  AlertDialogRoot,
  AlertDialogPortal,
  AlertDialogOverlay,
  AlertDialogContent,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogCancel,
} from 'reka-ui';
import { Archive, ArrowLeft, LoaderCircle, ShieldCheck, Trash2 } from 'lucide-vue-next';

type Plan = {
  id: string;
  revision: string;
  confirmation: string;
  expires_at: number;
  containers: { id: string; name: string; service: string }[];
  scope: string[];
  warnings: string[];
  database_backup_available: boolean;
};
const props = defineProps<{
  open: boolean;
  projectId: string;
  projectName: string;
  service?: string;
  readOnly?: boolean;
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const emit = defineEmits<{
  'update:open': [value: boolean];
  removed: [
    result: {
      archive_id: string;
      project_removed: boolean;
      snapshot_id?: string;
      database_backup_id?: string;
    },
  ];
}>();
const mode = ref<'archive' | 'remove_containers'>('archive');
const backup = ref<'configuration' | 'configuration_and_database' | 'none'>('configuration');
const plan = ref<Plan>(),
  busy = ref(false),
  error = ref(''),
  confirmation = ref('');
const acknowledge = ref(false),
  noBackup = ref(false),
  databaseAvailable = ref(false);
const heading = ref<HTMLElement>();
const target = computed(() => props.service || props.projectName);
const valid = computed(
  () =>
    plan.value &&
    confirmation.value === plan.value.confirmation &&
    acknowledge.value &&
    (backup.value !== 'none' || noBackup.value),
);
watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    mode.value = 'archive';
    backup.value = 'configuration';
    plan.value = undefined;
    error.value = '';
    confirmation.value = '';
    acknowledge.value = false;
    noBackup.value = false;
    databaseAvailable.value = false;
    // Read-only setup inspection controls backup availability without changing anything.
    try {
      const setup = await props.api(`/projects/${props.projectId}/setup`);
      if (props.open) databaseAvailable.value = !!setup.database?.provisioned;
    } catch {
      /* The review request reports actionable errors. */
    }
  },
  { immediate: true },
);
async function review() {
  busy.value = true;
  error.value = '';
  try {
    plan.value = await props.api(`/projects/${props.projectId}/removal/plan`, 'POST', {
      service: props.service || null,
      mode: mode.value,
      backup: backup.value,
    });
    confirmation.value = '';
    acknowledge.value = false;
    noBackup.value = false;
    await nextTick();
    heading.value?.focus();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function apply() {
  if (!valid.value || !plan.value || busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await props.api(`/projects/${props.projectId}/removal/apply`, 'POST', {
      plan_id: plan.value.id,
      revision: plan.value.revision,
      confirmation: confirmation.value,
      acknowledge_preserved_data: acknowledge.value,
      acknowledge_no_backup: noBackup.value,
    });
    emit('removed', result);
    emit('update:open', false);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
function back() {
  plan.value = undefined;
  confirmation.value = '';
  error.value = '';
}
</script>

<template>
  <AlertDialogRoot
    :open="open"
    @update:open="
      (value) => {
        if (!busy) emit('update:open', value);
      }
    "
  >
    <AlertDialogPortal>
      <AlertDialogOverlay class="dialog-overlay" />
      <AlertDialogContent
        class="base-dialog removal-dialog"
        :aria-busy="busy"
        @escape-key-down="
          (event) => {
            if (busy) event.preventDefault();
          }
        "
      >
        <AlertDialogTitle class="dialog-title">Remove {{ target }}</AlertDialogTitle>
        <AlertDialogDescription class="dialog-description"
          >Review what changes and choose a backup before removing
          {{ service ? 'this app' : 'this project' }}.</AlertDialogDescription
        >
        <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
        <form v-if="!plan" @submit.prevent="review">
          <fieldset :disabled="busy">
            <legend>What should happen?</legend>
            <label class="removal-option"
              ><input v-model="mode" type="radio" value="archive" name="removal-mode" /><span
                ><strong><Archive :size="17" />Archive in Selfhost</strong
                ><small
                  >Keep running containers and their data. Remove this selection from active
                  management and keep a restorable record.</small
                ></span
              ></label
            >
            <label class="removal-option" :class="{ unavailable: readOnly }"
              ><input
                v-model="mode"
                type="radio"
                value="remove_containers"
                name="removal-mode"
                :disabled="readOnly"
              /><span
                ><strong><Trash2 :size="17" />Remove containers</strong
                ><small
                  >Stop and delete the reviewed containers. Keep volumes, mounted files, networks
                  and external databases.</small
                ></span
              ></label
            >
            <p v-if="readOnly" class="subtle">
              This server is read-only. Its containers cannot be removed here.
            </p>
          </fieldset>
          <fieldset :disabled="busy">
            <legend>Create a backup first?</legend>
            <label class="removal-option"
              ><input
                v-model="backup"
                type="radio"
                value="configuration"
                name="removal-backup"
              /><span
                ><strong>Configuration snapshot</strong
                ><small
                  >Save settings, environment variables and managed config files. App files and
                  database contents are not included.</small
                ></span
              ></label
            >
            <label v-if="databaseAvailable" class="removal-option"
              ><input
                v-model="backup"
                type="radio"
                value="configuration_and_database"
                name="removal-backup"
              /><span
                ><strong>Configuration and database</strong
                ><small
                  >Also create a PostgreSQL archive of the connected project database. App files,
                  other databases and container layers are not included.</small
                ></span
              ></label
            >
            <label class="removal-option"
              ><input v-model="backup" type="radio" value="none" name="removal-backup" /><span
                ><strong>Continue without a new backup</strong
                ><small
                  >Existing backups and local project files are kept. You must confirm this choice
                  in the next step.</small
                ></span
              ></label
            >
          </fieldset>
          <div class="removal-note">
            <ShieldCheck :size="20" />
            <p v-if="backup !== 'none'">
              If the selected backup fails, removal stops. Back up app files with the app's backup
              tools if you need a complete recovery copy.
            </p>
            <p v-else>
              Archive records help recover Selfhost settings. They do not replace a separate backup
              of your applications and their data.
            </p>
          </div>
          <footer>
            <AlertDialogCancel class="button" :disabled="busy">Cancel</AlertDialogCancel
            ><button class="button primary" :disabled="busy">
              <LoaderCircle v-if="busy" :size="16" class="spin" />{{
                busy ? 'Preparing review...' : 'Review removal'
              }}
            </button>
          </footer>
        </form>
        <form v-else @submit.prevent="apply">
          <h3 ref="heading" tabindex="-1">Confirm the scope</h3>
          <ul class="removal-scope">
            <li v-for="item in plan.scope" :key="item">{{ item }}</li>
          </ul>
          <div class="removal-warning">
            <p v-for="warning in plan.warnings" :key="warning">{{ warning }}</p>
          </div>
          <details v-if="mode === 'remove_containers'" open>
            <summary>
              {{ plan.containers.length }}
              {{ plan.containers.length === 1 ? 'container' : 'containers' }} selected
            </summary>
            <ul>
              <li v-for="container in plan.containers" :key="container.id">
                <strong>{{ container.name }}</strong
                ><code>{{ container.id }}</code>
              </li>
            </ul>
            <p v-if="!plan.containers.length">
              No matching containers exist. Only the active Selfhost record will change.
            </p>
          </details>
          <p class="subtle">
            Backup:
            {{
              backup === 'configuration'
                ? 'configuration only'
                : backup === 'configuration_and_database'
                  ? 'configuration and connected PostgreSQL database'
                  : 'no new backup'
            }}. This review expires after ten minutes.
          </p>
          <label class="acknowledge"
            ><input v-model="acknowledge" type="checkbox" :disabled="busy" required /><span
              >I understand that stored volumes and external databases remain, and that data only
              inside removed containers is not backed up.</span
            ></label
          >
          <label v-if="backup === 'none'" class="acknowledge"
            ><input v-model="noBackup" type="checkbox" :disabled="busy" required /><span
              >I choose to continue without creating a new backup.</span
            ></label
          >
          <label
            >Type <strong>{{ plan.confirmation }}</strong> to confirm<input
              v-model="confirmation"
              :disabled="busy"
              autocomplete="off"
              spellcheck="false"
              required
              :aria-invalid="confirmation.length > 0 && confirmation !== plan.confirmation"
          /></label>
          <p v-if="busy" role="status">
            Saving the selected backup and applying the reviewed changes...
          </p>
          <footer>
            <button class="button" type="button" :disabled="busy" @click="back">
              <ArrowLeft :size="16" />Back</button
            ><AlertDialogCancel class="button" :disabled="busy">Cancel</AlertDialogCancel
            ><button class="button removal-confirm" :disabled="!valid || busy">
              <LoaderCircle v-if="busy" :size="16" class="spin" />{{
                mode === 'archive' ? 'Archive selection' : 'Remove containers'
              }}
            </button>
          </footer>
        </form>
      </AlertDialogContent>
    </AlertDialogPortal>
  </AlertDialogRoot>
</template>

<style scoped>
.removal-dialog {
  width: min(670px, calc(100vw - 32px));
}
.dialog-title {
  margin: 0 0 10px;
}
.removal-dialog fieldset {
  border: 0;
  padding: 0;
  margin: 24px 0;
}
.removal-dialog legend {
  font-weight: 650;
  margin-bottom: 10px;
}
.removal-option {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 14px;
  margin: 9px 0;
  cursor: pointer;
}
.removal-option:has(input:checked) {
  border-color: #d8b870;
  background: #35392c;
}
.removal-option input,
.acknowledge input {
  flex: 0 0 auto;
  width: 18px;
  height: 18px;
  margin: 3px 0;
  accent-color: #d8b870;
}
.removal-option strong {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
}
.removal-option small {
  display: block;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.55;
  margin-top: 5px;
}
.unavailable {
  opacity: 0.65;
  cursor: not-allowed;
}
.removal-note {
  display: flex;
  gap: 12px;
  align-items: flex-start;
  background: #30362c;
  padding: 14px;
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.6;
}
.removal-note svg {
  flex-shrink: 0;
  margin-top: 3px;
  color: #d8b870;
}
.removal-note p {
  margin: 0;
}
.removal-dialog footer {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 24px;
}
.removal-scope {
  padding-left: 22px;
  line-height: 1.65;
}
.removal-warning {
  padding: 1px 15px;
  border-left: 3px solid #d8b870;
  background: #38362c;
  border-radius: 0 8px 8px 0;
  font-size: 13px;
  line-height: 1.6;
}
.acknowledge {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  font-size: 13px;
  line-height: 1.6;
  margin: 16px 0;
}
.removal-dialog details {
  margin: 18px 0;
}
.removal-dialog details li {
  margin: 10px 0;
}
.removal-dialog code {
  display: block;
  font-size: 11px;
  overflow-wrap: anywhere;
  color: var(--muted);
  margin-top: 4px;
}
.removal-dialog .subtle {
  font-size: 13px;
  line-height: 1.55;
  color: var(--muted);
}
.removal-confirm {
  background: #764637;
  border-color: #c38669;
  color: #fff5e9;
}
.removal-confirm:hover:not(:disabled) {
  background: #8c503c;
}
.removal-dialog h3:focus {
  outline: 2px solid #d8b870;
  outline-offset: 5px;
}
.removal-dialog input:focus-visible {
  outline: 2px solid #d8b870;
  outline-offset: 3px;
}
.removal-dialog button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}
</style>
