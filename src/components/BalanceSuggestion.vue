<script setup lang="ts">
import { computed, ref } from 'vue';
import type { BalanceSuggestion, BalanceItem } from '../types/breakdown';
import type { DiskInfo } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  suggestion: BalanceSuggestion | null;
  loading: boolean;
  availableDisks: DiskInfo[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'execute': [items: BalanceItem[]];
}>();

const checkedPaths = ref<Set<string>>(new Set());

const sortedItems = computed(() => {
  if (!props.suggestion) return [];
  return [...props.suggestion.suggested_items].sort((a, b) => a.priority - b.priority);
});

const selectedSize = computed(() => {
  let total = 0;
  for (const item of sortedItems.value) {
    if (checkedPaths.value.has(item.path)) {
      total += item.size;
    }
  }
  return total;
});

const projectedSource = computed(() => {
  if (!props.suggestion) return 0;
  return props.suggestion.current_source_used - selectedSize.value;
});

const projectedTarget = computed(() => {
  if (!props.suggestion) return 0;
  return props.suggestion.current_target_used + selectedSize.value;
});

const sourceDiskInfo = computed(() => {
  if (!props.suggestion) return null;
  return props.availableDisks.find(d => `${d.drive_letter}\\` === props.suggestion!.source_disk) ?? null;
});

const targetDiskInfo = computed(() => {
  if (!props.suggestion) return null;
  return props.availableDisks.find(d => `${d.drive_letter}\\` === props.suggestion!.target_disk) ?? null;
});

function sourceBarPercent(used: number) {
  const total = sourceDiskInfo.value?.total_space ?? 1;
  return Math.min((used / total) * 100, 100);
}

function targetBarPercent(used: number) {
  const total = targetDiskInfo.value?.total_space ?? 1;
  return Math.min((used / total) * 100, 100);
}

function toggleItem(path: string) {
  const next = new Set(checkedPaths.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  checkedPaths.value = next;
}

function selectAll() {
  const next = new Set<string>();
  for (const item of sortedItems.value) {
    next.add(item.path);
  }
  checkedPaths.value = next;
}

function handleExecute() {
  const selected = sortedItems.value.filter(it => checkedPaths.value.has(it.path));
  if (selected.length > 0) {
    emit('execute', selected);
  }
}

function actionLabel(action: BalanceItem['action']) {
  return action === 'migrate' ? '搬走' : '重定向';
}
</script>

<template>
  <section class="balance">
    <div v-if="loading" class="balance-loading">
      <div class="balance-spinner"></div>
      <span>分析磁盘平衡建议…</span>
    </div>

    <template v-else-if="suggestion">
      <div class="balance-bars">
        <div class="balance-disk">
          <div class="balance-disk-label">
            <strong>{{ suggestion.source_disk }}</strong>
            <small>来源</small>
          </div>
          <div class="balance-bar-track">
            <div
              class="balance-bar-fill current"
              :style="{ width: sourceBarPercent(suggestion.current_source_used) + '%' }"
            ></div>
            <div
              class="balance-bar-fill projected"
              :style="{ width: sourceBarPercent(projectedSource) + '%' }"
            ></div>
          </div>
          <div class="balance-disk-stats">
            <span>当前 {{ formatBytes(suggestion.current_source_used) }}</span>
            <span class="projected-label">→ {{ formatBytes(projectedSource) }}</span>
          </div>
        </div>

        <div class="balance-arrow">
          <span class="balance-arrow-icon">→</span>
          <strong>可搬 {{ formatBytes(selectedSize || suggestion.total_movable) }}</strong>
        </div>

        <div class="balance-disk">
          <div class="balance-disk-label">
            <strong>{{ suggestion.target_disk }}</strong>
            <small>目标</small>
          </div>
          <div class="balance-bar-track">
            <div
              class="balance-bar-fill current target"
              :style="{ width: targetBarPercent(suggestion.current_target_used) + '%' }"
            ></div>
            <div
              class="balance-bar-fill projected target"
              :style="{ width: targetBarPercent(projectedTarget) + '%' }"
            ></div>
          </div>
          <div class="balance-disk-stats">
            <span>当前 {{ formatBytes(suggestion.current_target_used) }}</span>
            <span class="projected-label">→ {{ formatBytes(projectedTarget) }}</span>
          </div>
        </div>
      </div>

      <div class="balance-items">
        <div class="balance-items-head">
          <span>建议项 ({{ sortedItems.length }})</span>
          <button class="balance-select-all" @click="selectAll">全选</button>
        </div>
        <ul class="balance-item-list">
          <li
            v-for="item in sortedItems"
            :key="item.path"
            class="balance-item-row"
            :class="{ checked: checkedPaths.has(item.path) }"
          >
            <button class="balance-check" @click="toggleItem(item.path)">
              <svg v-if="checkedPaths.has(item.path)" viewBox="0 0 16 16" width="14" height="14">
                <path d="M3 8.5L6.5 12L13 4.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" fill="none" />
              </svg>
            </button>
            <div class="balance-item-info">
              <span class="balance-item-name">{{ item.name }}</span>
              <span class="balance-item-cat">{{ item.category }}</span>
            </div>
            <span class="balance-item-size">{{ formatBytes(item.size) }}</span>
            <span class="balance-item-action" :data-action="item.action">{{ actionLabel(item.action) }}</span>
          </li>
        </ul>
      </div>

      <div class="balance-footer">
        <span>已选 {{ checkedPaths.size }} 项 · {{ formatBytes(selectedSize) }}</span>
        <button
          class="balance-execute-btn"
          :disabled="checkedPaths.size === 0"
          @click="handleExecute"
        >一键执行全部</button>
      </div>
    </template>
  </section>
</template>

<style scoped>
.balance {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1rem 1.1rem;
  border-radius: var(--radius-md);
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.balance-loading {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.5rem 0;
}

.balance-loading span {
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
}

.balance-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

.balance-bars {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 0.85rem;
  align-items: center;
}

.balance-disk {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.balance-disk-label {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
}

.balance-disk-label strong {
  font-size: 0.92rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.balance-disk-label small {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
}

.balance-bar-track {
  position: relative;
  height: 18px;
  border-radius: 999px;
  background: var(--color-bg-tertiary);
  overflow: hidden;
}

.balance-bar-fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  border-radius: 999px;
  transition: width 0.5s cubic-bezier(0.16, 1, 0.3, 1);
}

.balance-bar-fill.current {
  background: rgba(15, 118, 110, 0.3);
  z-index: 1;
}

.balance-bar-fill.projected {
  background: var(--color-highlight);
  z-index: 2;
}

.balance-bar-fill.target.current {
  background: rgba(37, 99, 235, 0.3);
}

.balance-bar-fill.target.projected {
  background: var(--color-info);
}

.balance-disk-stats {
  display: flex;
  justify-content: space-between;
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
}

.projected-label {
  color: var(--color-highlight);
  font-weight: 600;
}

.balance-arrow {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.3rem;
  padding: 0 0.5rem;
}

.balance-arrow-icon {
  font-size: 1.4rem;
  color: var(--color-highlight);
}

.balance-arrow strong {
  font-size: 0.76rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  white-space: nowrap;
}

.balance-items {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.balance-items-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 0.2rem;
}

.balance-items-head span {
  font-size: 0.78rem;
  font-weight: 700;
  color: var(--color-text-secondary);
}

.balance-select-all {
  padding: 0.3rem 0.6rem;
  border: 1px solid var(--color-border-light);
  border-radius: 8px;
  background: transparent;
  color: var(--color-text-tertiary);
  font-size: 0.72rem;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.balance-select-all:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.balance-item-list {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  max-height: 220px;
  overflow-y: auto;
}

.balance-item-row {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.6rem;
  padding: 0.45rem 0.6rem;
  border-radius: 8px;
  transition: background var(--transition-fast);
}

.balance-item-row:hover {
  background: rgba(15, 23, 32, 0.02);
}

.balance-item-row.checked {
  background: rgba(15, 118, 110, 0.04);
}

.balance-check {
  width: 20px;
  height: 20px;
  border-radius: 6px;
  border: 1.5px solid var(--color-border-medium);
  background: var(--color-surface);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: white;
  transition: background var(--transition-fast), border-color var(--transition-fast);
}

.balance-item-row.checked .balance-check {
  background: var(--color-highlight);
  border-color: var(--color-highlight);
}

.balance-item-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
}

.balance-item-name {
  font-size: 0.84rem;
  font-weight: 600;
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.balance-item-cat {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
}

.balance-item-size {
  font-size: 0.84rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
  flex-shrink: 0;
}

.balance-item-action {
  padding: 0.15rem 0.45rem;
  border-radius: 999px;
  font-size: 0.66rem;
  font-weight: 700;
  flex-shrink: 0;
}

.balance-item-action[data-action='migrate'] {
  background: rgba(15, 159, 110, 0.1);
  color: #0d8a5f;
}

.balance-item-action[data-action='redirect'] {
  background: rgba(37, 99, 235, 0.1);
  color: #1d4ed8;
}

.balance-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--color-border-light);
}

.balance-footer span {
  font-size: 0.8rem;
  color: var(--color-text-secondary);
  font-feature-settings: 'tnum';
}

.balance-execute-btn {
  padding: 0.55rem 1rem;
  border-radius: 10px;
  border: none;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-size: 0.82rem;
  font-weight: 700;
  cursor: pointer;
  transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  box-shadow: 0 8px 18px rgba(17, 24, 39, 0.15);
}

.balance-execute-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 12px 24px rgba(17, 24, 39, 0.22);
}

.balance-execute-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 720px) {
  .balance-bars {
    grid-template-columns: 1fr;
    gap: 0.5rem;
  }

  .balance-arrow {
    flex-direction: row;
    padding: 0.3rem 0;
  }
}
</style>
