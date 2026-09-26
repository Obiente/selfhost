<script setup lang="ts">
import { ShieldCheck, RefreshCw } from 'lucide-vue-next';
defineProps<{
  providers: { id: string; name: string; login_url: string }[];
  loading: boolean;
  error: string;
  setup: boolean;
}>();
defineEmits<{ refresh: [] }>();
const local = ['localhost', '127.0.0.1', '[::1]'].includes(location.hostname);
</script>
<template>
  <main id="main-content" class="sign-in-page">
    <section class="sign-in-card" aria-labelledby="sign-in-title" :aria-busy="loading">
      <img src="/brand/selfhost-wordmark.svg" alt="selfhost" /><ShieldCheck
        class="sign-in-icon"
        :size="30"
        aria-hidden="true"
      />
      <h1 id="sign-in-title">{{ loading ? 'Opening Selfhost' : 'Sign in to Selfhost' }}</h1>
      <p v-if="error" class="monitor-error" role="alert">{{ error }}</p>
      <template v-if="providers.length"
        ><p>Continue with your identity provider.</p>
        <div class="provider-buttons">
          <a
            v-for="provider in providers"
            :key="provider.id"
            class="button primary"
            :href="provider.login_url"
            >Sign in with {{ provider.name }}</a
          >
        </div></template
      >
      <p v-else-if="!loading && !setup">
        No sign-in provider is connected yet. An administrator can connect one from Access after
        signing in locally.
      </p>
      <button v-if="!loading" class="text-button" @click="$emit('refresh')">
        <RefreshCw :size="15" />Refresh sign-in options
      </button>
      <div v-if="setup && !loading" class="sign-in-help">
        <p>
          Open the one-use setup link printed in the server terminal. It expires after 10 minutes;
          your setup session lasts 30 minutes. Restart the same command for a fresh link if needed.
        </p>
        <p>
          Use a trusted private network or encrypted VPN. After signing in, open Access to configure
          HTTPS and your identity provider, then restart without <code>--setup</code>.
        </p>
      </div>
      <details v-else-if="local && !loading" :open="!providers.length">
        <summary>Sign in with local recovery</summary>
        <p>
          Run <code>selfhost serve</code> and open the sign-in link shown in your terminal. The link
          works once and expires after 10 minutes.
        </p>
        <p>
          Once signed in, open Access to connect your identity provider. Linking a running
          identity-provider app alone does not grant users access to Selfhost.
        </p>
      </details>
      <p v-else-if="!providers.length && !loading" class="sign-in-help">
        Ask your administrator to configure sign-in from the computer running Selfhost.
      </p>
    </section>
  </main>
</template>
<style scoped>
.sign-in-page {
  min-height: 100dvh;
  display: grid;
  place-items: center;
  padding: 32px;
  background: var(--bg);
}
.sign-in-card {
  width: min(100%, 540px);
  padding: 36px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 14px;
  box-sizing: border-box;
}
.sign-in-card > img {
  display: block;
  width: 150px;
  margin-bottom: 40px;
}
.sign-in-icon {
  color: var(--green);
  margin-bottom: 16px;
}
h1 {
  font-size: 26px;
  margin: 0 0 16px;
}
p {
  color: var(--muted);
  line-height: 1.7;
}
.provider-buttons {
  display: grid;
  gap: 12px;
  margin: 24px 0;
}
.provider-buttons a {
  justify-content: center;
  text-align: center;
}
.text-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}
.sign-in-card details {
  border-top: 1px solid var(--border);
  margin-top: 24px;
  padding-top: 20px;
  font-size: 13px;
}
.sign-in-card summary {
  cursor: pointer;
  line-height: 1.6;
  padding: 4px 0;
}
.sign-in-card code {
  color: var(--green);
}
.sign-in-card a:focus-visible,
.sign-in-card button:focus-visible,
.sign-in-card summary:focus-visible {
  outline: 2px solid var(--green);
  outline-offset: 4px;
}
@media (max-width: 480px) {
  .sign-in-page {
    padding: 18px;
  }
  .sign-in-card {
    padding: 24px;
  }
}
</style>
