<script setup lang="ts">
import { computed, ref } from 'vue';
import AppIcon from '../../../ui/src/components/AppIcon.vue';
import catalogue from '../generated/catalogue.json';
import CliMethodSelector from './CliMethodSelector.vue';
import { cliCommands, cliMethod } from './cliMethod';
const query = ref(''),
  category = ref(''),
  capabilities = ref<string[]>([]);
const deployment = ref<Record<string, string>>({});
function chosen(app: any) {
  return (
    app.deployments.find((item: any) => item.id === deployment.value[app.id]) ||
    app.deployments.find((item: any) => item.default) ||
    app.deployments[0]
  );
}
function guideUrl(app: any) {
  const method = chosen(app);
  return app.guideUrl + (method?.id ? `?method=${encodeURIComponent(method.id)}` : '');
}
function methodFields(app: any, field: string) {
  return (chosen(app)?.services || []).flatMap((service: any) => service[field] || []);
}
function methodSetupModes(app: any) {
  return [
    ...new Set<string>(
      (chosen(app)?.services || []).flatMap((service: any) => service.onboarding?.modes || []),
    ),
  ];
}
const categories = [...new Set(catalogue.apps.map((app) => app.category))].sort();
const labels = catalogue.capabilities as Record<string, string>;
const available = Object.keys(labels).filter((key) =>
  catalogue.apps.some((app) => app.capabilities.includes(key)),
);
const results = computed(() =>
  catalogue.apps.filter((app) => {
    const words = query.value.toLowerCase().trim().split(/\s+/).filter(Boolean);
    const text = [
      app.name,
      app.id,
      app.description,
      app.category,
      ...app.capabilities.map((key) => labels[key]),
    ]
      .join(' ')
      .toLowerCase();
    return (
      (!category.value || app.category === category.value) &&
      capabilities.value.every((key) => app.capabilities.includes(key)) &&
      words.every((word) => text.includes(word))
    );
  }),
);
function reset() {
  query.value = '';
  category.value = '';
  capabilities.value = [];
}
function command(app: any) {
  const method = chosen(app);
  return `${cliCommands[cliMethod.value]} app --directory ./${app.id} init ${app.id}${method?.stack ? ' --stack' : method?.id ? ` --method ${method.id}` : ''}`;
}
</script>
<template>
  <main class="catalogue-page">
    <header class="catalogue-header">
      <div>
        <h1>Find your next service.</h1>
        <p>
          Explore the full Selfhost catalogue. Choose the setup and integrations that fit the way
          you run your apps.
        </p>
      </div>
      <a href="/contributing" class="home-button">Add an app <span aria-hidden="true">↗</span></a>
    </header>
    <section class="catalogue-filters" aria-label="Filter apps">
      <div class="catalogue-search">
        <label
          >Search apps<input
            v-model="query"
            type="search"
            placeholder="An app, a category, or a capability" /></label
        ><label
          >Category<select v-model="category">
            <option value="">All categories</option>
            <option v-for="value in categories" :key="value">{{ value }}</option>
          </select></label
        >
      </div>
      <fieldset>
        <legend>Selfhost support</legend>
        <div class="capability-filters">
          <label
            v-for="key in available"
            :key="key"
            :class="{ selected: capabilities.includes(key) }"
            ><input v-model="capabilities" :value="key" type="checkbox" />{{ labels[key] }}</label
          >
        </div>
      </fieldset>
      <div class="catalogue-count">
        <p aria-live="polite">
          {{ results.length }} of {{ catalogue.apps.length }} apps and stacks
        </p>
        <button v-if="query || category || capabilities.length" @click="reset">
          Clear filters
        </button>
      </div>
    </section>
    <details class="catalogue-legend">
      <summary>What the support tags mean</summary>
      <p>
        Each tag comes from an included Selfhost recipe or integration. Guided setup initializes an
        app through its API. Native settings edits supported configuration fields. OIDC connection
        creates a provider client when a supported provider is used. App actions run declared
        maintenance commands. Automatic link sync can add new services to a connected dashboard.
      </p>
      <p>
        A recipe can offer some of these features without offering all of them. Tested-version notes
        describe the checks performed for a specific image; they are separate from feature
        availability. You can also choose a custom version during deployment and review its
        compatibility yourself.
      </p>
    </details>
    <CliMethodSelector />
    <div v-if="results.length" class="catalogue-cards">
      <article v-for="app in results" :id="app.id" :key="app.id" class="catalogue-card">
        <div class="catalogue-card-heading">
          <AppIcon :src="app.icon" alt="" loading="lazy" />
          <div>
            <h2>{{ app.name }}</h2>
            <span>{{ app.category }}</span>
          </div>
        </div>
        <p class="catalogue-description">{{ app.description }}</p>
        <label v-if="app.deployments.length > 1" class="card-deployment"
          >Deployment
          <select
            :value="chosen(app)?.id"
            :aria-label="`${app.name} deployment`"
            @change="deployment[app.id] = ($event.target as HTMLSelectElement).value"
          >
            <option v-for="method in app.deployments" :key="method.id" :value="method.id">
              {{ method.name }}
            </option>
          </select>
        </label>
        <ul class="capability-tags" :aria-label="`${app.name} support`">
          <li v-for="key in app.capabilities" :key="key">{{ labels[key] }}</li>
        </ul>
        <p v-if="app.deployments.length > 1" class="form-help">
          Support tags cover available deployments. The setup guide shows support for your selected
          method.
        </p>
        <details class="app-capabilities">
          <summary>Deployment & integration details</summary>
          <template v-if="app.kind !== 'connection'"
            ><p>Generate editable Compose files, then start the app when you are ready.</p>
            <pre
              v-if="!chosen(app)?.requirements.length"
              tabindex="0"
            ><code>{{command(app)}}</code></pre>
            <p v-else>
              Review this deployment’s requirements in its setup guide before installing.
            </p>
          </template>
          <p v-for="image in chosen(app)?.images || []" :key="image.id">
            <strong>{{ image.id }} image</strong
            ><code class="image-reference">{{ image.image }}</code>
          </p>
          <div v-if="app.deployments.length">
            <h3>Deployment choices</h3>
            <ul>
              <li v-for="method in app.deployments" :key="method.id">
                <strong>{{ method.name }}</strong>
                <p>{{ method.description }}</p>
              </li>
            </ul>
          </div>
          <div v-if="methodSetupModes(app).length">
            <h3>Guided setup</h3>
            <p>
              {{
                methodSetupModes(app)
                  .map(
                    (mode) =>
                      ({
                        bootstrap: 'Initialize a new installation',
                        connect: 'Connect an existing installation',
                        sync: 'Add service links',
                      })[mode] || mode,
                  )
                  .join(' · ')
              }}
            </p>
            <a href="/app-onboarding">App setup guide</a>
          </div>
          <div v-if="methodFields(app, 'settings').length">
            <h3>Native settings</h3>
            <ul>
              <li v-for="field in methodFields(app, 'settings')" :key="field.label">
                {{ field.label }}<small v-if="field.advanced"> · Advanced</small>
              </li>
            </ul>
          </div>
          <div v-if="methodFields(app, 'actions').length">
            <h3>App actions</h3>
            <ul>
              <li v-for="action in methodFields(app, 'actions')" :key="action.id">
                {{ action.label }}<small v-if="action.confirmation"> · Confirmation required</small>
              </li>
            </ul>
          </div>
          <div
            v-if="
              app.testedVersions.length &&
              chosen(app)?.images.some((image: any) => image.image === app.image)
            "
          >
            <h3>Tested versions</h3>
            <ul>
              <li v-for="version in app.testedVersions" :key="version.image">
                <strong>{{ version.label }}</strong
                ><code class="image-reference">{{ version.image }}</code>
                <p>{{ version.notes }}</p>
              </li>
            </ul>
          </div>
          <p v-if="app.capabilities.includes('oidc')">
            <a href="/app-integrations#guided-identity-connections">Identity connection guide</a>
          </p>
          <p v-if="app.capabilities.includes('existing')">
            <a href="/existing-apps">Connect an existing installation</a>
          </p>
          <p v-if="app.capabilities.includes('triggers')">
            <a href="/tasks-and-triggers">Keep dashboard links up to date</a>
          </p>
        </details>
        <div class="catalogue-card-links">
          <a :href="guideUrl(app)">Setup & integration guide <span aria-hidden="true">↗</span></a
          ><a
            v-if="app.documentation"
            :href="app.documentation"
            target="_blank"
            rel="noopener noreferrer"
            >App docs <span class="sr-only">(opens a new tab)</span></a
          >
        </div>
      </article>
    </div>
    <section v-else class="catalogue-empty">
      <h2>No apps match these filters.</h2>
      <p>Try a different search or fewer support tags.</p>
      <button class="home-button" @click="reset">Show all apps</button>
    </section>
    <footer class="catalogue-footer">
      <h2>Your setup can stay yours.</h2>
      <p>
        Use Selfhost once, return for an update, or keep managing everything from the dashboard. The
        apps run from ordinary Compose files and their own configuration.
      </p>
      <a href="/standalone">Explore the standalone CLI →</a>
    </footer>
  </main>
</template>
<style scoped>
.card-deployment {
  display: block;
  margin: 12px 0;
  font-size: 13px;
}
.card-deployment select {
  display: block;
  width: 100%;
  padding: 8px;
  margin-top: 6px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 5px;
  background: var(--vp-c-bg);
}
.card-deployment select:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 2px;
}
.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
.catalogue-page {
  max-width: 1280px;
  margin: auto;
  padding: 64px 32px;
}
.catalogue-header {
  display: flex;
  justify-content: space-between;
  gap: 32px;
  align-items: center;
  margin-bottom: 40px;
}
.catalogue-header h1 {
  font-size: clamp(32px, 4vw, 52px);
  letter-spacing: -1.8px;
  line-height: 1.15;
  margin: 0 0 20px;
  font-weight: 650;
}
.catalogue-header p {
  max-width: 700px;
  font-size: 18px;
  line-height: 1.7;
  color: var(--vp-c-text-2);
}
.catalogue-header a {
  flex-shrink: 0;
}
.catalogue-filters {
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  padding: 24px;
  background: var(--vp-c-bg-alt);
}
.catalogue-search {
  display: grid;
  grid-template-columns: 2fr 1fr;
  gap: 24px;
}
.catalogue-search label {
  display: grid;
  gap: 10px;
  font-size: 14px;
  font-weight: 600;
}
.catalogue-search input,
.catalogue-search select {
  width: 100%;
  border: 1px solid var(--vp-c-divider);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  padding: 12px 14px;
  border-radius: 6px;
  font: inherit;
}
.catalogue-search input:focus-visible,
.catalogue-search select:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
fieldset {
  border: 0;
  padding: 0;
  margin: 24px 0 0;
}
legend {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 12px;
}
.capability-filters {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.capability-filters label {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 7px;
  cursor: pointer;
  font-size: 13px;
}
.capability-filters label.selected {
  border-color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}
.capability-filters input {
  accent-color: var(--vp-c-brand-1);
}
.catalogue-count {
  display: flex;
  justify-content: space-between;
  margin-top: 20px;
  font-size: 13px;
  color: var(--vp-c-text-2);
}
button,
a {
  touch-action: manipulation;
}
.catalogue-count button {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
}
.catalogue-legend {
  margin: 20px 0 28px;
  font-size: 14px;
  color: var(--vp-c-text-2);
}
summary {
  cursor: pointer;
  line-height: 1.7;
}
summary:focus-visible,
button:focus-visible,
a:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 4px;
}
.catalogue-legend p {
  line-height: 1.7;
  margin-top: 12px;
  max-width: 1000px;
}
.catalogue-cards {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 20px;
  margin: 28px 0;
}
.catalogue-card {
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  padding: 24px;
  display: flex;
  flex-direction: column;
  align-self: start;
  background: var(--vp-c-bg-alt);
  scroll-margin-top: 90px;
}
.catalogue-card-heading {
  display: flex;
  gap: 15px;
  align-items: center;
}
.catalogue-card-heading img {
  width: 42px;
  height: 42px;
  object-fit: contain;
}
.catalogue-card h2 {
  font-size: 21px;
  margin: 0 0 4px;
  font-weight: 600;
  letter-spacing: -0.5px;
}
.catalogue-card-heading span {
  font-size: 12px;
  color: var(--vp-c-text-3);
}
.catalogue-description {
  font-size: 14px;
  line-height: 1.65;
  color: var(--vp-c-text-2);
  margin: 18px 0;
}
.capability-tags {
  display: flex;
  gap: 7px;
  flex-wrap: wrap;
  list-style: none;
  padding: 0;
  margin: 0 0 20px;
}
.capability-tags li {
  font-size: 11px;
  line-height: 1.45;
  padding: 5px 8px;
  border: 1px solid #56604f;
  border-radius: 5px;
  color: var(--vp-c-brand-2);
}
.app-capabilities {
  border-top: 1px solid var(--vp-c-divider);
  padding-top: 16px;
  font-size: 13px;
}
.app-capabilities p {
  line-height: 1.65;
  margin: 12px 0;
  color: var(--vp-c-text-2);
}
.app-capabilities h3 {
  font-size: 14px;
  font-weight: 600;
  margin: 20px 0 8px;
}
.app-capabilities ul {
  padding-left: 18px;
  list-style: disc;
}
.app-capabilities li {
  line-height: 1.65;
  margin: 6px 0;
}
.app-capabilities a {
  color: var(--vp-c-brand-1);
  text-decoration: underline;
}
.app-capabilities pre {
  background: var(--vp-c-bg);
  padding: 12px;
  border-radius: 6px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  margin: 14px 0;
}
.image-reference {
  display: block;
  font-size: 12px;
  overflow-wrap: anywhere;
  margin: 6px 0;
}
.catalogue-card-links {
  display: flex;
  gap: 12px;
  justify-content: space-between;
  flex-wrap: wrap;
  margin-top: 22px;
  font-size: 13px;
  font-weight: 600;
  color: var(--vp-c-brand-1);
}
.catalogue-card-links a:hover {
  text-decoration: underline;
}
.catalogue-empty {
  text-align: center;
  padding: 70px 20px;
}
.catalogue-empty p {
  margin: 16px 0 25px;
  color: var(--vp-c-text-2);
}
.catalogue-footer {
  border-top: 1px solid var(--vp-c-divider);
  padding: 48px 0;
  margin-top: 56px;
}
.catalogue-footer h2 {
  font-size: 28px;
  letter-spacing: -0.7px;
}
.catalogue-footer p {
  max-width: 750px;
  line-height: 1.75;
  color: var(--vp-c-text-2);
  margin: 18px 0;
}
.catalogue-footer a {
  color: var(--vp-c-brand-1);
}
@media (max-width: 1020px) {
  .catalogue-cards {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .catalogue-header {
    align-items: start;
    flex-direction: column;
  }
}
@media (max-width: 640px) {
  .catalogue-page {
    padding: 40px 20px;
  }
  .catalogue-cards,
  .catalogue-search {
    grid-template-columns: 1fr;
  }
  .catalogue-filters {
    padding: 18px;
  }
  .catalogue-card {
    padding: 20px;
  }
}
</style>
