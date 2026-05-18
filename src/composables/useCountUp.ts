import { onScopeDispose, ref, watch, type Ref } from 'vue';

export interface UseCountUpOptions {
  /**
   * Animation duration in milliseconds. Defaults to `600`.
   */
  durationMs?: number;
  /**
   * If the absolute delta between the previous value and the new target is
   * smaller than this, the animation is skipped and the value is assigned
   * synchronously. Defaults to `Math.max(Math.abs(target) * 0.005, 1)`
   * (0.5% of the target, but at least 1 unit).
   */
  minDelta?: number;
  /**
   * Optional visibility predicate. When it returns `false`, no
   * `requestAnimationFrame` ticks are scheduled and the value is set
   * directly so the component does not waste main-thread work while it is
   * hidden (e.g. cart drawer closed, workspace tab inactive).
   */
  isVisible?: () => boolean;
}

/**
 * Animate a number towards `target()` using `requestAnimationFrame`.
 *
 * Compared to a naive implementation, this version:
 *   - skips raf entirely when the new target is within `minDelta` of the
 *     current value;
 *   - skips raf when `isVisible()` reports `false`;
 *   - cancels the pending frame on scope dispose to avoid leaking work
 *     after `effectScope` teardown.
 */
export function useCountUp(
  target: () => number,
  durationOrOptions: number | UseCountUpOptions = 600,
): Ref<number> {
  const options: UseCountUpOptions =
    typeof durationOrOptions === 'number' ? { durationMs: durationOrOptions } : durationOrOptions;

  const durationMs = options.durationMs ?? 600;
  const isVisible = options.isVisible;

  const value = ref(target());
  let raf = 0;
  let startTime = 0;
  let from = value.value;
  let to = target();

  const ease = (t: number) => 1 - Math.pow(1 - t, 3);

  function cancel() {
    if (raf !== 0) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
  }

  function tick(ts: number) {
    if (!startTime) startTime = ts;
    const t = Math.min((ts - startTime) / durationMs, 1);
    value.value = from + (to - from) * ease(t);
    if (t < 1) {
      raf = requestAnimationFrame(tick);
    } else {
      value.value = to;
      raf = 0;
    }
  }

  function commit(next: number) {
    cancel();
    from = value.value;
    to = next;

    const explicitMin = options.minDelta;
    const minDelta = explicitMin ?? Math.max(Math.abs(to) * 0.005, 1);
    if (Math.abs(to - from) < minDelta || !(isVisible?.() ?? true)) {
      value.value = to;
      return;
    }

    startTime = 0;
    raf = requestAnimationFrame(tick);
  }

  watch(target, (next) => commit(next));

  onScopeDispose(() => {
    cancel();
  });

  return value;
}
