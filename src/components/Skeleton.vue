<script setup lang="ts">
import { computed } from 'vue';

interface Props {
  variant?: 'text' | 'rect' | 'circle' | 'row';
  width?: string | number;
  height?: string | number;
  rounded?: string;
  rows?: number;
  gap?: string;
  animated?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'text',
  rows: 1,
  gap: '0.5rem',
  animated: true,
});

const w = computed(() => (typeof props.width === 'number' ? `${props.width}px` : props.width));
const h = computed(() => (typeof props.height === 'number' ? `${props.height}px` : props.height));

const blockStyle = computed(() => {
  const defaults = {
    text: { width: w.value ?? '100%', height: h.value ?? '0.9rem', radius: props.rounded ?? 'var(--radius-xs)' },
    rect: { width: w.value ?? '100%', height: h.value ?? '8rem', radius: props.rounded ?? 'var(--radius-sm)' },
    circle: { width: w.value ?? '2.5rem', height: h.value ?? '2.5rem', radius: props.rounded ?? '50%' },
    row: { width: w.value ?? '100%', height: h.value ?? '3rem', radius: props.rounded ?? 'var(--radius-sm)' },
  }[props.variant];
  return {
    width: defaults.width,
    height: defaults.height,
    borderRadius: defaults.radius,
  };
});
</script>

<template>
  <div class="skeleton-group" :style="{ gap }">
    <div
      v-for="i in rows"
      :key="i"
      class="skeleton-block"
      :class="{ 'skeleton-block--animated': animated }"
      :style="blockStyle"
      aria-hidden="true"
    ></div>
  </div>
</template>

<style scoped>
.skeleton-group {
  display: flex;
  flex-direction: column;
  width: 100%;
}

.skeleton-block {
  position: relative;
  overflow: hidden;
  background: var(--color-surface-muted);
  border: 1px solid var(--color-border-light);
}

.skeleton-block--animated::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(
    90deg,
    transparent 0%,
    var(--color-border-light) 40%,
    var(--color-border-medium) 50%,
    var(--color-border-light) 60%,
    transparent 100%
  );
  transform: translateX(-100%);
  animation: skeleton-shimmer 1.4s ease-in-out infinite;
}

@keyframes skeleton-shimmer {
  to { transform: translateX(100%); }
}

@media (prefers-reduced-motion: reduce) {
  .skeleton-block--animated::after {
    animation: none;
  }
}
</style>
