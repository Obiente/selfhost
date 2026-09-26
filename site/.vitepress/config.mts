import { defineConfig } from 'vitepress';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { cliMarkdown } from './cli-markdown.mjs';
const require = createRequire(import.meta.url);

export default defineConfig({
  title: 'selfhost',
  description:
    'Install, connect and manage your self-hosted services. Keep your configuration and your choices.',
  srcDir: '../docs',
  srcExclude: ['blog/**'],
  appearance: 'force-dark',
  cleanUrls: true,
  sitemap: { hostname: 'https://selfhost.obiente.org' },
  lastUpdated: false,
  markdown: { config: cliMarkdown },
  head: [['link', { rel: 'icon', href: '/brand/selfhost-monogram.svg', type: 'image/svg+xml' }]],
  vite: {
    publicDir: fileURLToPath(new URL('../../ui/public', import.meta.url)),
    resolve: {
      alias: [
        { find: /^vue$/, replacement: require.resolve('vue/dist/vue.runtime.esm-bundler.js') },
        { find: 'vue/server-renderer', replacement: require.resolve('vue/server-renderer') },
      ],
    },
  },
  themeConfig: {
    logo: '/brand/selfhost-monogram.svg',
    siteTitle: 'selfhost',
    nav: [
      { text: 'Catalogue', link: '/catalogue' },
      { text: 'Get started', link: '/getting-started' },
      { text: 'Dashboard', link: '/dashboard' },
      { text: 'CLI', link: '/cli' },
      { text: 'Contribute', link: '/contributing' },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/obiente/selfhost' }],
    search: { provider: 'local' },
    sidebar: [
      {
        text: 'Start here',
        items: [
          { text: 'Install Selfhost', link: '/getting-started' },
          { text: 'Guided setup', link: '/guided-setup' },
          { text: 'Standalone CLI', link: '/standalone' },
          { text: 'Dashboard guide', link: '/dashboard' },
          { text: 'Background service & domains', link: '/dashboard-hosting' },
          { text: 'CLI guide', link: '/cli' },
          { text: 'Terminal interface', link: '/tui' },
          { text: 'Command reference', link: '/cli-reference' },
          { text: 'Updates & recovery', link: '/updates' },
        ],
      },
      {
        text: 'Your services',
        items: [
          { text: 'Browse the full catalogue', link: '/catalogue' },
          { text: 'Tasks & triggers', link: '/tasks-and-triggers' },
          { text: 'Catalog & deployment choices', link: '/catalog-apps' },
          { text: 'Existing apps', link: '/existing-apps' },
          { text: 'Custom setups & databases', link: '/setups-and-databases' },
          { text: 'App settings & actions', link: '/app-integrations' },
          { text: 'Automatic app setup', link: '/app-onboarding' },
          { text: 'App profiles & tested workflows', link: '/app-profiles' },
          { text: 'Backups & removal', link: '/removal' },
        ],
      },
      {
        text: 'Connections & access',
        items: [
          { text: 'Identity setup', link: '/identity-setup' },
          { text: 'Servers & clusters', link: '/infrastructure' },
          { text: 'Networking & login', link: '/networking-and-login' },
          { text: 'Dedicated SSH keys', link: '/ssh-keys' },
          { text: 'Security', link: '/security' },
        ],
      },
      {
        text: 'Build with us',
        items: [
          { text: 'Contributing', link: '/contributing' },
          { text: 'Catalogue maintenance', link: '/catalog-maintenance' },
          { text: 'Packaging', link: '/distribution' },
          { text: 'Website development', link: '/website' },
        ],
      },
    ],
    outline: [2, 3],
    footer: {
      message: 'Built by Obiente. Your services, your configuration.',
      copyright: 'Licensed under AGPL-3.0-or-later.',
    },
  },
});
