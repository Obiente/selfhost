import { ref } from 'vue';
export type CliMethod = 'installed' | 'npx' | 'pnpx' | 'pnpm';
export const cliCommands = {
  installed: 'selfhost',
  npx: 'npx selfhost',
  pnpx: 'pnpx selfhost',
  pnpm: 'pnpm dlx selfhost',
};
export const cliMethod = ref<CliMethod>('installed');
const storageKey = 'selfhost-docs-cli-method';
export function loadCliMethod() {
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved && Object.hasOwn(cliCommands, saved)) cliMethod.value = saved as CliMethod;
  } catch {
    /* Storage may be disabled. */
  }
}
export function selectCliMethod(method: CliMethod) {
  cliMethod.value = method;
  try {
    localStorage.setItem(storageKey, method);
  } catch {
    /* The current page still updates. */
  }
}
