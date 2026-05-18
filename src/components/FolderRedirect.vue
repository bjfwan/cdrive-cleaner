<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { KnownFolderInfo, RedirectResult } from '../types/breakdown';
import type { DiskInfo } from '../types';
import { formatBytes } from '../utils/format';
import { useToast } from '../composables/useToast';
import { IconSpinner } from './icons';

interface Props {
  folders: KnownFolderInfo[];
  availableDisks: DiskInfo[];
  loading: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'close': [];
  'redirected': [result: RedirectResult];
}>();

const showToast = useToast();
const targetDisk = ref('');
const executingId = ref<string | null>(null);
const completedIds = ref<Map<string, { success: boolean; movedBytes: number }>>(new Map());
const executingAll = ref(false);

const nonSystemDisks = computed(() =>
  props.availableDisks.filter(d => d.drive_letter !== 'C:')
);

const redirectableFolders = computed(() =>
  props.folders.filter(f => f.is_on_system_drive && f.is_default_location)
);

const alreadyRedirected = computed(() =>
  props.folders.filter(f => !f.is_on_system_drive || !f.is_default_location)
);

onMounted(() => {
  if (nonSystemDisks.value.length > 0 && !targetDisk.value) {
    targetDisk.value = `${nonSystemDisks.value[0].drive_letter}\\`;
  }
});

async function redirectFolder(folder: KnownFolderInfo) {
  if (!targetDisk.value) {
    showToast('请选择目标磁盘', '', 'warning');
    return;
  }
  executingId.value = folder.id;
  try {
    const result = await invoke<RedirectResult>('relocate_folder', {
      folderId: folder.id,
      targetPath: targetDisk.value,
      moveFiles: true,
    });
    completedIds.value.set(folder.id, {
      success: result.success,
      movedBytes: result.moved_bytes,
    });
    if (result.success) {
      showToast(`${folder.display_name} 已迁移`, `搬了 ${result.moved_files} 个文件`, 'success');
      emit('redirected', result);
    } else {
      showToast('迁移失败', result.message, 'error');
    }
  } catch (err) {
    completedIds.value.set(folder.id, { success: false, movedBytes: 0 });
    showToast('操作失败', String(err), 'error');
  } finally {
    executingId.value = null;
  }
}

async function redirectAll() {
  if (!targetDisk.value) {
    showToast('请选择目标磁盘', '', 'warning');
    return;
  }
  executingAll.value = true;
  for (const folder of redirectableFolders.value) {
    if (completedIds.value.has(folder.id)) continue;
    await redirectFolder(folder);
  }
  executingAll.value = false;
}
</script>

<template>
  <div class="redirect-panel">
    <header class="redirect-head">
      <div>
        <h3>路径重定向</h3>
        <p>把用户文件夹从 C 盘迁移到其他磁盘</p>
      </div>
      <button class="redirect-close" @click="emit('close')" aria-label="关闭">✕</button>
    </header>

    <div class="redirect-body">
      <div v-if="loading" class="redirect-loading">
        <div class="redirect-spinner"></div>
        <span>检测已知文件夹…</span>
      </div>

      <template v-else>
        <div class="redirect-target">
          <label>目标磁盘</label>
          <select v-model="targetDisk" class="redirect-select">
            <option v-for="disk in nonSystemDisks" :key="disk.drive_letter" :value="`${disk.drive_letter}\\`">
              {{ disk.drive_letter }} ({{ disk.label || 'Local' }}) · {{ formatBytes(disk.free_space) }} 可用
            </option>
          </select>
        </div>

        <div v-if="redirectableFolders.length === 0 && alreadyRedirected.length === 0" class="redirect-empty">
          暂无可检测的已知文件夹
        </div>

        <div v-else class="redirect-table">
          <div class="redirect-table-head">
            <span>文件夹</span>
            <span>当前路径</span>
            <span>大小</span>
            <span>状态</span>
            <span>操作</span>
          </div>

          <div
            v-for="folder in alreadyRedirected"
            :key="folder.id"
            class="redirect-row done"
          >
            <span class="redirect-cell name">📁 {{ folder.display_name }}</span>
            <span class="redirect-cell path">{{ folder.current_path }}</span>
            <span class="redirect-cell size">{{ formatBytes(folder.size_bytes) }}</span>
            <span class="redirect-cell status done-badge">✅ 已重定向</span>
            <span class="redirect-cell"></span>
          </div>

          <div
            v-for="folder in redirectableFolders"
            :key="folder.id"
            class="redirect-row"
            :class="{
              completed: completedIds.has(folder.id),
              success: completedIds.get(folder.id)?.success,
            }"
          >
            <span class="redirect-cell name">📁 {{ folder.display_name }}</span>
            <span class="redirect-cell path">{{ folder.current_path }}</span>
            <span class="redirect-cell size">{{ formatBytes(folder.size_bytes) }}</span>
            <span class="redirect-cell status">
              <template v-if="completedIds.has(folder.id)">
                <span v-if="completedIds.get(folder.id)?.success" class="redirect-done-label">✓ 已迁移</span>
                <span v-else class="redirect-fail-label">✕ 失败</span>
              </template>
              <template v-else>
                在 C 盘
              </template>
            </span>
            <span class="redirect-cell action">
              <button
                v-if="!completedIds.has(folder.id)"
                class="redirect-btn"
                :disabled="executingId !== null || !targetDisk"
                @click="redirectFolder(folder)"
              >
                <IconSpinner v-if="executingId === folder.id" :size="14" />
                <span v-else>迁移</span>
              </button>
            </span>
          </div>
        </div>

        <div v-if="redirectableFolders.length > 0" class="redirect-footer">
          <div class="redirect-footer-note">
            <p>会把现有文件搬到新位置，应用会自动使用新路径</p>
          </div>
          <button
            class="redirect-all-btn"
            :disabled="executingId !== null || executingAll || !targetDisk || redirectableFolders.length === 0"
            @click="redirectAll"
          >
            <IconSpinner v-if="executingAll" :size="14" />
            <span v-else>一键全部重定向</span>
          </button>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.redirect-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.redirect-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.2rem;
  border-bottom: 1px solid var(--color-border-light);
  flex-shrink: 0;
}

.redirect-head h3 {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.redirect-head p {
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
  margin-top: 0.15rem;
}

.redirect-close {
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

.redirect-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.redirect-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 1rem 1.2rem 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.redirect-loading {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 2rem 0;
  justify-content: center;
}

.redirect-loading span {
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
}

.redirect-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

.redirect-target {
  display: flex;
  align-items: center;
  gap: 0.7rem;
}

.redirect-target label {
  font-size: 0.82rem;
  font-weight: 700;
  color: var(--color-text-secondary);
  flex-shrink: 0;
}

.redirect-select {
  flex: 1;
  padding: 0.5rem 0.75rem;
  border-radius: 10px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface);
  color: var(--color-text-primary);
  font-size: 0.84rem;
  cursor: pointer;
}

.redirect-empty {
  text-align: center;
  padding: 2rem 1rem;
  color: var(--color-text-tertiary);
  font-size: 0.9rem;
}

.redirect-table {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.redirect-table-head {
  display: grid;
  grid-template-columns: 1.2fr 2fr 0.8fr 1fr 0.8fr;
  gap: 0.5rem;
  padding: 0.6rem 0.85rem;
  background: var(--color-bg-tertiary);
  font-size: 0.7rem;
  font-weight: 700;
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.redirect-row {
  display: grid;
  grid-template-columns: 1.2fr 2fr 0.8fr 1fr 0.8fr;
  gap: 0.5rem;
  align-items: center;
  padding: 0.6rem 0.85rem;
  border-top: 1px solid var(--color-border-light);
  transition: background var(--transition-fast);
}

.redirect-row:hover {
  background: rgba(15, 23, 32, 0.02);
}

.redirect-row.done {
  opacity: 0.6;
}

.redirect-row.success {
  background: rgba(15, 159, 110, 0.03);
}

.redirect-cell {
  font-size: 0.82rem;
  color: var(--color-text-primary);
  min-width: 0;
}

.redirect-cell.name {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.redirect-cell.path {
  font-family: var(--font-mono);
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.redirect-cell.size {
  font-weight: 700;
  font-feature-settings: 'tnum';
}

.redirect-cell.status {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}

.done-badge {
  color: var(--color-success);
  font-weight: 600;
}

.redirect-done-label {
  color: var(--color-success);
  font-weight: 600;
}

.redirect-fail-label {
  color: var(--color-error);
  font-weight: 600;
}

.redirect-btn {
  padding: 0.35rem 0.7rem;
  border-radius: 8px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.3rem;
  min-width: 52px;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.redirect-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.redirect-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.redirect-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--color-border-light);
}

.redirect-footer-note p {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  line-height: 1.5;
}

.redirect-all-btn {
  padding: 0.55rem 1rem;
  border-radius: 10px;
  border: none;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-size: 0.82rem;
  font-weight: 700;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  flex-shrink: 0;
  transition: transform var(--transition-fast), box-shadow var(--transition-fast);
  box-shadow: 0 8px 18px rgba(17, 24, 39, 0.15);
}

.redirect-all-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 12px 24px rgba(17, 24, 39, 0.22);
}

.redirect-all-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 720px) {
  .redirect-table-head,
  .redirect-row {
    grid-template-columns: 1fr 1fr auto;
  }

  .redirect-cell.path,
  .redirect-cell.size {
    display: none;
  }
}
</style>
