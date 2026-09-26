import { buildCatalogue } from '../../scripts/catalogue-data.mjs';
import { fileURLToPath } from 'node:url';

export default {
  watch: ['../../catalog/**/*'],
  paths() {
    return buildCatalogue(fileURLToPath(new URL('../../', import.meta.url))).apps.map((app) => ({
      params: { app: app.id, name: app.name },
    }));
  },
  transformPageData(page) {
    page.title = `${page.params.name} setup and integrations`;
    page.description = `Choose a deployment and use ${page.params.name} with the Selfhost dashboard or standalone CLI.`;
  },
};
