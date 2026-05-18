<script setup lang="ts">
import { ref, onBeforeUnmount } from 'vue';
import { invoke } from '@tauri-apps/api/core';

interface FileExplanation {
  path: string;
  explanation: string;
  app_name: string | null;
  safe_to_delete: boolean;
  will_regenerate: boolean;
}

const visible = ref(false);
const tooltipStyle = ref<Record<string, string>>({});
const info = ref<FileExplanation | null>(null);

const cache = new Map<string, FileExplanation>();
let hoverTimer: ReturnType<typeof setTimeout> | null = null;
let currentPath = '';

async function show(path: string, el: HTMLElement) {
  currentPath = path;
  if (hoverTimer) clearTimeout(hoverTimer);

  hoverTimer = setTimeout(async () => {
    if (currentPath !== path) return;

    let data = cache.get(path);
    if (!data) {
      try {
        data = await invoke<FileExplanation>('explain_file', { path });
        cache.set(path, data);
      } catch {
        return;
      }
    }

    if (currentPath !== path) return;
    if (!data.explanation) return;

    info.value = data;

    const rect = el.getBoundingClientRect();
    const tooltipW = 300;
    let left = rect.left + rect.width / 2 - tooltipW / 2;
    left = Math.max(8, Math.min(left, window.innerWidth - tooltipW - 8));
    const top = rect.top - 8;

    tooltipStyle.value = {
      top: `${top}px`,
      left: `${left}px`,
      transform: 'translateY(-100%)',
    };

    visible.value = true;
  }, 200);
}

function hide() {
  if (hoverTimer) {
    clearTimeout(hoverTimer);
    hoverTimer = null;
  }
  currentPath = '';
  visible.value = false;
  info.value = null;
}

onBeforeUnmount(() => {
  if (hoverTimer) clearTimeout(hoverTimer);
});

defineExpose({ show, hide });
</script>

<template>
  <Teleport to="body">
    <div v-if="visible && info" class="explain-tooltip" :style="tooltipStyle">
      <div class="explain-app" v-if="info.app_name">{{ info.app_name }}</div>
      <p class="explain-text">{{ info.explanation }}</p>
      <div class="explain-meta">
        <span class="explain-tag" :class="info.safe_to_delete ? 'safe' : 'caution'">
          {{ info.safe_to_delete ? '✅ 安全删除' : '⚠️ 谨慎' }}
        </span>
        <span v-if="info.will_regenerate" class="explain-regen">会自动重建</span>
      </div>
      <div class="explain-arrow"></div>
    </div>
  </Teleport>
</template>

<style scoped>
.explain-tooltip {
  position: fixed;
  z-index: 8000;
  width: 300px;
  padding: 0.75rem 0.9rem;
  border-radius: var(--radius-md, 12px);
  background: var(--color-surface-strong, #fff);
  border: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  box-shadow: var(--shadow-lg, 0 20px 40px rgba(15, 23, 32, 0.15));
  font-family: var(--font-sans, system-ui);
  pointer-events: none;
  animation: explain-in 0.15s ease;
}

@keyframes explain-in {
  from { opacity: 0; transform: translateY(calc(-100% + 4px)); }
  to { opacity: 1; transform: translateY(-100%); }
}

.explain-app {
  font-size: 0.72rem;
  font-weight: 700;
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 0.25rem;
}

.explain-text {
  font-size: 0.82rem;
  color: var(--color-text-primary);
  line-height: 1.5;
  margin-bottom: 0.4rem;
}

.explain-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.explain-tag {
  font-size: 0.72rem;
  font-weight: 700;
  padding: 0.12rem 0.4rem;
  border-radius: 6px;
}

.explain-tag.safe {
  background: rgba(15, 159, 110, 0.1);
  color: #0d8a5f;
}

.explain-tag.caution {
  background: rgba(245, 158, 11, 0.12);
  color: #b45309;
}

.explain-regen {
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
}

.explain-arrow {
  position: absolute;
  bottom: -5px;
  left: 50%;
  transform: translateX(-50%) rotate(45deg);
  width: 10px;
  height: 10px;
  background: var(--color-surface-strong, #fff);
  border-right: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  border-bottom: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .explain-tag.safe {
  background: rgba(52, 211, 153, 0.14);
  color: #34d399;
}

[data-theme="dark"] .explain-tag.caution {
  background: rgba(251, 191, 36, 0.14);
  color: #fbbf24;
}
</style>
