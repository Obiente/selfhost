import { onMounted, onUnmounted } from 'vue';

/** Poll only visible pages and never overlap a request. */
export function usePolling(
  action: () => Promise<unknown>,
  interval: number,
  enabled: () => boolean = () => true,
) {
  let timer: ReturnType<typeof setInterval> | undefined,
    running = false,
    disposed = false;
  async function refresh() {
    if (disposed || running || !enabled() || document.visibilityState !== 'visible') return;
    running = true;
    try {
      await action();
    } finally {
      running = false;
    }
  }
  function visible() {
    if (document.visibilityState === 'visible') void refresh().catch(() => {});
  }
  onMounted(() => {
    timer = setInterval(() => {
      void refresh().catch(() => {});
    }, interval);
    document.addEventListener('visibilitychange', visible);
  });
  onUnmounted(() => {
    disposed = true;
    clearInterval(timer);
    document.removeEventListener('visibilitychange', visible);
  });
  return { refresh };
}
