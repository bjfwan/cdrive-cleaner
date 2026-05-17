<script setup lang="ts">
import type { JunkCategory } from '../types/junk';

const emit = defineEmits<{
  apply: [categories: JunkCategory[] | null];
}>();

interface Preset {
  readonly key: string;
  readonly title: string;
  readonly subtitle: string;
  readonly recommended?: boolean;
  readonly categories: readonly JunkCategory[] | null;
}

const presets: readonly Preset[] = [
  {
    key: 'quick',
    title: '快速清理',
    subtitle: '浏览器缓存 + 缩略图 + 崩溃转储，最安全的清理',
    categories: ['browser_cache', 'thumbnail_cache', 'crash_dump'],
  },
  {
    key: 'standard',
    title: '标准清理',
    subtitle: '推荐 · 上面 + 系统临时 + Windows 更新残留',
    recommended: true,
    categories: ['browser_cache', 'thumbnail_cache', 'crash_dump', 'system_temp', 'windows_update'],
  },
  {
    key: 'deep',
    title: '深度清理',
    subtitle: '管理员推荐 · 全部分类，含回收站、应用日志',
    categories: null,
  },
];
</script>

<template>
  <div class="junk-presets">
    <button
      v-for="preset in presets"
      :key="preset.key"
      type="button"
      class="preset-card"
      :class="{ recommended: preset.recommended }"
      :aria-label="preset.recommended ? `${preset.title}（推荐）` : preset.title"
      @click="emit('apply', preset.categories === null ? null : [...preset.categories])"
    >
      <span v-if="preset.recommended" class="preset-badge" aria-hidden="true">推荐</span>
      <div class="preset-glow" aria-hidden="true"></div>

      <div class="preset-body">
        <h3 class="preset-title">{{ preset.title }}</h3>
        <p class="preset-subtitle">{{ preset.subtitle }}</p>
      </div>

      <div class="preset-footer">
        <span>扫描后显示可释放空间</span>
      </div>
    </button>
  </div>
</template>

<style scoped>
.junk-presets {
  display: flex;
  flex-direction: row;
  gap: 16px;
  align-items: stretch;
  width: 100%;
}

.preset-card {
  position: relative;
  flex: 1 1 0;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 1rem;
  padding: 1.15rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(247, 241, 232, 0.9));
  box-shadow: var(--shadow-xs);
  text-align: left;
  cursor: pointer;
  overflow: hidden;
  font: inherit;
  color: inherit;
  transition: transform var(--transition-base), box-shadow var(--transition-base),
    border-color var(--transition-base), background var(--transition-base);
}

.preset-card:hover {
  transform: translateY(-2px);
  border-color: var(--color-border-medium);
  box-shadow: var(--shadow-sm);
}

.preset-card:focus-visible {
  outline: 2px solid rgba(15, 118, 110, 0.45);
  outline-offset: 2px;
}

.preset-card.recommended {
  border-color: rgba(15, 118, 110, 0.32);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.88), rgba(239, 248, 246, 0.88));
  box-shadow: 0 18px 36px rgba(15, 118, 110, 0.12);
}

.preset-card.recommended:hover {
  box-shadow: 0 22px 44px rgba(15, 118, 110, 0.18);
}

.preset-glow {
  position: absolute;
  inset: auto -15% -35% auto;
  width: 9rem;
  height: 9rem;
  background: radial-gradient(circle, rgba(15, 118, 110, 0.12), transparent 68%);
  pointer-events: none;
  transition: transform var(--transition-slow), opacity var(--transition-slow);
}

.preset-card:hover .preset-glow,
.preset-card.recommended .preset-glow {
  transform: scale(1.08);
  opacity: 1;
}

.preset-badge {
  position: absolute;
  top: 0.75rem;
  right: 0.75rem;
  padding: 0.25rem 0.6rem;
  border-radius: var(--radius-pill);
  background: rgba(15, 118, 110, 0.92);
  color: #fff;
  font-size: 0.68rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  z-index: 1;
}

.preset-body {
  position: relative;
  z-index: 1;
}

.preset-title {
  font-size: 1.05rem;
  font-weight: 800;
  margin: 0 0 0.45rem;
  color: var(--color-text-primary);
}

.preset-subtitle {
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--color-text-tertiary);
  margin: 0;
}

.preset-footer {
  position: relative;
  z-index: 1;
  padding-top: 0.75rem;
  border-top: 1px dashed rgba(23, 23, 23, 0.08);
  font-size: 0.72rem;
  letter-spacing: 0.04em;
  color: var(--color-text-tertiary);
}

@media (max-width: 720px) {
  .junk-presets {
    flex-direction: column;
  }
}
</style>
