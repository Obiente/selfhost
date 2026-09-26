<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { Archive, RefreshCw } from 'lucide-vue-next';
import BaseDialog from './components/BaseDialog.vue';
import EmptyState from './components/EmptyState.vue';
import { useAsyncTask } from './composables/useAsyncTask';
const props = defineProps<{
  api: (path: string, method?: string, body?: unknown) => Promise<any>;
}>();
const emit = defineEmits<{ restored: [] }>();
const labels: Record<string, string> = {
  runtime_pending: 'Removal interrupted',
  state_pending: 'Ready for recovery',
  prepared: 'Review prepared',
  completed: 'Archived',
  reconciled: 'Configuration recovered',
  restored: 'Restored',
};
const archives = ref<any[]>([]),
  selected = ref<any>(),
  confirmation = ref('');
const { busy, error, run } = useAsyncTask();
async function load() {
  archives.value = await props.api('/removal/archives');
}
onMounted(() => run(load));
async function restore() {
  await run(async () => {
    await props.api(
      `/removal/archives/${selected.value.id}/${selected.value.needs_reconciliation ? 'reconcile' : 'restore'}`,
      'POST',
      { confirmation: confirmation.value },
    );
    selected.value = undefined;
    await load();
    emit('restored');
  });
}
</script>
<template>
  <section class="archived-projects">
    <header class="section-heading">
      <h2><Archive :size="20" />Archived projects and apps</h2>
      <button class="icon-button" aria-label="Refresh archives" :disabled="busy" @click="run(load)">
        <RefreshCw :size="17" />
      </button>
    </header>
    <p>
      Restore project records or reconcile an interrupted removal. Containers are never started by
      these actions.
    </p>
    <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
    <EmptyState
      v-if="!busy && !archives.length"
      title="No archived projects"
      description="Removed project records and their backup references will appear here."
    />
    <article v-for="item in archives" :key="item.id" class="archive-row">
      <div>
        <strong>{{ item.project_name }}</strong>
        <p>
          {{ labels[item.status] || item.status
          }}<span v-if="item.service"> · {{ item.service }}</span>
        </p>
        <small v-if="item.snapshot_id">Configuration backup saved</small
        ><small v-if="item.database_backup_id">Database backup saved</small>
      </div>
      <button
        v-if="item.can_restore_project || item.needs_reconciliation"
        class="button"
        @click="
          selected = item;
          confirmation = '';
        "
      >
        {{ item.needs_reconciliation ? 'Reconcile removal' : 'Restore project' }}
      </button>
    </article>
    <BaseDialog
      :open="!!selected"
      :title="selected?.needs_reconciliation ? 'Reconcile removal' : 'Restore project record'"
      description="Restore the saved configuration and pause its schedules. Review the project before starting anything."
      :busy="busy"
      @update:open="
        (value) => {
          if (!value) selected = undefined;
        }
      "
      ><form v-if="selected" @submit.prevent="restore">
        <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
        <label
          >Type <strong>{{ selected.project_name }}</strong> to confirm<input
            v-model="confirmation"
            autocomplete="off"
            required /></label
        ><button class="button primary" :disabled="busy || confirmation !== selected.project_name">
          {{ selected.needs_reconciliation ? 'Reconcile' : 'Restore record' }}
        </button>
      </form></BaseDialog
    >
  </section>
</template>
<style scoped>
.archived-projects {
  margin-top: 32px;
  border-top: 1px solid var(--border);
  padding-top: 24px;
}
.archived-projects > p {
  color: var(--muted);
  line-height: 1.6;
  margin: 12px 0 20px;
}
.section-heading h2 {
  display: flex;
  align-items: center;
  gap: 10px;
}
.archive-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  border-bottom: 1px solid var(--border);
  padding: 20px 0;
}
.archive-row p,
.archive-row small {
  color: var(--muted);
  font-size: 12px;
  margin-top: 6px;
  display: block;
}
@media (max-width: 600px) {
  .archive-row {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
