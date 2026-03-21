<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  IconRiskSafe,
  IconRiskLow,
  IconRiskMedium,
  IconRiskDanger,
  IconRiskUnknown,
  IconFolder,
  IconFile,
  IconMigrate,
} from './icons';
import type { DirectoryNode, FileInfo } from '../types';
import { formatBytes, formatNumber } from '../utils/format';
import { useToast } from '../composables/useToast';

const showToast = useToast();

interface Props {
  directories: DirectoryNode[];
  totalSize: number;
  deepScanning: boolean;
  currentPath: string;
  hasDeepScanned: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  navigate: [path: string];
  'migrate-dir': [dir: DirectoryNode];
  'migrate-file': [file: FileInfo];
  'batch-migrate': [items: Array<DirectoryNode | FileInfo>];
}>();

const loadingFiles = ref(false);
const currentFiles = ref<FileInfo[]>([]);
const selectedDirs = ref<Set<string>>(new Set());
const selectedFiles = ref<Set<string>>(new Set());

const selectedItems = computed(() => {
  const items: Array<DirectoryNode | FileInfo> = [];

  props.directories.forEach((dir) => {
    if (selectedDirs.value.has(dir.path)) {
      items.push(dir);
    }
  });

  currentFiles.value.forEach((file) => {
    if (selectedFiles.value.has(file.path)) {
      items.push(file);
    }
  });

  return items;
});

const hasSelection = computed(() => selectedDirs.value.size > 0 || selectedFiles.value.size > 0);
const selectableDirs = computed(() => props.directories.filter((dir) => isDirSelectable(dir)));
const selectableFiles = computed(() => currentFiles.value.filter((file) => !file.is_symlink));
const allSelectableCount = computed(() => selectableDirs.value.length + selectableFiles.value.length);
const allSelected = computed(() => allSelectableCount.value > 0 && selectedItems.value.length === allSelectableCount.value);

function isDirSelectable(dir: DirectoryNode) {
  return !dir.is_symlink && (!props.deepScanning || dir.has_children);
}

function toggleDirSelection(dir: DirectoryNode) {
  const newSet = new Set(selectedDirs.value);

  if (newSet.has(dir.path)) {
    newSet.delete(dir.path);
  } else {
    newSet.add(dir.path);
  }

  selectedDirs.value = newSet;
}

function toggleFileSelection(file: FileInfo) {
  const newSet = new Set(selectedFiles.value);

  if (newSet.has(file.path)) {
    newSet.delete(file.path);
  } else {
    newSet.add(file.path);
  }

  selectedFiles.value = newSet;
}

function selectAll() {
  const newDirSet = new Set(selectedDirs.value);
  const newFileSet = new Set(selectedFiles.value);

  selectableDirs.value.forEach((dir) => {
    if (isDirSelectable(dir)) {
      newDirSet.add(dir.path);
    }
  });

  selectableFiles.value.forEach((file) => {
    newFileSet.add(file.path);
  });

  selectedDirs.value = newDirSet;
  selectedFiles.value = newFileSet;
}

function clearSelection() {
  selectedDirs.value = new Set();
  selectedFiles.value = new Set();
}

function handleBatchMigrate() {
  if (!hasSelection.value) {
    return;
  }

  emit('batch-migrate', selectedItems.value);
  clearSelection();
}

function handleItemClick(dir: DirectoryNode) {
  if (!props.hasDeepScanned) {
    return;
  }

  emit('navigate', dir.path);
}

async function loadDirectoryFiles(path: string) {
  loadingFiles.value = true;

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    currentFiles.value = await invoke<FileInfo[]>('scan_directory_files', { path });
  } catch {
    currentFiles.value = [];
    showToast('加载文件失败', '无法读取当前目录的文件列表', 'error');
  } finally {
    loadingFiles.value = false;
  }
}

watch(
  () => [props.currentPath, props.totalSize] as const,
  ([newPath]) => {
    loadDirectoryFiles(newPath);
    clearSelection();
  },
  { immediate: true },
);

function getVerdictLabel(verdict?: string): string {
  switch (verdict) {
    case 'safe':
      return '安全';
    case 'safe_after_action':
      return '需要操作';
    case 'blocked':
      return '阻塞';
    case 'system_critical':
      return '系统关键';
    default:
      return '未知';
  }
}

function getVerdictClass(verdict?: string): string {
  switch (verdict) {
    case 'safe':
      return 'risk-safe';
    case 'safe_after_action':
      return 'risk-warning';
    case 'blocked':
      return 'risk-blocked';
    case 'system_critical':
      return 'risk-danger';
    default:
      return '';
  }
}
</script>

<template>
  <div class="list-view">
    <div v-if="directories.length === 0 && currentFiles.length === 0 && !loadingFiles" class="empty">
      <IconFolder class="empty-icon" :size="44" />
      <h3>此目录目前没有内容</h3>
      <p>没有检测到子目录或文件。</p>
    </div>

    <div v-else-if="loadingFiles" class="loading">
      <div class="spinner"></div>
      <span>正在加载当前目录下的文件明细...</span>
    </div>

    <template v-else>
      <div v-if="hasSelection" class="batch-toolbar">
        <div class="batch-copy">
          <span class="batch-info">已选择 {{ selectedItems.length }} 项</span>
          <span class="batch-note">{{ hasDeepScanned ? '可以直接发起批量迁移' : '完成深度扫描后可启用批量迁移' }}</span>
        </div>

        <div class="batch-actions">
          <button class="batch-btn batch-migrate-btn" @click="handleBatchMigrate" :disabled="!hasDeepScanned">
            <IconMigrate :size="16" />
            批量迁移
          </button>
          <button class="batch-btn batch-clear-btn" @click="clearSelection">清除选择</button>
        </div>
      </div>

      <div class="table-shell">
        <div class="table-header">
          <div class="th th-checkbox">
            <input
              type="checkbox"
              class="checkbox"
              :checked="allSelected"
              @change="allSelected ? clearSelection() : selectAll()"
            />
          </div>
          <div class="th th-name">名称</div>
          <div class="th th-size">大小</div>
          <div class="th th-percent">占比</div>
          <div class="th th-files">文件数/类型</div>
          <div class="th th-actions">操作</div>
        </div>

        <div class="table-body">
          <div
            v-for="dir in directories"
            :key="dir.path"
            class="table-row"
            :class="{
              'row-disabled': !isDirSelectable(dir),
              'row-selected': selectedDirs.has(dir.path),
              'row-link': dir.is_symlink,
            }"
          >
            <div class="td td-checkbox" @click.stop>
              <input
                type="checkbox"
                class="checkbox"
                :checked="selectedDirs.has(dir.path)"
                :disabled="!isDirSelectable(dir)"
                @change="toggleDirSelection(dir)"
              />
            </div>

            <div
              class="td td-name"
              @click="handleItemClick(dir)"
              :class="{ disabled: !hasDeepScanned }"
              :title="!hasDeepScanned ? '请先进行深度扫描以查看子目录' : ''"
            >
              <IconFolder :size="18" />
              <span class="name-text">{{ dir.name }}</span>
              <span v-if="dir.is_symlink" class="badge symlink" :title="dir.link_target || '迁移后的链接占位'">链接</span>
              <span
                v-if="dir.safety"
                class="risk-badge"
                :class="getVerdictClass(dir.safety.verdict)"
                :title="`${getVerdictLabel(dir.safety.verdict)} - ${dir.safety.app_type}`"
              >
                <IconRiskSafe v-if="dir.safety.verdict === 'safe'" :size="16" />
                <IconRiskLow v-else-if="dir.safety.verdict === 'safe_after_action'" :size="16" />
                <IconRiskMedium v-else-if="dir.safety.verdict === 'blocked'" :size="16" />
                <IconRiskDanger v-else-if="dir.safety.verdict === 'system_critical'" :size="16" />
                <IconRiskUnknown v-else :size="16" />
              </span>
              <span v-if="deepScanning && !dir.has_children" class="badge">扫描中</span>
            </div>

            <div class="td td-size" @click="handleItemClick(dir)" :class="{ disabled: !hasDeepScanned }">
              {{ formatBytes(dir.size) }}
            </div>

            <div class="td td-percent" @click="handleItemClick(dir)" :class="{ disabled: !hasDeepScanned }">
              <div class="percent-container">
                <div class="percent-bar" :style="{ width: `${totalSize ? (dir.size / totalSize) * 100 : 0}%` }"></div>
                <span class="percent-text">{{ totalSize ? ((dir.size / totalSize) * 100).toFixed(1) : '0.0' }}%</span>
              </div>
            </div>

            <div class="td td-files" @click="handleItemClick(dir)" :class="{ disabled: !hasDeepScanned }">
              {{ dir.is_symlink ? '链接目录' : formatNumber(dir.file_count) }}
            </div>

            <div class="td td-actions">
              <button
                class="action-btn migrate-btn"
                @click.stop="$emit('migrate-dir', dir)"
                :disabled="!hasDeepScanned || dir.is_symlink"
                :title="dir.is_symlink ? '该目录已经迁移为链接占位' : (!hasDeepScanned ? '请先进行深度扫描' : '迁移到其他磁盘')"
              >
                <IconMigrate :size="16" />
              </button>
            </div>
          </div>

          <div
            v-for="file in currentFiles"
            :key="file.path"
            class="table-row table-row-file"
            :class="{ 'row-selected': selectedFiles.has(file.path) }"
          >
            <div class="td td-checkbox" @click.stop>
              <input
                type="checkbox"
                class="checkbox"
                :checked="selectedFiles.has(file.path)"
                :disabled="file.is_symlink"
                @change="toggleFileSelection(file)"
              />
            </div>

            <div class="td td-name">
              <IconFile :size="18" />
              <span class="name-text">{{ file.name }}</span>
              <span v-if="file.is_readonly" class="badge readonly">只读</span>
              <span v-if="file.is_symlink" class="badge symlink" :title="file.link_target || '迁移后的链接占位'">链接</span>
            </div>

            <div class="td td-size">{{ formatBytes(file.size) }}</div>

            <div class="td td-percent">
              <div class="percent-container">
                <div class="percent-bar file-bar" :style="{ width: `${totalSize ? (file.size / totalSize) * 100 : 0}%` }"></div>
                <span class="percent-text">{{ totalSize ? ((file.size / totalSize) * 100).toFixed(1) : '0.0' }}%</span>
              </div>
            </div>

            <div class="td td-files">{{ file.is_symlink ? '链接' : (file.extension || '-') }}</div>

            <div class="td td-actions">
              <button
                class="action-btn migrate-btn"
                @click.stop="$emit('migrate-file', file)"
                :disabled="deepScanning || file.is_symlink"
                :title="file.is_symlink ? '该条目已经迁移为链接占位' : (deepScanning ? '深度扫描完成后可迁移' : '迁移到其他磁盘')"
              >
                <IconMigrate :size="16" />
              </button>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.list-view {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.batch-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
  padding: 0.95rem 1rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(135deg, rgba(15, 118, 110, 0.08), rgba(37, 99, 235, 0.08));
  border: 1px solid rgba(15, 118, 110, 0.12);
}

.batch-copy {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.batch-info {
  font-size: 0.96rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.batch-note {
  font-size: 0.8rem;
  line-height: 1.5;
  color: var(--color-text-secondary);
}

.batch-actions {
  display: flex;
  gap: 0.65rem;
}

.batch-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  border-radius: 0.95rem;
  padding: 0.72rem 1rem;
  border: 1px solid transparent;
  font-size: 0.86rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), border-color var(--transition-base), opacity var(--transition-fast);
}

.batch-migrate-btn {
  background: linear-gradient(135deg, var(--color-accent-primary), var(--color-accent-secondary));
  color: var(--color-text-inverse);
  box-shadow: var(--shadow-sm);
}

.batch-migrate-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

.batch-migrate-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.batch-clear-btn {
  background: rgba(255, 255, 255, 0.74);
  color: var(--color-text-secondary);
  border-color: var(--color-border-light);
}

.batch-clear-btn:hover {
  background: var(--color-surface-hover);
}

.table-shell {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
  overflow: hidden;
}

.table-header,
.table-row {
  display: grid;
  grid-template-columns: 42px minmax(220px, 1fr) minmax(100px, 120px) minmax(120px, 160px) minmax(88px, 112px) 56px;
  gap: 1rem;
  align-items: center;
}

.table-header {
  position: sticky;
  top: 0;
  z-index: 2;
  padding: 0.95rem 1rem;
  background: rgba(244, 239, 232, 0.94);
  border-bottom: 1px solid var(--color-border-light);
  backdrop-filter: blur(18px);
}

.th {
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.table-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0.3rem 0;
}

.table-row {
  margin: 0 0.6rem;
  padding: 0.82rem 0.4rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  transition: transform var(--transition-base), background var(--transition-base), box-shadow var(--transition-base), border-color var(--transition-base), opacity var(--transition-fast);
}

.table-row:hover {
  transform: translateY(-1px);
  background: rgba(255, 255, 255, 0.72);
  border-color: rgba(46, 33, 18, 0.08);
  box-shadow: var(--shadow-xs);
}

.row-selected {
  background: linear-gradient(135deg, rgba(15, 118, 110, 0.1), rgba(37, 99, 235, 0.08));
  border-color: rgba(15, 118, 110, 0.14);
}

.row-disabled {
  opacity: 0.5;
}

.row-disabled:hover {
  transform: none;
}

.table-row-file {
  opacity: 0.92;
}

.td {
  display: flex;
  align-items: center;
  min-width: 0;
  font-size: 0.88rem;
}

.td-checkbox,
.th-checkbox {
  justify-content: center;
}

.checkbox {
  width: 1rem;
  height: 1rem;
  accent-color: var(--color-highlight);
  cursor: pointer;
}

.checkbox:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.td-name {
  gap: 0.65rem;
  color: var(--color-text-primary);
  font-weight: 700;
  cursor: pointer;
}

.td-name.disabled,
.td-size.disabled,
.td-percent.disabled,
.td-files.disabled {
  cursor: not-allowed;
  opacity: 0.56;
}

.name-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-size {
  color: var(--color-text-primary);
  font-weight: 700;
}

.td-percent {
  color: var(--color-text-secondary);
}

.percent-container {
  position: relative;
  width: 100%;
  height: 1.5rem;
  border-radius: 999px;
  background: rgba(23, 23, 23, 0.08);
  overflow: hidden;
}

.percent-bar {
  position: absolute;
  inset: 0 auto 0 0;
  border-radius: inherit;
  background: linear-gradient(90deg, rgba(15, 118, 110, 0.88), rgba(37, 99, 235, 0.7));
}

.percent-bar.file-bar {
  background: linear-gradient(90deg, rgba(15, 159, 110, 0.9), rgba(93, 201, 194, 0.76));
}

.percent-text {
  position: absolute;
  inset: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 0.72rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.td-files {
  color: var(--color-text-secondary);
}

.td-actions {
  justify-content: center;
}

.action-btn {
  width: 2.3rem;
  height: 2.3rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-border-light);
  border-radius: 0.9rem;
  background: rgba(255, 255, 255, 0.7);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), color var(--transition-fast), opacity var(--transition-fast);
}

.migrate-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: linear-gradient(135deg, var(--color-accent-primary), var(--color-accent-secondary));
  color: var(--color-text-inverse);
  box-shadow: var(--shadow-sm);
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.badge {
  padding: 0.22rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: 0.68rem;
  font-weight: 800;
  flex-shrink: 0;
}

.badge.readonly {
  background: rgba(217, 119, 6, 0.12);
  color: #92400e;
}

.badge.symlink {
  background: rgba(37, 99, 235, 0.12);
  color: #1d4ed8;
}

.badge:not(.readonly):not(.symlink) {
  background: rgba(15, 118, 110, 0.12);
  color: var(--color-highlight);
}

.risk-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  margin-left: 0.1rem;
  transition: transform var(--transition-fast);
}

.risk-badge:hover {
  transform: scale(1.08);
}

.risk-safe {
  color: #16a34a;
}

.risk-moderate {
  color: #ca8a04;
}

.risk-risky {
  color: #f97316;
}

.risk-dangerous {
  color: #ef4444;
}

.empty,
.loading {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  padding: 3rem 2rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
}

.empty-icon {
  color: rgba(73, 68, 60, 0.34);
}

.empty h3,
.loading span {
  color: var(--color-text-primary);
}

.empty p {
  color: var(--color-text-tertiary);
}

.spinner {
  width: 1.5rem;
  height: 1.5rem;
  border-radius: 50%;
  border: 2px solid rgba(23, 23, 23, 0.12);
  border-top-color: var(--color-highlight);
  animation: spin 0.85s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 980px) {
  .table-header,
  .table-row {
    grid-template-columns: 42px minmax(180px, 1fr) minmax(90px, 110px) minmax(110px, 140px) 56px;
  }

  .th-files,
  .td-files {
    display: none;
  }

  .batch-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .batch-actions {
    width: 100%;
  }

  .batch-btn {
    flex: 1;
    justify-content: center;
  }
}

@media (max-width: 720px) {
  .table-header,
  .table-row {
    grid-template-columns: 42px minmax(0, 1fr) 56px;
    gap: 0.8rem;
  }

  .th-size,
  .td-size,
  .th-percent,
  .td-percent {
    display: none;
  }

  .table-header {
    padding: 0.82rem;
  }

  .table-row {
    margin: 0 0.45rem;
    padding: 0.76rem 0.32rem;
  }
}
</style>
