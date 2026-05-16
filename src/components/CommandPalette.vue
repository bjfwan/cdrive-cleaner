<script setup lang="ts">
import { computed, nextTick, onMounted, onBeforeUnmount, ref, watch } from 'vue';
import type { DirectoryNode, ScanResult } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  show: boolean;
  scanResult: ScanResult | null;
}

const props = defineProps<Props>();
const emit = defineEmits<{ close: []; jump: [path: string] }>();

const query = ref('');
const inputRef = ref<HTMLInputElement | null>(null);
const activeIndex = ref(0);

const allDirs = computed<DirectoryNode[]>(() => {
  if (!props.scanResult) return [];
  const out: DirectoryNode[] = [];
  const stack: DirectoryNode[] = [...props.scanResult.directories];
  while (stack.length > 0) {
    const n = stack.pop()!;
    out.push(n);
    if (n.children?.length) stack.push(...n.children);
  }
  return out;
});

const results = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) {
    return [...allDirs.value]
      .sort((a, b) => b.size - a.size)
      .slice(0, 20)
      .map((d) => ({ path: d.path, name: d.name, size: d.size }));
  }
  const matches = allDirs.value
    .filter((d) => d.path.toLowerCase().includes(q) || d.name.toLowerCase().includes(q))
    .sort((a, b) => b.size - a.size)
    .slice(0, 30);
  return matches.map((d) => ({ path: d.path, name: d.name, size: d.size }));
});

watch(results, () => { activeIndex.value = 0; });

watch(
  () => props.show,
  async (v) => {
    if (v) {
      query.value = '';
      activeIndex.value = 0;
      await nextTick();
      inputRef.value?.focus();
    }
  },
);

function onKey(e: KeyboardEvent) {
  if (!props.show) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    emit('close');
  } else if (e.key === 'ArrowDown') {
    e.preventDefault();
    activeIndex.value = Math.min(activeIndex.value + 1, results.value.length - 1);
    scrollActiveIntoView();
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    activeIndex.value = Math.max(activeIndex.value - 1, 0);
    scrollActiveIntoView();
  } else if (e.key === 'Enter') {
    const r = results.value[activeIndex.value];
    if (r) {
      e.preventDefault();
      emit('jump', r.path);
      emit('close');
    }
  }
}

function scrollActiveIntoView() {
  nextTick(() => {
    const el = document.querySelector('.cmdk-result.active');
    el?.scrollIntoView({ block: 'nearest' });
  });
}

onMounted(() => window.addEventListener('keydown', onKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="cmdk-overlay" @click="emit('close')">
      <div class="cmdk" @click.stop>
        <div class="cmdk-search">
          <span class="cmdk-prompt">⌘</span>
          <input
            ref="inputRef"
            v-model="query"
            placeholder="搜索目录或路径…"
            spellcheck="false"
            autocomplete="off"
          />
          <span class="cmdk-hint">↑↓ 选择 · Enter 跳转 · Esc 关闭</span>
        </div>

        <ul v-if="results.length > 0" class="cmdk-results">
          <li
            v-for="(r, i) in results"
            :key="r.path"
            class="cmdk-result"
            :class="{ active: i === activeIndex }"
            @mousemove="activeIndex = i"
            @click="emit('jump', r.path); emit('close')"
          >
            <div class="cmdk-result-main">
              <div class="cmdk-result-name">{{ r.name }}</div>
              <div class="cmdk-result-path">{{ r.path }}</div>
            </div>
            <div class="cmdk-result-size">{{ formatBytes(r.size) }}</div>
          </li>
        </ul>
        <div v-else class="cmdk-empty">没有匹配项</div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.cmdk-overlay {
  position: fixed; inset: 0;
  background: rgba(15, 23, 32, 0.32);
  display: flex;
  justify-content: center;
  padding-top: 12vh;
  z-index: 2200;
}

.cmdk {
  width: min(640px, 92vw);
  max-height: 64vh;
  background: var(--color-bg-secondary);
  border-radius: 18px;
  box-shadow: 0 30px 80px rgba(17, 24, 39, 0.32);
  border: 1px solid var(--color-border-light);
  overflow: hidden;
  display: flex; flex-direction: column;
}

.cmdk-search {
  display: flex; align-items: center; gap: 0.7rem;
  padding: 0.85rem 1rem;
  border-bottom: 1px solid var(--color-border-light);
}

.cmdk-prompt {
  width: 28px; height: 28px;
  display: inline-flex; align-items: center; justify-content: center;
  background: var(--color-surface);
  border: 1px solid var(--color-border-medium);
  border-radius: 8px;
  font-weight: 700;
  color: var(--color-text-secondary);
}

.cmdk-search input {
  flex: 1;
  border: none;
  background: transparent;
  font-size: 1rem;
  color: var(--color-text-primary);
  outline: none;
}

.cmdk-hint {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
  white-space: nowrap;
}

.cmdk-results {
  list-style: none;
  flex: 1; min-height: 0;
  overflow-y: auto;
  padding: 0.4rem;
}

.cmdk-result {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 0.7rem;
  padding: 0.6rem 0.7rem;
  border-radius: 10px;
  cursor: pointer;
  transition: background var(--transition-fast);
}
.cmdk-result.active { background: rgba(15, 118, 110, 0.1); }
.cmdk-result-main { min-width: 0; }
.cmdk-result-name { font-weight: 600; color: var(--color-text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.cmdk-result-path { font-size: 0.74rem; font-family: var(--font-mono); color: var(--color-text-tertiary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 0.16rem; }
.cmdk-result-size { font-size: 0.82rem; font-weight: 600; color: var(--color-text-secondary); font-feature-settings: 'tnum'; align-self: center; }

.cmdk-empty {
  padding: 2rem 1rem;
  text-align: center;
  color: var(--color-text-tertiary);
}
</style>
