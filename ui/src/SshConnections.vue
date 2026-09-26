<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
const props = withDefaults(
  defineProps<{
    api: (path: string, method?: string, body?: unknown) => Promise<any>;
    purpose?: string;
  }>(),
  { purpose: 'docker' },
);
const emit = defineEmits<{ select: [alias: string, purpose: string] }>();
const items = ref<any[]>([]),
  current = ref<any>(null),
  scan = ref<any>(null),
  busy = ref(false),
  error = ref(''),
  notice = ref(''),
  verified = ref(''),
  revoked = ref(false),
  enableConsent = ref(false);
const form = reactive({ name: '', host: '', user: 'selfhost', port: 22, purpose: props.purpose });
async function run(action: () => Promise<void>) {
  busy.value = true;
  error.value = '';
  notice.value = '';
  try {
    await action();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
  }
}
async function load() {
  items.value = await props.api('/ssh');
}
function select(item: any) {
  current.value = item;
  scan.value = null;
  verified.value = '';
  enableConsent.value = false;
  revoked.value = false;
  notice.value = '';
}
async function create() {
  await run(async () => {
    select(await props.api('/ssh', 'POST', form));
    await load();
  });
}
async function refresh() {
  current.value = await props.api(`/ssh/${current.value.profile.id}`);
  await load();
}
async function discover() {
  await run(async () => {
    scan.value = await props.api(`/ssh/${current.value.profile.id}/scan`, 'POST');
  });
}
async function trust() {
  await run(async () => {
    await props.api(`/ssh/${current.value.profile.id}/trust`, 'POST', {
      public_key: scan.value.public_key,
      fingerprint: verified.value.trim(),
    });
    await refresh();
    notice.value = 'Host key verified and pinned.';
  });
}
async function test() {
  await run(async () => {
    const result = await props.api(`/ssh/${current.value.profile.id}/test`, 'POST');
    notice.value = result.message;
    await refresh();
  });
}
async function enable() {
  await run(async () => {
    await props.api(`/ssh/${current.value.profile.id}/enable`, 'POST');
    await refresh();
    notice.value = 'SSH alias enabled on the machine running Selfhost.';
  });
}
async function remove() {
  await run(async () => {
    await props.api(`/ssh/${current.value.profile.id}/remove`, 'POST', { revoked: revoked.value });
    current.value = null;
    await load();
    notice.value = 'Local key and alias removed.';
  });
}
async function copy(value: string) {
  await run(async () => {
    await navigator.clipboard.writeText(value);
    notice.value = 'Copied.';
  });
}
onMounted(() => run(load));
</script>
<template>
  <details class="ssh-panel panel">
    <summary>Dedicated SSH connections</summary>
    <p>
      Create separate keys for Selfhost on this machine. Private keys stay in its protected data
      directory. Use the same account and data directory when running the CLI or background
      dashboard.
    </p>
    <p v-if="error" role="alert" class="monitor-error">{{ error }}</p>
    <p v-if="notice" role="status">{{ notice }}</p>
    <div class="ssh-list" aria-label="Saved SSH connections">
      <button
        type="button"
        class="button"
        v-for="item in items"
        :key="item.profile.id"
        :disabled="busy"
        :aria-pressed="current?.profile.id === item.profile.id"
        @click="select(item)"
      >
        {{ item.profile.name }} · {{ item.profile.user }}@{{ item.profile.host }}
      </button>
      <button type="button" class="button" :disabled="busy" @click="current = null">New key</button>
    </div>
    <form v-if="!current" class="ssh-fields" @submit.prevent="create">
      <label>Connection name<input v-model="form.name" required maxlength="64" /></label>
      <label
        >Server hostname or IP<input
          v-model="form.host"
          required
          maxlength="253"
          autocomplete="off"
      /></label>
      <label
        >SSH account<input v-model="form.user" required maxlength="128" autocomplete="off"
      /></label>
      <label
        >SSH port<input v-model.number="form.port" type="number" min="1" max="65535" required
      /></label>
      <label
        >Use this key for<select v-model="form.purpose">
          <option value="docker">Docker host</option>
          <option value="proxmox">Proxmox host</option>
          <option value="proxy">Reverse proxy</option>
        </select></label
      >
      <p class="ssh-wide">
        The Ed25519 key has no passphrase so unattended operations can use it. Protect Selfhost's
        account and data backups. This creates a local key; it does not grant remote access.
      </p>
      <button class="button primary" :disabled="busy">Create dedicated key</button>
    </form>
    <section v-else :aria-busy="busy">
      <h3>{{ current.profile.name }}</h3>
      <p>
        {{ current.profile.user }}@{{ current.profile.host }}:{{ current.profile.port }} ·
        {{ current.profile.purpose }}
      </p>
      <details>
        <summary>Public key and fingerprint</summary>
        <pre>{{ current.profile.public_key }}</pre>
        <code>{{ current.profile.fingerprint }}</code>
      </details>
      <h4>1. Verify this server</h4>
      <p v-if="current.host_trusted">
        A server host key is pinned. Connections will reject a different key.
      </p>
      <template v-else>
        <p>
          Read the host key, then compare its fingerprint using the server's console or another
          trusted channel.
        </p>
        <pre>ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256</pre>
        <button type="button" class="button" :disabled="busy" @click="discover">
          Read server host key
        </button>
        <details>
          <summary>Enter a host key manually</summary>
          <button type="button" class="button" @click="scan = { public_key: '', fingerprint: '' }">
            Enter public host key</button
          ><label v-if="scan"
            >Server public key<textarea v-model="scan.public_key" rows="2" />
          </label>
        </details>
        <form v-if="scan" @submit.prevent="trust">
          <p>
            Fingerprint reported by this connection:
            <code>{{ scan.fingerprint || 'Enter the verified fingerprint below' }}</code>
          </p>
          <label
            >Fingerprint verified through a trusted channel<input
              v-model="verified"
              required
              placeholder="SHA256:…"
              autocomplete="off"
          /></label>
          <button class="button primary" :disabled="busy || !verified.trim()">
            Trust this host key
          </button>
        </form>
      </template>
      <h4>2. Authorize the key on the server</h4>
      <p>
        Run this as <strong>{{ current.profile.user }}</strong> on
        <strong>{{ current.profile.host }}</strong
        >. It adds this public key once and keeps existing keys.
      </p>
      <p v-if="current.profile.purpose === 'docker'">
        This key permits Docker's SSH transport only. Docker access can control the host; Selfhost's
        read-only setting is an application control, not an SSH permission boundary.
      </p>
      <p v-else>
        Proxy and Proxmox operations need remote commands. This key disables forwarding, agent
        forwarding and terminal allocation; command access still has this account's privileges. Use
        an account with only the permissions you intend to grant.
      </p>
      <pre>{{ current.install_script }}</pre>
      <button type="button" class="button" :disabled="busy" @click="copy(current.install_script)">
        Copy authorization command
      </button>
      <h4>3. Test and enable the connection</h4>
      <button type="button" class="button" :disabled="busy || !current.host_trusted" @click="test">
        Test dedicated key
      </button>
      <p v-if="current.tested">The dedicated key passed a connection test.</p>
      <template v-if="!current.alias_enabled">
        <label class="ssh-consent"
          ><input type="checkbox" v-model="enableConsent" />Add this alias to the SSH config of the
          account running Selfhost. Preserve a backup and keep existing entries.</label
        >
        <button
          type="button"
          class="button primary"
          :disabled="busy || !current.tested || !enableConsent"
          @click="enable"
        >
          Enable SSH alias
        </button>
      </template>
      <template v-else
        ><p>
          Alias: <code>{{ current.profile.id }}</code>
        </p>
        <button
          type="button"
          class="button primary"
          :disabled="busy"
          @click="emit('select', current.profile.id, current.profile.purpose)"
        >
          Use this connection
        </button></template
      >
      <details>
        <summary>OpenSSH configuration</summary>
        <pre>{{ current.config }}</pre>
        <p>
          The key and config are ordinary OpenSSH files. Keep them to use this connection without
          Selfhost.
        </p>
      </details>
      <details>
        <summary>Revoke and remove this connection</summary>
        <p>{{ current.revoke_instructions }}</p>
        <label class="ssh-consent"
          ><input type="checkbox" v-model="revoked" />I removed this public key from the remote
          server and want to delete its local private key.</label
        ><button type="button" class="button" :disabled="busy || !revoked" @click="remove">
          Delete local key and alias
        </button>
      </details>
    </section>
  </details>
</template>
<style scoped>
.ssh-panel {
  margin-block: 1rem;
}
.ssh-panel summary {
  cursor: pointer;
  font-weight: 600;
  padding-block: 0.65rem;
}
.ssh-list {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-block: 1rem;
}
.ssh-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}
.ssh-wide {
  grid-column: 1/-1;
}
.ssh-panel label {
  display: grid;
  gap: 0.35rem;
  margin-block: 0.65rem;
}
.ssh-panel pre {
  padding: 1rem;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--bg);
  border: 1px solid var(--line);
  border-radius: 8px;
}
.ssh-panel code {
  overflow-wrap: anywhere;
}
.ssh-panel .ssh-consent {
  display: flex;
  align-items: flex-start;
  gap: 0.65rem;
}
.ssh-consent input {
  width: auto;
  flex-shrink: 0;
  margin-top: 0.3rem;
}
.ssh-panel h4 {
  margin-top: 1.5rem;
}
.ssh-panel button {
  margin-block: 0.35rem;
}
.ssh-panel input,
.ssh-panel select,
.ssh-panel textarea {
  min-width: 0;
}
@media (max-width: 650px) {
  .ssh-fields {
    grid-template-columns: 1fr;
  }
}
</style>
