<script setup lang="ts">
import { ref } from 'vue';
const path = ref('new');
const paths = [
  {
    id: 'new',
    title: 'Start a new service',
    text: 'Set up an app from your terminal and keep its ordinary Compose files. Return to Selfhost whenever you want help with an update or integration.',
    link: '/standalone',
    cta: 'Use the standalone CLI',
  },
  {
    id: 'existing',
    title: 'Bring what you already run',
    text: 'Link a running app to see it alongside your projects. Start with read-only access and decide which actions Selfhost may run.',
    link: '/existing-apps',
    cta: 'Connect an existing app',
  },
  {
    id: 'custom',
    title: 'Use your own setup',
    text: 'Work with ordinary Compose files, environment variables and native application settings. Export them whenever you need to.',
    link: '/setups-and-databases',
    cta: 'Create a custom setup',
  },
];
</script>
<template>
  <div class="home-page">
    <section class="home-hero">
      <div>
        <img class="home-wordmark" src="/brand/selfhost-wordmark.svg" alt="selfhost" />
        <h1>A home for<br />everything you run.</h1>
        <p>
          Set up one app from your terminal, connect a service you already run, or manage your
          servers together. Choose when and how you use Selfhost.
        </p>
        <div class="home-actions">
          <a class="home-button primary" href="/getting-started"
            >Get started <span aria-hidden="true">↗</span></a
          ><a class="home-button" href="/catalogue">Browse the catalogue</a>
        </div>
        <p class="home-small">Open source. Ordinary configuration. Yours to keep.</p>
      </div>
      <div class="home-system" aria-label="Selfhost connects a workspace to your services">
        <div class="system-header">
          <img src="/brand/selfhost-monogram.svg" alt="" />
          <div><strong>Your services</strong><span>CLI, terminal interface or dashboard</span></div>
        </div>
        <div class="system-row">
          <span class="system-icon">01</span>
          <div><strong>Your services</strong><span>New deployments and existing apps</span></div>
          <span class="system-status">Connected</span>
        </div>
        <div class="system-row">
          <span class="system-icon">02</span>
          <div><strong>Your servers</strong><span>Local, remote, VM or container</span></div>
          <span class="system-status">Connected</span>
        </div>
        <div class="system-row">
          <span class="system-icon">03</span>
          <div><strong>Your identity</strong><span>Connect an existing provider</span></div>
          <span class="system-status">Connected</span>
        </div>
        <div class="system-footer">
          <code>compose.yaml</code><code>.env</code><code>config/</code
          ><span>Portable by design</span>
        </div>
      </div>
    </section>
    <section class="home-start" aria-labelledby="start-heading">
      <div class="section-intro">
        <h2 id="start-heading">Where do you want to begin?</h2>
        <p>A first app and an established homelab can start in different places.</p>
      </div>
      <div class="path-choices" role="group" aria-label="Choose your starting point">
        <button
          v-for="item in paths"
          :key="item.id"
          :aria-pressed="path === item.id"
          @click="path = item.id"
        >
          {{ item.title }}
        </button>
      </div>
      <article
        v-for="item in paths.filter((item) => item.id === path)"
        :key="item.id"
        class="path-detail"
        aria-live="polite"
      >
        <h3>{{ item.title }}</h3>
        <p>{{ item.text }}</p>
        <a :href="item.link">{{ item.cta }} <span aria-hidden="true">↗</span></a>
      </article>
    </section>
    <section class="home-principles" aria-label="How Selfhost works">
      <article>
        <span>Configuration</span>
        <h2>Simple when you want it.<br />Detailed when you need it.</h2>
        <p>
          Use guided setup for everyday choices. Open advanced settings for native app
          configuration, connections, custom actions and deployment details.
        </p>
        <a href="/app-integrations">Manage app settings ↗</a>
      </article>
      <article>
        <span>Control</span>
        <h2>Your infrastructure<br />stays yours.</h2>
        <p>
          Keep ordinary Compose, environment and configuration files. Your apps continue to run
          after Selfhost closes, and you can use their own tools whenever you choose.
        </p>
        <a href="/standalone">Run an app independently ↗</a>
      </article>
    </section>
    <section class="home-docs">
      <div>
        <h2>A guide for the next step.</h2>
        <p>Setup instructions, dashboard walkthroughs and command examples in one place.</p>
      </div>
      <a class="home-button primary" href="/getting-started">Read the documentation ↗</a>
    </section>
  </div>
</template>
