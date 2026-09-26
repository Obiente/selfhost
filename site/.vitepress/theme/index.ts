import DefaultTheme from 'vitepress/theme';
import HomePage from './HomePage.vue';
import CataloguePage from './CataloguePage.vue';
import AppGuide from './AppGuide.vue';
import { h } from 'vue';
import { useData } from 'vitepress';
import CliCode from './CliCode.vue';
import CliInline from './CliInline.vue';
import CliMethodSelector from './CliMethodSelector.vue';
import './custom.css';
export default {
  extends: DefaultTheme,
  Layout: {
    setup() {
      const { frontmatter } = useData();
      return () =>
        h(DefaultTheme.Layout, null, {
          'doc-before': () =>
            frontmatter.value.cliSelector === false ? null : h(CliMethodSelector),
        });
    },
  },
  enhanceApp({ app }) {
    app.component('HomePage', HomePage);
    app.component('CataloguePage', CataloguePage);
    app.component('AppGuide', AppGuide);
    app.component('CliCode', CliCode);
    app.component('CliInline', CliInline);
  },
};
