import { ref, watch } from 'vue';

export function useCountUp(target: () => number, durationMs = 600) {
  const value = ref(target());
  let raf = 0;
  let startTime = 0;
  let from = value.value;
  let to = target();

  const ease = (t: number) => 1 - Math.pow(1 - t, 3);

  function tick(ts: number) {
    if (!startTime) startTime = ts;
    const t = Math.min((ts - startTime) / durationMs, 1);
    value.value = from + (to - from) * ease(t);
    if (t < 1) {
      raf = requestAnimationFrame(tick);
    } else {
      value.value = to;
    }
  }

  watch(target, (next) => {
    from = value.value;
    to = next;
    startTime = 0;
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(tick);
  });

  return value;
}
