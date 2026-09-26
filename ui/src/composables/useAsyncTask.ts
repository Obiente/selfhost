import { ref } from 'vue';

/** One user action at a time; errors stay associated with the initiating flow. */
export function useAsyncTask() {
  const busy = ref(false),
    error = ref('');
  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    if (busy.value) return;
    busy.value = true;
    error.value = '';
    try {
      return await action();
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason);
    } finally {
      busy.value = false;
    }
  }
  return {
    busy,
    error,
    run,
    clearError: () => {
      error.value = '';
    },
  };
}
