import { onMounted, onUnmounted } from "vue";

/**
 * WKWebView-safe polling primitive: backend → frontend state flows via
 * polling, never Tauri events. Callers pass a refresh fn; the composable calls it on
 * mount and then on a fixed interval. Default 5s cadence.
 *
 * Chat streaming is the exception — it polls its run-loop at ~150ms, not this
 * 5s composable.
 */
export function usePolling(refresh: () => void, intervalMs = 5000) {
  let timer: ReturnType<typeof setInterval> | null = null;

  onMounted(() => {
    refresh();
    timer = setInterval(refresh, intervalMs);
  });

  onUnmounted(() => {
    if (timer) clearInterval(timer);
  });
}
