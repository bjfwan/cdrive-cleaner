import { onScopeDispose, shallowRef, type ShallowRef } from 'vue';

/**
 * Throttled shallowRef writer.
 *
 * Pushes are coalesced using `requestAnimationFrame` plus a wall-clock
 * threshold of `intervalMs`. Only the latest value reaches the reactive
 * `state` after the interval elapses, so high-frequency event streams
 * (e.g. Tauri scan progress events) only trigger one render at most every
 * `intervalMs`.
 *
 * The composable registers `onScopeDispose` and flushes the pending value
 * synchronously, ensuring the final state is observed before unmount.
 */
export interface ThrottledShallowRef<T> {
  state: ShallowRef<T>;
  push: (next: T) => void;
  flush: () => void;
}

export function useThrottledShallowRef<T>(initial: T, intervalMs = 80): ThrottledShallowRef<T> {
  const state = shallowRef<T>(initial);

  let latest: T = initial;
  let hasPending = false;
  let lastCommit = 0;
  let raf = 0;
  const useRaf = typeof requestAnimationFrame === 'function';

  function commit() {
    if (!hasPending) return;
    hasPending = false;
    state.value = latest;
    lastCommit = nowMs();
  }

  function nowMs() {
    return typeof performance !== 'undefined' && typeof performance.now === 'function'
      ? performance.now()
      : Date.now();
  }

  function tick() {
    raf = 0;
    if (!hasPending) return;
    const elapsed = nowMs() - lastCommit;
    if (elapsed >= intervalMs) {
      commit();
    } else {
      schedule();
    }
  }

  function schedule() {
    if (raf !== 0) return;
    if (useRaf) {
      raf = requestAnimationFrame(tick);
    } else {
      // Node / SSR / test fallback.
      raf = setTimeout(tick, intervalMs) as unknown as number;
    }
  }

  function push(next: T) {
    latest = next;
    hasPending = true;
    const elapsed = nowMs() - lastCommit;
    if (elapsed >= intervalMs && raf === 0) {
      // Allow the very first push (or the one after a long idle) to commit
      // promptly so the UI shows movement immediately.
      commit();
      return;
    }
    schedule();
  }

  function flush() {
    if (raf !== 0) {
      if (useRaf) {
        cancelAnimationFrame(raf);
      } else {
        clearTimeout(raf as unknown as ReturnType<typeof setTimeout>);
      }
      raf = 0;
    }
    commit();
  }

  onScopeDispose(() => {
    flush();
  });

  return { state, push, flush };
}
