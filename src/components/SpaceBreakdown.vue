<script setup lang="ts">
import { computed, ref } from 'vue';
import type { SpaceBreakdown, BreakdownItem, BreakdownCategory } from '../types/breakdown';
import { formatBytes } from '../utils/format';

interface Props {
  breakdown: SpaceBreakdown | null;
  defaultSavings: number;
  loading: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'migrate-item': [item: BreakdownItem];
  'navigate': [path: string];
}>();

const expandedId = ref<string | null>(null);
const hoveredId = ref<string | null>(null);

const categories = computed(() => props.breakdown?.categories ?? []);
const actionableTotal = computed(() => props.breakdown?.actionable_total ?? 0);
const actionablePercent = computed(() => {
  if (!props.breakdown || props.breakdown.disk_used === 0) return 0;
  return ((actionableTotal.value / props.breakdown.disk_used) * 100).toFixed(1);
});

function toggleExpand(id: string) {
  expandedId.value = expandedId.value === id ? null : id;
}

function actionableBadge(actionable: BreakdownCategory['actionable']) {
  switch (actionable) {
    case 'full': return { icon: '✅', label: '可操作' };
    case 'partial': return { icon: '⚠️', label: '部分可操作' };
    case 'none': return { icon: '🔒', label: '不可动' };
    default: return { icon: '❓', label: '未知' };
  }
}

function barOpacity(actionable: BreakdownCategory['actionable']) {
  return actionable === 'none' ? 0.5 : 1;
}
</script>

<template>
  <section class="breakdown">
    <div v-if="loading" class="breakdown-loading">
      <div class="breakdown-spinner"></div>
      <span>正在分析空间分布…</span>
    </div>

    <template v-else-if="breakdown">
      <div class="breakdown-bar-wrap">
        <div class="breakdown-bar">
          <div
            v-for="cat in categories"
            :key="cat.id"
            class="breakdown-bar-seg"
            :style="{
              width: cat.percentage + '%',
              backgroundColor: cat.color,
              opacity: barOpacity(cat.actionable),
            }"
            @mouseenter="hoveredId = cat.id"
            @mouseleave="hoveredId = null"
          >
            <div v-if="hoveredId === cat.id" class="breakdown-tooltip">
              <strong>{{ cat.label }}</strong>
              <span>{{ formatBytes(cat.size) }} · {{ cat.percentage.toFixed(1) }}%</span>
            </div>
          </div>
        </div>
      </div>

      <ul class="breakdown-list">
        <li
          v-for="cat in categories"
          :key="cat.id"
          class="breakdown-item"
          :class="{ expanded: expandedId === cat.id }"
        >
          <div class="breakdown-item-row" @click="toggleExpand(cat.id)">
            <span class="breakdown-dot" :style="{ backgroundColor: cat.color }"></span>
            <div class="breakdown-item-info">
              <div class="breakdown-item-title">
                <strong>{{ cat.label }}</strong>
                <span class="breakdown-badge" :data-actionable="cat.actionable">
                  {{ actionableBadge(cat.actionable).icon }} {{ actionableBadge(cat.actionable).label }}
                </span>
              </div>
              <span class="breakdown-item-desc">{{ cat.description }}</span>
            </div>
            <div class="breakdown-item-stats">
              <strong>{{ formatBytes(cat.size) }}</strong>
              <small>{{ cat.percentage.toFixed(1) }}% · {{ cat.item_count }} 项</small>
            </div>
            <span class="breakdown-chevron">{{ expandedId === cat.id ? '▾' : '▸' }}</span>
          </div>

          <div v-if="expandedId === cat.id" class="breakdown-details">
            <div
              v-for="item in cat.top_items"
              :key="item.path"
              class="breakdown-detail-row"
            >
              <div class="breakdown-detail-info">
                <span class="breakdown-detail-name">{{ item.name }}</span>
                <span class="breakdown-detail-explain">{{ item.explanation }}</span>
              </div>
              <span class="breakdown-detail-size">{{ formatBytes(item.size) }}</span>
              <div class="breakdown-detail-actions">
                <button
                  v-if="item.can_migrate"
                  class="breakdown-action-btn"
                  @click.stop="emit('migrate-item', item)"
                >搬走</button>
                <button
                  v-if="item.can_delete || item.can_migrate"
                  class="breakdown-action-btn ghost"
                  @click.stop="emit('navigate', item.path)"
                >查看</button>
              </div>
            </div>
            <div v-if="cat.top_items.length === 0" class="breakdown-detail-empty">
              暂无明细
            </div>
          </div>
        </li>
      </ul>

      <div class="breakdown-footer">
        <span>可操作空间: <strong>{{ formatBytes(actionableTotal) }}</strong> ({{ actionablePercent }}%)</span>
        <span>已默认选中: <strong>{{ formatBytes(defaultSavings) }}</strong></span>
      </div>
    </template>
  </section>
</template>

<style scoped>
.breakdown {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 1rem 1.1rem;
  border-radius: var(--radius-md);
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.breakdown-loading {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.5rem 0;
}

.breakdown-loading span {
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
}

.breakdown-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

.breakdown-bar-wrap {
  padding: 0.25rem 0;
}

.breakdown-bar {
  display: flex;
  width: 100%;
  height: 32px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  background: var(--color-bg-tertiary);
}

.breakdown-bar-seg {
  position: relative;
  min-width: 2px;
  transition: opacity var(--transition-fast), filter var(--transition-fast);
  cursor: pointer;
}

.breakdown-bar-seg:hover {
  filter: brightness(1.1);
}

.breakdown-tooltip {
  position: absolute;
  bottom: calc(100% + 8px);
  left: 50%;
  transform: translateX(-50%);
  padding: 0.45rem 0.7rem;
  border-radius: var(--radius-xs);
  background: var(--color-surface-dark);
  color: var(--color-text-inverse);
  font-size: 0.74rem;
  white-space: nowrap;
  z-index: 10;
  pointer-events: none;
  box-shadow: var(--shadow-sm);
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.breakdown-tooltip strong {
  font-weight: 700;
}

.breakdown-tooltip span {
  opacity: 0.85;
}

.breakdown-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.breakdown-item {
  border-radius: var(--radius-xs);
  border: 1px solid transparent;
  transition: border-color var(--transition-fast), background var(--transition-fast);
}

.breakdown-item:hover {
  background: rgba(15, 23, 32, 0.02);
}

.breakdown-item.expanded {
  border-color: var(--color-border-light);
  background: rgba(255, 255, 255, 0.5);
}

.breakdown-item-row {
  display: grid;
  grid-template-columns: 12px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.7rem;
  padding: 0.6rem 0.75rem;
  cursor: pointer;
}

.breakdown-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.breakdown-item-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
}

.breakdown-item-title {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.breakdown-item-title strong {
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.breakdown-badge {
  padding: 0.1rem 0.4rem;
  border-radius: 999px;
  font-size: 0.66rem;
  font-weight: 700;
  background: rgba(32, 45, 58, 0.06);
  color: var(--color-text-tertiary);
}

.breakdown-badge[data-actionable='full'] {
  background: rgba(15, 159, 110, 0.1);
  color: #0d8a5f;
}

.breakdown-badge[data-actionable='partial'] {
  background: rgba(217, 119, 6, 0.1);
  color: #b45309;
}

.breakdown-badge[data-actionable='none'] {
  background: rgba(107, 114, 128, 0.1);
  color: #4b5563;
}

.breakdown-item-desc {
  font-size: 0.76rem;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.breakdown-item-stats {
  text-align: right;
  flex-shrink: 0;
}

.breakdown-item-stats strong {
  display: block;
  font-size: 0.9rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
}

.breakdown-item-stats small {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
}

.breakdown-chevron {
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.breakdown-details {
  padding: 0 0.75rem 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.breakdown-detail-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.6rem;
  padding: 0.5rem 0.7rem;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.6);
  border: 1px solid var(--color-border-light);
}

.breakdown-detail-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.12rem;
}

.breakdown-detail-name {
  font-size: 0.84rem;
  font-weight: 600;
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.breakdown-detail-explain {
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
}

.breakdown-detail-size {
  font-size: 0.84rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
  flex-shrink: 0;
}

.breakdown-detail-actions {
  display: flex;
  gap: 0.3rem;
  flex-shrink: 0;
}

.breakdown-action-btn {
  padding: 0.35rem 0.6rem;
  border-radius: 8px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 0.74rem;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.breakdown-action-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.breakdown-action-btn.ghost {
  background: transparent;
  border-color: transparent;
}

.breakdown-action-btn.ghost:hover {
  background: rgba(32, 45, 58, 0.06);
}

.breakdown-detail-empty {
  padding: 0.6rem;
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  text-align: center;
}

.breakdown-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.65rem 0.75rem;
  border-radius: var(--radius-xs);
  background: rgba(15, 118, 110, 0.04);
  border: 1px solid rgba(15, 118, 110, 0.1);
  font-size: 0.8rem;
  color: var(--color-text-secondary);
}

.breakdown-footer strong {
  color: var(--color-text-primary);
  font-weight: 700;
  font-feature-settings: 'tnum';
}

@media (max-width: 720px) {
  .breakdown-item-row {
    grid-template-columns: 12px minmax(0, 1fr) auto;
  }

  .breakdown-chevron {
    display: none;
  }

  .breakdown-footer {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.3rem;
  }
}
</style>
