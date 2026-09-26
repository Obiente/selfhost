<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import AppIcon from '../../../ui/src/components/AppIcon.vue';
import catalogue from '../generated/catalogue.json';
import { cliCommands, cliMethod } from './cliMethod';
import GuideCode from './GuideCode.vue';
import GuideFields from './GuideFields.vue';
const props = defineProps<{ appId: string }>();
const app = computed<any>(() => catalogue.apps.find((app) => app.id === props.appId));
const selected = ref(app.value.deployments.findIndex((method: any) => method.default));
if (selected.value < 0) selected.value = 0;
onMounted(() => {
  const requested = new URLSearchParams(window.location.search).get('method');
  const index = app.value.deployments.findIndex((method: any) => method.id === requested);
  if (index >= 0) selected.value = index;
});
const mode = ref('cli');
const method = computed<any>(() => app.value.deployments[selected.value]);
const existingProfile = computed(() => method.value?.existingProfile || app.value.existingProfile);
const acknowledged = ref<string[]>([]);
const cli = computed(() => cliCommands[cliMethod.value]);
const prefix = computed(() => `${cli.value} app --directory ./${app.value.id}`);
const json = (value: any) => JSON.stringify(value, null, 2);
const inputs = computed(() =>
  Object.fromEntries(
    (method.value?.inputs || []).map((field: any) => [field.id, field.default ?? 'YOUR_VALUE']),
  ),
);
const requirementsReady = computed(() =>
  method.value?.requirements.every((item: any) => acknowledged.value.includes(item.id)),
);
const init = computed(
  () =>
    `${prefix.value} init ${app.value.id}${method.value?.stack ? ' --stack' : method.value?.id ? ` --method ${method.value.id}` : ''}${method.value?.inputs.length ? ' --inputs inputs.json' : ''}${(
      method.value?.requirements || []
    )
      .filter((item: any) => acknowledged.value.includes(item.id))
      .map((item: any) => ` --ack ${item.id}`)
      .join('')}`,
);
function setupRequest(service: any) {
  const mode = service.onboarding.modes.includes('bootstrap')
    ? 'bootstrap'
    : service.onboarding.modes[0];
  return json({
    mode,
    inputs: Object.fromEntries(
      service.onboarding.fields
        .filter((field: any) => field.modes.includes(mode))
        .map((field: any) => [
          field.id,
          field.kind === 'secret'
            ? `env:SELFHOST_${field.id.toUpperCase().replaceAll('-', '_')}`
            : (field.default ?? 'YOUR_VALUE'),
        ]),
    ),
    apps: [],
  });
}
function changeExample() {
  return json({ FIELD_ID: 'YOUR_VALUE' });
}
function existingActionCommand(action: any) {
  return `${cli.value} existing action EXISTING_ID ${action.id}${action.write ? ' --confirm "EXACT_APP_NAME"' : ''}`;
}
</script>
<template>
  <article class="app-guide">
    <a href="/catalogue">← All apps</a>
    <header>
      <AppIcon :src="app.icon" alt="" />
      <div>
        <h1>{{ app.name }}</h1>
        <p>{{ app.description }}</p>
      </div>
    </header>
    <p>
      Choose your deployment and the way you want to use Selfhost. The instructions below follow
      that choice.
    </p>
    <div class="guide-choices">
      <label v-if="app.deployments.length"
        >Deployment
        <select v-model="selected" @change="acknowledged = []">
          <option v-for="(choice, index) in app.deployments" :key="choice.id" :value="index">
            {{ choice.name }}
          </option>
        </select>
      </label>
      <label
        >Use Selfhost with
        <select v-model="mode">
          <option value="cli">Standalone CLI</option>
          <option value="dashboard">Dashboard</option>
        </select>
      </label>
    </div>
    <section v-if="method" aria-labelledby="deployment-title">
      <h2 id="deployment-title">Set up {{ app.name }}</h2>
      <p>{{ method.description }}</p>
      <ul v-if="app.guide.setup?.length">
        <li v-for="note in app.guide.setup" :key="note">{{ note }}</li>
      </ul>
      <details v-if="method.inputs.length">
        <summary>Customize deployment inputs ({{ method.inputs.length }})</summary>
        <GuideFields :fields="method.inputs" />
      </details>
      <fieldset v-if="method.requirements.length" class="requirements">
        <legend>Review this deployment’s requirements</legend>
        <label v-for="item in method.requirements" :key="item.id"
          ><input v-model="acknowledged" type="checkbox" :value="item.id" />{{ item.label }}</label
        >
        <p>
          Your selections add the matching CLI acknowledgements. The installer checks them again.
        </p>
      </fieldset>
      <template v-if="mode === 'cli'">
        <p v-if="method.inputs.length">
          Save this as <code>inputs.json</code> and adjust the values before initialization.
        </p>
        <GuideCode v-if="method.inputs.length" :text="json(inputs)" />
        <p v-if="!requirementsReady" class="notice">
          Review and acknowledge every requirement above before running initialization.
        </p>
        <GuideCode :text="init" />
        <p>
          Initialization writes portable files and does not start the app. Review
          <code>compose.yaml</code>, the private <code>.env</code> and any files in
          <code>files/</code>, then start it:
        </p>
        <GuideCode :text="`${prefix} start`" />
        <p>
          No Selfhost dashboard or background service is needed. You can also start these files
          directly:
        </p>
        <GuideCode :text="`cd ${app.id}\ndocker compose -f compose.yaml up -d`" />
      </template>
      <ol v-else>
        <li>
          Open the app catalogue in your Selfhost dashboard and select
          <strong>{{ app.name }}</strong
          ><template v-if="app.kind === 'stack'"> from the available stacks</template>.
        </li>
        <li>
          Choose <strong>{{ method.name }}</strong
          >, your target server and project name. Adjust the declared inputs and review any
          requirements.
        </li>
        <li>
          Create the project, review its configuration, then start it. Open its service to continue
          setup.
        </li>
      </ol>
      <ul v-if="method.instructions.length">
        <li v-for="note in method.instructions" :key="note">{{ note }}</li>
      </ul>
    </section>
    <section v-if="existingProfile">
      <h2>Connect an existing installation</h2>
      <p>
        Use <strong>Existing apps → Link an app</strong> in the dashboard, or the CLI’s existing-app
        commands. Choose {{ existingProfile.name }}, its URL and, optionally, its exact Docker
        container on a connected server.
      </p>
      <template v-if="mode === 'cli'">
        <p>
          Save <code>existing.json</code> with your app URL. Add <code>server_id</code> and
          <code>container</code> for container inspection and actions.
        </p>
        <GuideCode
          :text="
            json({ profile: existingProfile.id, name: app.name, url: 'https://app.example.com' })
          "
        />
        <GuideCode
          :text="`${cli} existing link --file existing.json\n${cli} existing list\n${cli} existing inspect EXISTING_ID`"
        />
      </template>
      <p>
        Linking enables the existing-app profile’s supported inspection and actions. It does not
        import Compose or attach the new-deployment settings and onboarding profiles described
        below.
      </p>
      <a href="/existing-apps">Existing app commands, permissions and unlinking</a>
      <details v-if="existingProfile.actions.length">
        <summary>Actions for a linked container</summary>
        <div v-for="action in existingProfile.actions" :key="action.id">
          <h3>{{ action.label }}</h3>
          <p>{{ action.description }}</p>
          <p v-if="action.write">
            Enable this action explicitly in management permissions on a writable server. Running it
            also requires the exact saved app name.
          </p>
          <GuideCode v-if="mode === 'cli'" :text="existingActionCommand(action)" />
        </div>
      </details>
    </section>
    <section v-if="app.guide.integration_notes?.length || app.guide.limitations?.length">
      <h2>Before configuring integrations</h2>
      <ul>
        <li
          v-for="note in [...(app.guide.integration_notes || []), ...(app.guide.limitations || [])]"
          :key="note"
        >
          {{ note }}
        </li>
      </ul>
    </section>
    <section v-if="method && !method.services.length">
      <h2>Integrations for this deployment</h2>
      <p>
        This deployment has no attached native settings or onboarding profile. Follow its setup
        instructions above and use the app’s own administration interface. Choosing another
        deployment can expose different Selfhost integrations.
      </p>
    </section>
    <section v-for="service in method?.services || []" :key="service.id" class="service-guide">
      <h2>{{ service.name }} integrations</h2>
      <p>
        Compose service: <code>{{ service.id }}</code
        >. These operations use the profile saved when this deployment is created.
      </p>
      <ul v-if="service.warnings.length" class="notice">
        <li v-for="warning in service.warnings" :key="warning">{{ warning }}</li>
      </ul>
      <section v-if="service.onboarding">
        <h3>Initialize and connect</h3>
        <p>
          Supported setup modes: <strong>{{ service.onboarding.modes.join(', ') }}</strong
          >.
        </p>
        <ul>
          <li v-for="warning in service.onboarding.warnings" :key="warning">{{ warning }}</li>
        </ul>
        <details>
          <summary>Setup fields</summary>
          <GuideFields :fields="service.onboarding.fields" />
        </details>
        <template v-if="mode === 'cli'">
          <p>
            Save <code>onboarding.json</code>, replace example values and supply
            <code>env:</code> secrets privately through your shell. Empty <code>apps</code> adds no
            links; add explicit links when this profile supports them.
          </p>
          <GuideCode :text="setupRequest(service)" />
          <GuideCode
            :text="`${prefix} setup ${service.id}\n${prefix} setup-plan ${service.id} onboarding.json\n${prefix} setup-apply ${service.id} onboarding.json --revision REVIEWED_REVISION`"
          />
        </template>
        <p v-else>
          Start the service, open its setup section, select an available mode, fill in the fields
          and choose <strong>Review setup</strong>. Check the operations before applying.
        </p>
        <p><a href="/app-onboarding">Setup modes, secret references and recovery</a></p>
        <template v-if="service.onboarding.modes.includes('sync')">
          <h4>Keep service links current</h4>
          <p v-if="mode === 'dashboard'">
            After connecting the app, create a link-sync task, review its service selection and
            enable its schedule or project-change trigger. Scheduled work requires Selfhost to stay
            running.
          </p>
          <template v-else
            ><p>
              Save your explicit new links as <code>links.json</code>, then review and sync whenever
              you need to. No scheduler is required.
            </p>
            <GuideCode
              :text="`${prefix} sync-plan ${service.id} links.json\n${prefix} sync ${service.id} links.json --revision REVIEWED_REVISION`"
          /></template>
          <a href="/tasks-and-triggers">Tasks, triggers and link examples</a>
        </template>
      </section>
      <section v-if="service.settings.length">
        <h3>Native settings</h3>
        <p>
          Changes use the app’s
          {{
            service.driver === 'compose_environment'
              ? 'Compose environment'
              : service.driver === 'yaml'
                ? 'native YAML file'
                : 'configuration commands'
          }}. Unrelated settings are preserved.
        </p>
        <details>
          <summary>Supported fields ({{ service.settings.length }})</summary>
          <GuideFields :fields="service.settings" />
        </details>
        <template v-if="mode === 'cli'">
          <p>
            Save a JSON map of the field IDs you want to change as <code>changes.json</code>.
            Replace <code>FIELD_ID</code> with an ID from the supported fields above, and use its
            declared value type. Review the plan and replace <code>REVIEWED_REVISION</code> with the
            revision it returns.
          </p>
          <GuideCode :text="changeExample()" />
          <GuideCode
            :text="`${prefix} config ${service.id}\n${prefix} plan ${service.id} changes.json\n${prefix} apply ${service.id} changes.json --revision REVIEWED_REVISION`"
          />
        </template>
        <p v-else>
          Open the service’s <strong>App settings</strong>. Use <strong>Advanced</strong> for
          additional fields. Edit, preview and apply the reviewed changes.
        </p>
        <p v-if="service.driver !== 'command'">
          Saved changes need service recreation to become active. Existing interpolated environment
          values stay under your control in <code>.env</code> or Compose.
        </p>
      </section>
      <section v-if="service.oidc">
        <h3>Identity provider login</h3>
        <p>
          Selfhost can register a client with a supported provider and configure this app. Callback
          path: <code>{{ service.oidc.callback }}</code
          >.
        </p>
        <p v-if="service.oidc.administratorRole">
          This profile can explicitly map a verified human account to
          {{ service.oidc.administratorRole }} after review.
        </p>
        <p v-else>
          No administrator role is assigned by Selfhost. Check the app’s first-login policy and
          retain a local recovery account.
        </p>
        <template v-if="mode === 'cli'">
          <p>
            Save <code>connection.json</code> and replace the URLs. Supply
            <code>SELFHOST_IDP_TOKEN</code> privately in your shell.
          </p>
          <GuideCode
            :text="
              json({
                provider: 'zitadel',
                issuer: 'https://identity.example.com',
                app_url: 'https://app.example.com',
                name: app.name,
              })
            "
          />
          <GuideCode
            :text="`${prefix} connect-account ${service.id} connection.json\n${prefix} connect-plan ${service.id} connection.json\n${prefix} connect ${service.id} connection.json --revision REVIEWED_REVISION`"
          />
        </template>
        <p v-else>
          Open the service’s identity connection, select a supported provider, enter the provider
          and app URLs, and provide a temporary provider token. Review client creation and app
          changes before applying.
        </p>
        <p>
          Test sign-in in a separate browser session. HTTPS domains and HTTP loopback development
          origins are supported by Selfhost; the chosen app and provider must also accept the
          resulting callback.
          <a href="/app-integrations#guided-identity-connections"
            >Provider permissions, localhost and recovery</a
          >.
        </p>
      </section>
      <section v-if="service.database">
        <h3>Database placement</h3>
        <p>
          This recipe supports {{ service.database.join(', ') }}. Workspace projects can choose a
          dedicated database, a shared source with a separate database and account, or an existing
          external database before their first start.
        </p>
        <ul v-if="Object.keys(service.databaseTls).length">
          <li v-for="(modes, engine) in service.databaseTls" :key="engine">
            {{ engine }} adapter TLS modes: {{ (modes as string[]).join(', ') }}. Use a private
            network for connections with TLS disabled.
          </li>
        </ul>
        <p v-if="mode === 'dashboard'">
          Open project setup settings and select the database placement. Configure a source under
          <strong>Databases</strong> if needed.
        </p>
        <p v-else>
          Standalone directories use the generated Compose and environment files directly. Configure
          the database there before first start. Selfhost’s source provisioning commands belong to
          workspace projects and are optional.
        </p>
        <a href="/setups-and-databases">Database setup, CLI commands and backups</a>
      </section>
      <section v-if="service.actions.length">
        <h3>App actions</h3>
        <p v-if="mode === 'dashboard'">
          Open the service’s actions or App settings, select a workflow and review its inputs before
          running it.
        </p>
        <details v-for="action in service.actions" :key="`${action.kind}-${action.id}`">
          <summary>{{ action.label }}<span v-if="action.schedulable"> · schedulable</span></summary>
          <p>{{ action.description }}</p>
          <GuideFields v-if="action.inputs.length" :fields="action.inputs" />
          <template v-if="mode === 'cli'">
            <p v-if="action.inputs.length">
              Save the input IDs and selected values in a private
              <code>action-inputs.json</code> file.
            </p>
            <GuideCode
              :text="`${prefix} action ${service.id} ${action.id}${action.inputs.length ? ' --inputs action-inputs.json' : ''}`"
            />
          </template>
          <p v-if="action.schedulable">
            A workspace can schedule this workflow while Selfhost runs. A standalone user can invoke
            it through an external scheduler. <a href="/tasks-and-triggers">Scheduling guide</a>.
          </p>
        </details>
      </section>
      <p
        v-if="
          !service.settings.length &&
          !service.actions.length &&
          !service.onboarding &&
          !service.oidc &&
          !service.database
        "
      >
        No additional native integrations are attached to this service. Use its own administration
        tools after deployment.
      </p>
      <p v-if="service.documentation">
        <a :href="service.documentation">App configuration reference</a>
      </p>
    </section>
    <section v-if="method?.images?.length">
      <h2>Versions and updates</h2>
      <p>
        This deployment uses the following images. Existing projects retain their saved recipe and
        images. A new catalogue version does not silently update them.
      </p>
      <ul>
        <li v-for="item in method.images" :key="item.id">
          <strong>{{ item.id }}</strong
          >: <code>{{ item.image }}</code>
        </li>
      </ul>
      <ul
        v-if="
          app.testedVersions.length &&
          method.services.some((service: any) => service.recipe === app.id)
        "
      >
        <li v-for="version in app.testedVersions" :key="version.image">
          <strong>{{ version.label }}</strong> <code>{{ version.image }}</code>
          <p>{{ version.notes }}</p>
        </li>
      </ul>
      <p>
        Review compatibility and back up app data before an update.
        <a href="/catalog-maintenance#choose-an-app-version"
          >Choose an image version and review the change</a
        >.
      </p>
    </section>
    <section>
      <h2>Keep control of your setup</h2>
      <p>
        Selfhost remains optional. Your app runs using ordinary Compose files, its own settings and
        persistent data. Keep the Compose project name and volumes to preserve storage.
        Configuration exports and setting backups do not include application data.
      </p>
      <p>
        <a href="/standalone">Standalone CLI guide</a> · <a href="/removal">Backups and removal</a
        ><template v-if="app.documentation">
          · <a :href="app.documentation">{{ app.name }} documentation</a></template
        >
      </p>
    </section>
  </article>
</template>
<style scoped>
header {
  display: flex;
  align-items: center;
  gap: 20px;
  margin: 24px 0;
}
header img {
  width: 56px;
  height: 56px;
  object-fit: contain;
}
header h1 {
  margin: 0;
}
header p {
  margin-bottom: 0;
}
.guide-choices {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  margin: 24px 0;
  padding: 20px;
  background: var(--vp-c-bg-soft);
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
}
.guide-choices label {
  font-weight: 600;
  font-size: 14px;
}
select {
  display: block;
  width: 100%;
  margin-top: 8px;
  padding: 10px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 5px;
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font: inherit;
}
details {
  margin: 12px 0;
  padding: 12px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 6px;
}
summary {
  cursor: pointer;
  font-weight: 600;
}
summary span {
  font-size: 12px;
  font-weight: normal;
}
.requirements {
  border: 1px solid var(--vp-c-brand-1);
  padding: 12px 16px;
  margin: 16px 0;
  border-radius: 6px;
}
.requirements label {
  display: flex;
  align-items: start;
  gap: 10px;
  margin: 12px 0;
  font-size: 14px;
}
.requirements input {
  margin-top: 6px;
  flex: 0 0 auto;
}
.notice {
  border-left: 3px solid var(--vp-c-brand-1);
  padding-left: 16px;
}
select:focus-visible,
summary:focus-visible,
input:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 3px;
}
code {
  overflow-wrap: anywhere;
}
@media (max-width: 600px) {
  .guide-choices {
    grid-template-columns: 1fr;
  }
  header {
    align-items: start;
  }
}
</style>
