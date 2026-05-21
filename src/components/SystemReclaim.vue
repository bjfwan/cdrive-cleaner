<script setup lang="ts">
import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ReclaimOpportunity, ReclaimResult } from '../types/breakdown';
import { formatBytes } from '../utils/format';
import { useToast } from '../composables/useToast';
import { IconSpinner, IconRefresh, IconDisk, IconSpeed, IconHistory, IconRollback, IconMigrate, IconDelete, IconShield } from './icons';
import ConfirmDialog from './ConfirmDialog.vue';

interface Props {
  opportunities: ReclaimOpportunity[];
  loading: boolean;
  isElevated: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'close': [];
  'reclaimed': [freedBytes: number];
}>();

const showToast = useToast();
const executingId = ref<string | null>(null);
const elapsedSeconds = ref(0);
let elapsedTimer: ReturnType<typeof setInterval> | null = null;
const completedIds = ref<Map<string, { freed: number; success: boolean; message?: string }>>(new Map());
const confirmTarget = ref<ReclaimOpportunity | null>(null);
const confirmType = ref<'danger' | 'warning'>('warning');

const statusHints: Record<string, string> = {
  windows_update: 'DISM 正在分析组件存储，通常需要 3–10 分钟…',
  delivery_optimization: '正在清理分发优化缓存…',
  hibernation: '正在禁用休眠并删除 hiberfil.sys…',
  restore_points: '正在删除系统还原点…',
  pagefile: '正在配置页面文件迁移…',
};

function startElapsedTimer() {
  elapsedSeconds.value = 0;
  elapsedTimer = setInterval(() => { elapsedSeconds.value++; }, 1000);
}
function stopElapsedTimer() {
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null; }
}
function formatElapsed(s: number): string {
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r > 0 ? `${m} 分 ${r} 秒` : `${m} 分钟`;
}

onUnmounted(() => stopElapsedTimer());

function reclaimIcon(id: string) {
  const map: Record<string, any> = {
    hibernation: IconSpeed,
    page_file: IconMigrate,
    windows_update: IconRefresh,
    temp_files: IconDelete,
    recycle_bin: IconDelete,
    delivery_optimization: IconDisk,
    system_restore: IconRollback,
    winsxs: IconShield,
  };
  return map[id] ?? IconHistory;
}

async function executeReclaim(opportunity: ReclaimOpportunity) {
  if (opportunity.risk_level === 'irreversible') {
    confirmTarget.value = opportunity;
    confirmType.value = 'danger';
    return;
  }
  if (opportunity.risk_level === 'caution') {
    confirmTarget.value = opportunity;
    confirmType.value = 'warning';
    return;
  }
  await doExecute(opportunity);
}

async function confirmAndExecute() {
  if (!confirmTarget.value) return;
  const target = confirmTarget.value;
  confirmTarget.value = null;
  await doExecute(target);
}

async function doExecute(opportunity: ReclaimOpportunity) {
  executingId.value = opportunity.id;
  startElapsedTimer();
  try {
    const result = await invoke<ReclaimResult>('execute_reclaim', {
      id: opportunity.id,
      force: false,
    });
    completedIds.value.set(opportunity.id, {
      freed: result.freed_bytes,
      success: result.success,
      message: result.message,
    });
    if (result.success) {
      showToast(`已释放 ${formatBytes(result.freed_bytes)}`, result.message, 'success');
      emit('reclaimed', result.freed_bytes);
    } else {
      showToast('操作失败', result.message, 'error');
    }
  } catch (err) {
    const errMsg = String(err);
    completedIds.value.set(opportunity.id, { freed: 0, success: false, message: errMsg });
    showToast('执行失败', errMsg, 'error');
  } finally {
    stopElapsedTimer();
    executingId.value = null;
  }
}
</script>

<template>
  <div class="reclaim-panel">
    <header class="reclaim-head">
      <div>
        <h3>系统空间回收</h3>
        <p>清理系统级缓存与残留文件</p>
      </div>
      <button class="reclaim-close" @click="emit('close')" aria-label="关闭">✕</button>
    </header>

    <div class="reclaim-body">
      <div v-if="loading" class="reclaim-loading">
        <div class="reclaim-spinner"></div>
        <span>检测可回收空间…</span>
      </div>

      <div v-else-if="opportunities.length === 0" class="reclaim-empty">
        <span>暂未检测到可回收空间</span>
      </div>

      <div v-else class="reclaim-list">
        <div
          v-for="op in opportunities"
          :key="op.id"
          class="reclaim-card"
          :class="{
            completed: completedIds.has(op.id),
            success: completedIds.get(op.id)?.success,
            failed: completedIds.has(op.id) && !completedIds.get(op.id)?.success,
          }"
        >
          <div class="reclaim-card-left">
            <span class="reclaim-card-icon"><component :is="reclaimIcon(op.id)" :size="20" /></span>
            <div class="reclaim-card-info">
              <strong>{{ op.label }}</strong>
              <span>{{ op.description }}</span>
              <div class="reclaim-tags">
                <span v-if="op.requires_admin" class="reclaim-tag admin">需管理员</span>
                <span v-if="op.requires_reboot" class="reclaim-tag reboot">需重启</span>
                <span v-if="op.risk_level === 'irreversible'" class="reclaim-tag danger">不可逆</span>
                <span v-else-if="op.risk_level === 'caution'" class="reclaim-tag caution">注意</span>
                <span v-if="!op.reversible && op.risk_level !== 'irreversible'" class="reclaim-tag">不可还原</span>
              </div>
            </div>
          </div>
          <div class="reclaim-card-right">
            <strong class="reclaim-size">{{ formatBytes(op.reclaimable_size) }}</strong>
            <template v-if="completedIds.has(op.id)">
              <span v-if="completedIds.get(op.id)?.success" class="reclaim-done">
                ✓ 已释放 {{ formatBytes(completedIds.get(op.id)!.freed) }}
              </span>
              <span v-else class="reclaim-fail" :title="completedIds.get(op.id)?.message">✕ 失败</span>
            </template>
            <template v-else-if="executingId === op.id">
              <div class="reclaim-executing">
                <div class="reclaim-executing-row">
                  <IconSpinner :size="14" />
                  <span class="reclaim-elapsed">{{ formatElapsed(elapsedSeconds) }}</span>
                </div>
                <span class="reclaim-hint">{{ statusHints[op.id] ?? '正在执行…' }}</span>
              </div>
            </template>
            <button
              v-else
              class="reclaim-btn"
              :disabled="executingId !== null"
              @click="executeReclaim(op)"
            >
              <span v-if="op.requires_admin && !isElevated">需管理员</span>
              <span v-else>执行</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <ConfirmDialog
      :show="!!confirmTarget"
      :title="confirmTarget?.risk_level === 'irreversible' ? '此操作不可逆' : '确认执行？'"
      :message="`即将执行「${confirmTarget?.label ?? ''}」，将回收约 ${formatBytes(confirmTarget?.reclaimable_size ?? 0)}。${confirmTarget?.risk_level === 'irreversible' ? '此操作无法撤回，请确认。' : '请确认操作。'}`"
      :confirm-text="confirmTarget?.risk_level === 'irreversible' ? '我确认执行' : '确认'"
      cancel-text="取消"
      :type="confirmType"
      @confirm="confirmAndExecute"
      @cancel="confirmTarget = null"
    />
  </div>
</template>

<style scoped>
.reclaim-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.reclaim-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.2rem;
  border-bottom: 1px solid var(--color-border-light);
  flex-shrink: 0;
}

.reclaim-head h3 {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.reclaim-head p {
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
  margin-top: 0.15rem;
}

.reclaim-close {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-xs);
  border: 1px solid var(--color-border-light);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.9rem;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.reclaim-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.reclaim-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 1rem 1.2rem 1.5rem;
}

.reclaim-loading {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 2rem 0;
  justify-content: center;
}

.reclaim-loading span {
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
}

.reclaim-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

.reclaim-empty {
  text-align: center;
  padding: 3rem 1rem;
  color: var(--color-text-tertiary);
  font-size: 0.9rem;
}

.reclaim-list {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.reclaim-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.85rem 1rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-light);
  background: var(--color-surface);
  transition: border-color var(--transition-fast), background var(--transition-fast);
}

.reclaim-card:hover {
  border-color: var(--color-border-medium);
}

.reclaim-card.success {
  border-color: rgba(15, 159, 110, 0.2);
  background: rgba(15, 159, 110, 0.04);
}

.reclaim-card.failed {
  border-color: rgba(220, 38, 38, 0.15);
  background: rgba(220, 38, 38, 0.03);
}

.reclaim-card-left {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  min-width: 0;
  flex: 1;
}

.reclaim-card-icon {
  flex-shrink: 0;
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
}

.reclaim-card-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.reclaim-card-info strong {
  font-size: 0.9rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.reclaim-card-info > span {
  font-size: 0.76rem;
  color: var(--color-text-tertiary);
  line-height: 1.4;
}

.reclaim-tags {
  display: flex;
  gap: 0.3rem;
  margin-top: 0.25rem;
  flex-wrap: wrap;
}

.reclaim-tag {
  padding: 0.12rem 0.4rem;
  border-radius: 999px;
  font-size: 0.64rem;
  font-weight: 700;
  background: rgba(107, 114, 128, 0.1);
  color: #4b5563;
}

.reclaim-tag.admin {
  background: rgba(37, 99, 235, 0.1);
  color: #1d4ed8;
}

.reclaim-tag.reboot {
  background: rgba(217, 119, 6, 0.1);
  color: #b45309;
}

.reclaim-tag.danger {
  background: rgba(220, 38, 38, 0.1);
  color: #b91c1c;
}

.reclaim-tag.caution {
  background: rgba(217, 119, 6, 0.1);
  color: #b45309;
}

.reclaim-card-right {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.35rem;
  flex-shrink: 0;
}

.reclaim-size {
  font-size: 1.1rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
}

.reclaim-btn {
  padding: 0.45rem 0.85rem;
  border-radius: 8px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  font-size: 0.8rem;
  font-weight: 600;
  cursor: pointer;
  min-width: 60px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.3rem;
  transition: background var(--transition-fast), color var(--transition-fast), transform var(--transition-fast);
}

.reclaim-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  transform: translateY(-1px);
}

.reclaim-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.reclaim-done {
  font-size: 0.76rem;
  font-weight: 600;
  color: var(--color-success);
}

.reclaim-fail {
  font-size: 0.76rem;
  font-weight: 600;
  color: var(--color-error);
  cursor: default;
}

.reclaim-executing {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.2rem;
}

.reclaim-executing-row {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.reclaim-elapsed {
  font-size: 0.8rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-highlight);
}

.reclaim-hint {
  font-size: 0.68rem;
  color: var(--color-text-tertiary);
  max-width: 180px;
  text-align: right;
  line-height: 1.3;
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .reclaim-tag {
  background: rgba(156, 163, 175, 0.14);
  color: #d1d5db;
}

[data-theme="dark"] .reclaim-tag.admin {
  background: rgba(96, 165, 250, 0.16);
  color: #93c5fd;
}

[data-theme="dark"] .reclaim-tag.reboot,
[data-theme="dark"] .reclaim-tag.caution {
  background: rgba(251, 191, 36, 0.14);
  color: #fbbf24;
}

[data-theme="dark"] .reclaim-tag.danger {
  background: rgba(248, 113, 113, 0.14);
  color: #fca5a5;
}
</style>
