<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from 'vue';
import { ChevronDown, LogOut, Settings2, ShieldCheck, UserRound } from 'lucide-vue-next';
export type Account = {
  kind: 'local' | 'oidc';
  name: string;
  provider: string;
  subject: string;
  role: string;
  created_at: number;
  expires_at: number;
  idle_expires_at: number;
};
const props = defineProps<{ account: Account; busy: boolean; error: string }>();
const emit = defineEmits<{ access: []; signout: [all: boolean]; refresh: [] }>();
const open = ref(false),
  confirmAll = ref(false),
  root = ref<HTMLElement>(),
  trigger = ref<HTMLButtonElement>();
function close(restore = false) {
  open.value = false;
  confirmAll.value = false;
  if (restore) nextTick(() => trigger.value?.focus());
}
function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) close();
}
function keys(event: KeyboardEvent) {
  if (open.value && event.key === 'Escape') {
    event.preventDefault();
    close(true);
  }
}
function toggle() {
  open.value = !open.value;
  confirmAll.value = false;
  if (open.value) emit('refresh');
}
function time(value: number) {
  return new Date(value * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
onMounted(() => {
  document.addEventListener('pointerdown', outside);
  document.addEventListener('keydown', keys);
});
onUnmounted(() => {
  document.removeEventListener('pointerdown', outside);
  document.removeEventListener('keydown', keys);
});
</script>

<template>
  <div
    ref="root"
    class="account-control"
    @focusout="
      (event: FocusEvent) => {
        if (event.relatedTarget && !root?.contains(event.relatedTarget as Node)) close();
      }
    "
  >
    <button
      ref="trigger"
      class="account-trigger"
      :aria-expanded="open"
      aria-controls="account-panel"
      :aria-label="`Account: ${props.account.name}`"
      @click="toggle"
    >
      <UserRound :size="18" /><span>{{
        account.kind === 'local' ? 'Local admin' : account.name
      }}</span
      ><ChevronDown :size="14" />
    </button>
    <section v-if="open" id="account-panel" class="account-panel" aria-label="Your account">
      <header>
        <span class="account-symbol"><UserRound :size="22" /></span>
        <div>
          <h2>{{ account.name }}</h2>
          <p>{{ account.provider }}</p>
        </div>
      </header>
      <p class="account-role">
        <ShieldCheck :size="15" />{{ account.role }}<span>Full workspace access</span>
      </p>
      <dl>
        <template v-if="account.subject"
          ><dt>Identity</dt>
          <dd>{{ account.subject }}</dd></template
        >
        <dt>Session ends</dt>
        <dd>{{ time(account.expires_at) }}</dd>
        <dt>Inactive sign-out</dt>
        <dd>After 30 minutes</dd>
      </dl>
      <p v-if="account.kind === 'local'" class="account-hint">
        Signed in on this computer. After signing out, restart <code>selfhost serve</code> for a
        fresh local link, or use your identity provider.
      </p>
      <p v-else class="account-hint">
        Signing out here ends your Selfhost session. Your identity provider may keep you signed in.
      </p>
      <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
      <div class="account-actions">
        <button
          :disabled="busy"
          @click="
            close();
            emit('access');
          "
        >
          <Settings2 :size="17" />Login &amp; access settings
        </button>
        <button :disabled="busy" @click="emit('signout', false)">
          <LogOut :size="17" />{{ busy ? 'Signing out...' : 'Sign out' }}
        </button>
        <button
          v-if="account.kind === 'oidc' && !confirmAll"
          :disabled="busy"
          @click="confirmAll = true"
        >
          Sign out all my sessions
        </button>
      </div>
      <div v-if="confirmAll" class="account-confirm">
        <p>End all your Selfhost sessions, including this one?</p>
        <button class="button" :disabled="busy" @click="confirmAll = false">Cancel</button
        ><button class="button primary" :disabled="busy" @click="emit('signout', true)">
          Sign out all
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.account-control {
  position: relative;
}
.account-trigger {
  display: flex;
  align-items: center;
  gap: 9px;
  border: 1px solid var(--border);
  border-radius: 24px;
  background: #333930;
  color: #eee8d7;
  padding: 8px 12px;
  max-width: 210px;
  cursor: pointer;
}
.account-trigger > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.account-trigger > svg {
  flex-shrink: 0;
}
.account-trigger:hover,
.account-trigger[aria-expanded='true'] {
  border-color: #d8b870;
}
.account-panel {
  position: absolute;
  right: 0;
  top: calc(100% + 12px);
  width: 340px;
  max-width: calc(100vw - 32px);
  padding: 22px;
  background: #262c27;
  border: 1px solid #535b4e;
  border-radius: 14px;
  box-shadow: 0 16px 48px #0006;
  z-index: 80;
  text-align: left;
}
.account-panel header {
  display: flex;
  gap: 13px;
  align-items: center;
}
.account-panel h2 {
  font-size: 17px;
  margin: 0;
  overflow-wrap: anywhere;
}
.account-panel p {
  line-height: 1.5;
}
.account-panel header p {
  margin: 4px 0 0;
  color: var(--muted);
  font-size: 13px;
}
.account-symbol {
  display: grid;
  place-items: center;
  min-width: 42px;
  height: 42px;
  border-radius: 50%;
  background: #3a4032;
  color: #d8b870;
}
.account-role {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: #d8b870;
  flex-wrap: wrap;
}
.account-role span {
  color: var(--muted);
  margin-left: auto;
}
.account-panel dl {
  display: grid;
  grid-template-columns: 1fr 1.2fr;
  gap: 10px;
  font-size: 12px;
  border-block: 1px solid var(--border);
  padding: 16px 0;
}
.account-panel dt {
  color: var(--muted);
}
.account-panel dd {
  margin: 0;
  text-align: right;
  overflow-wrap: anywhere;
}
.account-hint {
  font-size: 12px;
  color: var(--muted);
}
.account-actions {
  display: grid;
  gap: 4px;
  margin-top: 12px;
}
.account-actions button {
  background: none;
  border: 0;
  text-align: left;
  padding: 11px 8px;
  display: flex;
  gap: 10px;
  align-items: center;
  color: inherit;
  border-radius: 7px;
  cursor: pointer;
}
.account-actions button:hover {
  background: #384034;
}
.account-confirm {
  font-size: 13px;
  border-top: 1px solid var(--border);
  margin-top: 12px;
}
.account-confirm .button {
  margin-right: 8px;
}
@media (max-width: 640px) {
  .account-trigger {
    padding: 8px;
  }
  .account-trigger > span,
  .account-trigger > svg:last-child {
    display: none;
  }
  .account-panel {
    position: fixed;
    top: 64px;
    right: 12px;
    max-height: calc(100dvh - 82px);
    overflow: auto;
  }
}
</style>
