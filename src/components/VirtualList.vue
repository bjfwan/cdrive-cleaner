<script setup lang="ts" generic="T">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';

interface Props {
  items: ReadonlyArray<T>;
  itemSize: number;
  buffer?: number;
  /** 当 itemSize 不能完全反映实际行高时可以放大估算 buffer。 */
  overscan?: number;
  endThreshold?: number;
}

const props = withDefaults(defineProps<Props>(), {
  buffer: 6,
  overscan: 0,
  endThreshold: 480,
});

const emit = defineEmits<{
  'scroll-near-end': [];
}>();

const containerRef = ref<HTMLElement | null>(null);
const viewportHeight = shallowRef(0);
const scrollTop = shallowRef(0);

let scrollRaf = 0;
let resizeObserver: ResizeObserver | null = null;

function checkNearEnd() {
  const el = containerRef.value;
  if (!el) return;
  if (el.scrollHeight - el.scrollTop - el.clientHeight <= props.endThreshold) {
    emit('scroll-near-end');
  }
}

function onScroll() {
  const el = containerRef.value;
  if (!el) return;
  if (scrollRaf) return;
  scrollRaf = window.requestAnimationFrame(() => {
    scrollRaf = 0;
    if (!containerRef.value) return;
    scrollTop.value = containerRef.value.scrollTop;
    checkNearEnd();
  });
}

function syncViewport() {
  const el = containerRef.value;
  if (!el) return;
  viewportHeight.value = el.clientHeight;
  scrollTop.value = el.scrollTop;
  checkNearEnd();
}

onMounted(() => {
  syncViewport();
  if (typeof ResizeObserver !== 'undefined' && containerRef.value) {
    resizeObserver = new ResizeObserver(() => {
      syncViewport();
    });
    resizeObserver.observe(containerRef.value);
  }
});

onBeforeUnmount(() => {
  if (scrollRaf) {
    cancelAnimationFrame(scrollRaf);
    scrollRaf = 0;
  }
  if (resizeObserver) {
    resizeObserver.disconnect();
    resizeObserver = null;
  }
});

watch(
  () => props.items.length,
  () => {
    // 列表长度变化时确保 scrollTop 不超过新内容范围
    const el = containerRef.value;
    if (!el) return;
    const max = Math.max(0, props.items.length * props.itemSize - el.clientHeight);
    if (el.scrollTop > max) {
      el.scrollTop = max;
      scrollTop.value = max;
    }
    checkNearEnd();
  },
);

const total = computed(() => props.items.length);
const totalHeight = computed(() => total.value * props.itemSize);

const range = computed(() => {
  const itemSize = props.itemSize;
  const buffer = props.buffer + props.overscan;
  const len = total.value;
  if (len === 0 || itemSize <= 0) {
    return { start: 0, end: 0, padTop: 0, padBottom: 0 };
  }
  const vh = viewportHeight.value || 0;
  const startIndex = Math.max(0, Math.floor(scrollTop.value / itemSize) - buffer);
  const visibleCount = Math.ceil(vh / itemSize) + buffer * 2;
  const endIndex = Math.min(len, startIndex + visibleCount);
  return {
    start: startIndex,
    end: endIndex,
    padTop: startIndex * itemSize,
    padBottom: Math.max(0, (len - endIndex) * itemSize),
  };
});

const visibleItems = computed(() => {
  const { start, end } = range.value;
  if (start === 0 && end === total.value) {
    return props.items as ReadonlyArray<T>;
  }
  const arr: T[] = [];
  for (let i = start; i < end; i++) {
    arr.push(props.items[i]);
  }
  return arr;
});

defineExpose({
  scrollToIndex(index: number, position: 'start' | 'center' = 'start') {
    const el = containerRef.value;
    if (!el) return;
    const top = index * props.itemSize;
    const target =
      position === 'center'
        ? Math.max(0, top - (el.clientHeight - props.itemSize) / 2)
        : top;
    el.scrollTop = target;
  },
  el: containerRef,
});
</script>

<template>
  <div
    ref="containerRef"
    class="vlist"
    @scroll.passive="onScroll"
  >
    <div
      class="vlist-spacer"
      :style="{ height: totalHeight + 'px' }"
    >
      <div
        class="vlist-window"
        :style="{ transform: `translateY(${range.padTop}px)` }"
      >
        <slot
          v-for="(item, i) in visibleItems"
          :key="(item as any)?.path ?? (item as any)?.id ?? (range.start + i)"
          :item="item"
          :index="range.start + i"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.vlist {
  position: relative;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  contain: strict;
}

.vlist-spacer {
  position: relative;
  width: 100%;
}

.vlist-window {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  will-change: transform;
}
</style>
