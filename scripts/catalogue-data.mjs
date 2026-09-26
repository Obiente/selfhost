import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { parse } from 'smol-toml';

export const capabilityLabels = {
  deploy: 'Compose deployment',
  stack: 'Multi-service stack',
  existing: 'Connect existing',
  onboarding: 'Guided setup',
  settings: 'Native settings',
  oidc: 'OIDC connection',
  provider: 'Identity provider',
  actions: 'App actions',
  scheduled: 'Scheduled actions',
  database: 'Database options',
  links: 'Dashboard links',
  triggers: 'Automatic link sync',
  methods: 'Deployment choices',
  versions: 'Tested version choices',
};
function jsonFiles(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => [name.slice(0, -5), JSON.parse(readFileSync(join(directory, name), 'utf8'))]);
}
export function buildCatalogue(root) {
  const path = join(root, 'catalog');
  const sidecar = (folder) => new Map(jsonFiles(join(path, folder)));
  const integrations = sidecar('integrations'),
    onboarding = sidecar('onboarding'),
    versions = sidecar('versions'),
    existing = sidecar('existing');
  const methods = jsonFiles(join(path, 'deployments')).flatMap(([, items]) => items);
  const cards = [];
  const recipes = new Map();
  for (const file of readdirSync(path)
    .filter((name) => name.endsWith('.toml'))
    .sort()) {
    const app = parse(readFileSync(join(path, file), 'utf8'));
    recipes.set(app.id, app);
    if (!/^[a-z0-9-]+$/.test(app.id)) throw Error('Invalid catalogue app ID');
    const integration = integrations.get(app.id) || app.integration;
    const setup = onboarding.get(app.id) || app.onboarding;
    const tested = (versions.get(app.id)?.tested || []).map(({ image, label, notes }) => ({
      image,
      label,
      notes,
    }));
    const capabilities = ['deploy'];
    if (existing.has(app.id)) capabilities.push('existing');
    if (setup) capabilities.push('onboarding');
    if (integration?.fields?.length) capabilities.push('settings');
    if (integration?.oidc_client) capabilities.push('oidc');
    if (integration?.oidc_provider) capabilities.push('provider');
    const actions = [
      ...(app.actions || []).map((action) => ({
        id: action.id,
        label: action.label,
        confirmation: !!action.confirm,
      })),
      ...(integration?.actions || []).map((action) => ({
        id: action.id,
        label: action.label,
        confirmation: true,
      })),
    ];
    if (actions.length) capabilities.push('actions');
    if (integration?.actions?.some((action) => action.schedulable)) capabilities.push('scheduled');
    if (app.database) capabilities.push('database');
    if (app.dashboard || setup?.steps?.some((step) => step.each_app)) capabilities.push('links');
    if (setup?.steps?.some((step) => step.modes.includes('sync'))) capabilities.push('triggers');
    const deployments = methods
      .filter((method) => method.app === app.id)
      .map((method) => ({ id: method.id, name: method.name, description: method.description }));
    if (deployments.length > 1) capabilities.push('methods');
    if (tested.length > 1) capabilities.push('versions');
    cards.push({
      id: app.id,
      name: app.name,
      description: app.description,
      category: app.category,
      kind: 'app',
      image: app.image,
      icon: app.icon,
      documentation: app.docs,
      capabilities,
      actions,
      settings: (integration?.fields || []).map((field) => ({
        label: field.label,
        advanced: !!field.advanced,
      })),
      setupModes: [...new Set((setup?.steps || []).flatMap((step) => step.modes))],
      testedVersions: tested,
      deployments,
    });
  }
  for (const [id, stack] of jsonFiles(join(path, 'stacks'))) {
    if (cards.some((card) => card.id === id)) continue;
    const parent = methods.find((method) => method.blueprint === id)?.app;
    const capabilities = ['stack'];
    if (stack.category === 'Identity') capabilities.push('provider');
    if (existing.has(id)) capabilities.push('existing');
    const actions = (existing.get(id)?.actions || []).map((action) => ({
      id: action.id,
      label: action.label,
      confirmation: !!action.write,
    }));
    if (actions.length) capabilities.push('actions');
    cards.push({
      id,
      name: stack.name,
      description: stack.description,
      category: stack.category,
      kind: 'stack',
      image: '',
      icon:
        stack.icon ||
        cards.find((card) => card.id === parent)?.icon ||
        '/brand/selfhost-monogram.svg',
      documentation: '',
      capabilities,
      actions,
      settings: [],
      setupModes: [],
      testedVersions: [],
      deployments: [],
      parent: parent || null,
    });
  }
  for (const [id, profile] of existing) {
    if (cards.some((card) => card.id === id) || !profile.image_repositories?.length) continue;
    const actions = (profile.actions || []).map((action) => ({
      id: action.id,
      label: action.label,
      confirmation: !!action.write,
    }));
    cards.push({
      id,
      name: profile.name,
      description: profile.description,
      category: 'Existing services',
      kind: 'connection',
      image: '',
      icon: '/brand/selfhost-monogram.svg',
      documentation: '',
      capabilities: ['existing', ...(actions.length ? ['actions'] : [])],
      actions,
      settings: [],
      setupModes: [],
      testedVersions: [],
      deployments: [],
    });
  }
  const stacks = sidecar('stacks');
  const guides = sidecar('guides');
  const existingGuide = (id) => {
    const profile = existing.get(id);
    return profile
      ? {
          id,
          name: profile.name,
          actions: (profile.actions || []).map(({ id, label, description, write }) => ({
            id,
            label,
            description,
            write: !!write,
          })),
        }
      : null;
  };
  // Publish only documentation metadata, never recipe environment, commands or secrets.
  const field = (item, id = item.id) => ({
    id,
    label: item.label,
    kind: item.kind,
    description: item.description || '',
    advanced: !!item.advanced,
    required: !!item.required,
    choices: item.choices || [],
    minimum: item.minimum,
    maximum: item.maximum,
    minimum_length: item.minimum_length,
    maximum_length: item.maximum_length,
    modes: item.modes || [],
    ...(item.kind !== 'secret' && item.default !== undefined ? { default: item.default } : {}),
  });
  const serviceGuide = (id, recipeId, profileId) => {
    const recipe = recipes.get(recipeId);
    const profile = integrations.get(profileId || recipeId) || recipe?.integration;
    const setup = recipe ? onboarding.get(recipeId) || recipe.onboarding : null;
    return {
      id,
      recipe: recipeId || null,
      name: recipe?.name || profile?.name || id,
      driver: profile?.driver?.kind || null,
      settings: (profile?.fields || []).map((item) => field(item)),
      warnings: profile?.warnings || [],
      documentation: profile?.documentation || recipe?.docs || '',
      actions: [
        ...(recipe?.actions || []).map((action) => ({
          id: action.id,
          label: action.label,
          description: action.description,
          kind: 'recipe',
          confirmation: !!action.confirm,
          inputs: [],
          schedulable: false,
        })),
        ...(profile?.actions || []).map((action) => ({
          id: action.id,
          label: action.label,
          description: action.description,
          kind: 'native',
          confirmation: true,
          inputs: (action.inputs || []).map((item) => field(item)),
          schedulable: !!action.schedulable,
        })),
      ],
      onboarding: setup
        ? {
            modes: [...new Set(setup.steps.flatMap((step) => step.modes))],
            fields: setup.fields.map((item) => field(item)),
            warnings: setup.warnings || [],
          }
        : null,
      oidc: profile?.oidc_client
        ? {
            callback: profile.oidc_client.callback_path,
            administratorRole: profile.oidc_client.administrator_role || null,
          }
        : null,
      database: recipe?.database
        ? [...new Set([recipe.database.engine, ...(recipe.database.engines || [])])]
        : null,
      databaseTls: recipe?.database?.engine_ssl_modes || {},
    };
  };
  const stackGuide = (stack) => ({
    images: Object.entries(stack.compose?.services || {}).map(([id, service]) => ({
      id,
      image: service.image,
    })),
    inputs: Object.entries(stack.inputs || {}).map(([id, item]) => field(item, id)),
    instructions: stack.instructions || [],
    requirements: stack.requirements || [],
    services: Object.entries(stack.compose?.services || {}).flatMap(([id, service]) => {
      const recipe = service['x-selfhost-app'];
      const profile = service['x-selfhost-integration'];
      return recipe || profile ? [serviceGuide(id, recipe, profile)] : [];
    }),
  });
  for (const card of cards) {
    const recipe = recipes.get(card.id);
    card.guide = guides.get(card.id) || { setup: [], integration_notes: [], limitations: [] };
    card.deploymentOnly = !!recipe?.deployment_only;
    card.existing = existing.has(card.id);
    card.existingProfile = existingGuide(card.id);
    const declared = methods.filter((method) => method.app === card.id);
    card.deployments = declared.map((method) => {
      if (method.blueprint && !stacks.has(method.blueprint))
        throw Error(`Missing blueprint ${method.blueprint}`);
      return {
        id: method.id,
        name: method.name,
        description: method.description,
        default: !!method.default,
        stack: false,
        blueprint: method.blueprint || null,
        existingProfile: existingGuide(method.blueprint || card.id) || existingGuide(card.id),
        ...(method.blueprint
          ? stackGuide(stacks.get(method.blueprint))
          : {
              inputs: [],
              instructions: [],
              requirements: [],
              images: [
                {
                  id: method.recipe || card.id,
                  image: recipes.get(method.recipe || card.id)?.image,
                },
              ],
              services: [serviceGuide(method.recipe || card.id, method.recipe || card.id)],
            }),
      };
    });
    if (!card.deployments.length && card.kind === 'app' && !card.deploymentOnly) {
      card.deployments = [
        {
          id: '',
          name: 'Docker Compose',
          description: 'Editable Compose files in your own directory.',
          default: true,
          stack: false,
          blueprint: null,
          inputs: [],
          instructions: [],
          requirements: [],
          images: [{ id: card.id, image: card.image }],
          services: [serviceGuide(card.id, card.id)],
        },
      ];
    } else if (!card.deployments.length && card.kind === 'stack') {
      card.deployments = [
        {
          id: '',
          name: card.name,
          description: card.description,
          default: true,
          stack: true,
          blueprint: card.id,
          ...stackGuide(stacks.get(card.id)),
        },
      ];
    }
    if (card.deploymentOnly && !card.deployments.length)
      throw Error(`Missing deployment for ${card.id}`);
    // Tags describe features actually attached to at least one deployment choice.
    const services = card.deployments.flatMap((method) => method.services);
    const tags = new Set(
      card.capabilities.filter(
        (tag) => !['settings', 'oidc', 'onboarding', 'database', 'scheduled'].includes(tag),
      ),
    );
    if (services.some((service) => service.settings.length)) tags.add('settings');
    if (services.some((service) => service.oidc)) tags.add('oidc');
    if (services.some((service) => service.onboarding)) tags.add('onboarding');
    if (services.some((service) => service.database)) tags.add('database');
    if (services.some((service) => service.actions.some((action) => action.schedulable)))
      tags.add('scheduled');
    if (services.some((service) => service.actions.length)) tags.add('actions');
    if (card.deployments.some((method) => method.blueprint)) tags.add('stack');
    card.capabilities = [...tags];
    card.guideUrl = `/apps/${card.id}`;
  }
  cards.sort((a, b) => a.name.localeCompare(b.name, 'en'));
  if (new Set(cards.map((app) => app.id)).size !== cards.length)
    throw Error('Duplicate catalogue app ID');
  return { schema: 1, capabilities: capabilityLabels, apps: cards };
}
