<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
const props = defineProps<{ api: (path: string) => Promise<any> }>();
const runtime = ref<any>(null);
const error = ref('');
const commands = computed(() => {
  if (!runtime.value) return '';
  const prefix = runtime.value.command_prefix;
  return `${prefix} dashboard install --port ${runtime.value.port}\n${prefix} dashboard start\n${prefix} dashboard logs`;
});
onMounted(async () => {
  try {
    runtime.value = await props.api('/dashboard/runtime');
  } catch (e) {
    error.value = (e as Error).message;
  }
});
</script>
<template>
  <section class="hosting" aria-labelledby="hosting-title">
    <h2 id="hosting-title">Keep Selfhost running</h2>
    <p v-if="runtime?.supervised" role="status">
      This dashboard is running under its background service.
    </p>
    <p v-else-if="runtime?.installed">
      A background service is installed for this workspace. Check its status with
      <code>selfhost dashboard status</code>.
    </p>
    <p v-else>
      Install a user service to keep the dashboard running after you close the terminal and start it
      when you sign in.
    </p>
    <p v-if="error" role="alert">{{ error }}</p>
    <details v-if="runtime">
      <summary>Background service commands</summary>
      <p>
        Run these on the computer hosting Selfhost. Install a release CLI first, and stop its
        foreground dashboard before starting the service on the same port.
      </p>
      <pre tabindex="0">{{ commands }}</pre>
      <p>
        On Linux, enable user lingering to start without signing in. Windows and macOS user services
        start at sign-in. The private logs contain your local sign-in link.
      </p>
    </details>
    <h3>Use your own domain</h3>
    <p>
      Set your HTTPS Selfhost address in the sign-in setup below. Point your reverse proxy at
      <code>http://127.0.0.1:{{ runtime?.port || 'PORT' }}</code> on this host, and preserve the
      public Host header.
    </p>
    <p>
      A proxy in another server, container or LXC needs a secured tunnel to this loopback address.
      Its own localhost points to a different place.
    </p>
    <p>
      Configure DNS, TLS and the provider callback before applying the new address. Keep local
      recovery open while testing domain sign-in.
    </p>
  </section>
</template>
<style scoped>
.hosting {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 24px;
  margin: 24px 0;
}
h2 {
  font-size: 19px;
  margin: 0 0 12px;
}
h3 {
  margin-top: 24px;
}
p {
  color: var(--muted);
  line-height: 1.65;
}
summary {
  cursor: pointer;
  color: var(--green);
}
pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--bg);
  padding: 16px;
  line-height: 1.7;
}
code {
  overflow-wrap: anywhere;
}
</style>
