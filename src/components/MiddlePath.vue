<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue';
import { middleTruncateForWidth } from '../utils/middleTruncate';

interface Props {
  path: string;
  font?: string;
  title?: string;
}

const props = defineProps<Props>();

const el = ref<HTMLSpanElement | null>(null);
const display = ref<string>(props.path);

let observer: ResizeObserver | null = null;
let supported = true;

function recompute() {
  const node = el.value;
  if (!node) {
    display.value = props.path;
    return;
  }
  if (!supported) {
    display.value = props.path;
    return;
  }
  const width = node.clientWidth;
  if (!width || width <= 0) {
    display.value = props.path;
    return;
  }
  display.value = middleTruncateForWidth(props.path, width, { font: props.font });
}

onMounted(() => {
  if (typeof ResizeObserver === 'undefined') {
    supported = false;
    display.value = props.path;
    return;
  }
  observer = new ResizeObserver(() => {
    recompute();
  });
  if (el.value) observer.observe(el.value);
  recompute();
});

watch(
  () => props.path,
  () => {
    recompute();
  },
);

watch(
  () => props.font,
  () => {
    recompute();
  },
);

onBeforeUnmount(() => {
  if (observer) {
    observer.disconnect();
    observer = null;
  }
});
</script>

<template>
  <span ref="el" class="mp" :title="title ?? path">{{ display }}</span>
</template>

<style scoped>
.mp {
  display: block;
  width: 100%;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
}
</style>
