<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { IconRiskSafe, IconRiskLow, IconRiskMedium, IconRiskDanger, IconRiskUnknown, IconFolder, IconFile, IconMigrate } from './icons';
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
  'navigate': [path: string];
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
  props.directories.forEach(dir => {
    if (selectedDirs.value.has(dir.path)) {
      items.push(dir);
    }
  });
  currentFiles.value.forEach(file => {
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
  return !dir.is_symlink && (!props.deepScanning || (dir.children && dir.children.length > 0));
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
  
  selectableDirs.value.forEach(dir => {
    if (isDirSelectable(dir)) {
      newDirSet.add(dir.path);
    }
  });
  selectableFiles.value.forEach(file => {
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
  if (hasSelection.value) {
    emit('batch-migrate', selectedItems.value);
    clearSelection();
  }
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
    const files = await invoke<FileInfo[]>('scan_directory_files', { path });
    currentFiles.value = files;
  } catch {
    currentFiles.value = [];
    showToast('加载文件失败', '无法读取当前目录的文件列表', 'error');
  } finally {
    loadingFiles.value = false;
  }
}

watch(() => [props.currentPath, props.totalSize] as const, ([newPath]) => {
  loadDirectoryFiles(newPath);
  clearSelection();
}, { immediate: true });

function getRiskLabel(riskLevel?: string): string {
  switch (riskLevel) {
    case 'safe': return '安全';
    case 'moderate': return '低风险';
    case 'risky': return '中风险';
    case 'dangerous': return '危险';
    default: return '未知';
  }
}

function getRiskClass(riskLevel?: string): string {
  switch (riskLevel) {
    case 'safe': return 'risk-safe';
    case 'moderate': return 'risk-moderate';
    case 'risky': return 'risk-risky';
    case 'dangerous': return 'risk-dangerous';
    default: return '';
  }
}
</script>

<template>
  <div class="list-view">
    <div v-if="directories.length === 0 && currentFiles.length === 0 && !loadingFiles" class="empty">
      <IconFolder class="empty-icon" :size="48" />
      <h3>此目录为空</h3>
      <p>没有子目录和文件</p>
    </div>
    
    <div v-if="loadingFiles" class="loading">
      <div class="spinner"></div>
      <span>加载文件中...</span>
    </div>
    
    <template v-else-if="directories.length > 0 || currentFiles.length > 0">
      <div v-if="hasSelection" class="batch-toolbar">
        <span class="batch-info">已选择 {{ selectedItems.length }} 项</span>
        <div class="batch-actions">
          <button class="batch-btn batch-migrate-btn" @click="handleBatchMigrate" :disabled="!hasDeepScanned">
            <IconMigrate :size="16" />
            批量迁移
          </button>
          <button class="batch-btn batch-clear-btn" @click="clearSelection">清除选择</button>
        </div>
      </div>
      
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
          :class="{ 'row-disabled': !isDirSelectable(dir), 'row-selected': selectedDirs.has(dir.path), 'row-link': dir.is_symlink }"
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
          <div class="td td-name" @click="handleItemClick(dir)" :class="{ 'disabled': !hasDeepScanned }" :title="!hasDeepScanned ? '请先进行深度扫描以查看子目录' : ''">
            <IconFolder :size="18" />
            <span>{{ dir.name }}</span>
            <span v-if="dir.is_symlink" class="badge symlink" :title="dir.link_target || '迁移后的链接占位'">链接</span>
            <span v-if="dir.safety" class="risk-badge" :class="getRiskClass(dir.safety.risk_level)" :title="`${getRiskLabel(dir.safety.risk_level)} - ${dir.safety.app_type}`">
              <IconRiskSafe v-if="dir.safety.risk_level === 'safe'" :size="16" />
              <IconRiskLow v-else-if="dir.safety.risk_level === 'moderate'" :size="16" />
              <IconRiskMedium v-else-if="dir.safety.risk_level === 'risky'" :size="16" />
              <IconRiskDanger v-else-if="dir.safety.risk_level === 'dangerous'" :size="16" />
              <IconRiskUnknown v-else :size="16" />
            </span>
            <span v-if="deepScanning && (!dir.children || dir.children.length === 0)" class="badge">扫描中</span>
          </div>
          <div class="td td-size" @click="handleItemClick(dir)" :class="{ 'disabled': !hasDeepScanned }">{{ formatBytes(dir.size) }}</div>
          <div class="td td-percent" @click="handleItemClick(dir)" :class="{ 'disabled': !hasDeepScanned }">
            <div class="percent-container">
              <div class="percent-bar" :style="{ width: `${(dir.size / totalSize) * 100}%` }"></div>
              <span class="percent-text">{{ ((dir.size / totalSize) * 100).toFixed(1) }}%</span>
            </div>
          </div>
          <div class="td td-files" @click="handleItemClick(dir)" :class="{ 'disabled': !hasDeepScanned }">{{ dir.is_symlink ? '链接目录' : formatNumber(dir.file_count) }}</div>
          <div class="td td-actions">
            <button class="action-btn migrate-btn" @click.stop="$emit('migrate-dir', dir)" :disabled="!hasDeepScanned || dir.is_symlink" :title="dir.is_symlink ? '该目录已经迁移为链接占位' : (!hasDeepScanned ? '请先进行深度扫描' : '迁移到其他磁盘')">
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
            <span>{{ file.name }}</span>
            <span v-if="file.is_readonly" class="badge readonly">只读</span>
            <span v-if="file.is_symlink" class="badge symlink" :title="file.link_target || '迁移后的链接占位'">链接</span>
          </div>
          <div class="td td-size">{{ formatBytes(file.size) }}</div>
          <div class="td td-percent">
            <div class="percent-container">
              <div class="percent-bar file-bar" :style="{ width: `${(file.size / totalSize) * 100}%` }"></div>
              <span class="percent-text">{{ ((file.size / totalSize) * 100).toFixed(1) }}%</span>
            </div>
          </div>
          <div class="td td-files">{{ file.is_symlink ? '链接' : (file.extension || '-') }}</div>
          <div class="td td-actions">
            <button class="action-btn migrate-btn" @click.stop="$emit('migrate-file', file)" :disabled="deepScanning || file.is_symlink" :title="file.is_symlink ? '该条目已经迁移为链接占位' : (deepScanning ? '深度扫描完成后可迁移' : '迁移到其他磁盘')">
              <IconMigrate :size="16" />
            </button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.list-view {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.batch-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.875rem 1.25rem;
  background: linear-gradient(135deg, #eff6ff 0%, #f0f9ff 100%);
  border: 1px solid #bfdbfe;
  border-radius: 10px;
  margin-bottom: 1rem;
  flex-shrink: 0;
  animation: slideDown 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}

@keyframes slideDown {
  from {
    opacity: 0;
    transform: translateY(-10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.batch-info {
  font-size: 0.9375rem;
  font-weight: 600;
  color: #1e40af;
}

.batch-actions {
  display: flex;
  gap: 0.75rem;
}

.batch-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.batch-migrate-btn {
  background: #007aff;
  color: white;
}

.batch-migrate-btn:hover:not(:disabled) {
  background: #0051d5;
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(0, 122, 255, 0.3);
}

.batch-migrate-btn:active:not(:disabled) {
  transform: translateY(0);
}

.batch-migrate-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.batch-clear-btn {
  background: white;
  color: #78716c;
  border: 1px solid #e7e5e4;
}

.batch-clear-btn:hover {
  background: #f5f5f4;
  border-color: #d6d3d1;
}

.batch-clear-btn:active {
  transform: scale(0.98);
}

.checkbox {
  width: 18px;
  height: 18px;
  cursor: pointer;
  accent-color: #007aff;
  transition: transform 0.15s cubic-bezier(0.4, 0, 0.2, 1);
}

.checkbox:hover:not(:disabled) {
  transform: scale(1.1);
}

.checkbox:active:not(:disabled) {
  transform: scale(0.95);
}

.checkbox:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.td-checkbox {
  justify-content: center;
  cursor: pointer;
}

.row-selected {
  background: linear-gradient(90deg, rgba(0, 122, 255, 0.08) 0%, rgba(0, 122, 255, 0.03) 100%);
  transition: background 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 4rem 2rem;
}

.empty-icon {
  color: #d6d3d1;
  margin-bottom: 1.5rem;
}

.empty h3 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.5rem;
}

.empty p {
  font-size: 0.9375rem;
  color: #78716c;
}

.loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 3rem 2rem;
  color: #78716c;
  font-size: 0.9375rem;
}

.spinner {
  width: 20px;
  height: 20px;
  border: 2px solid #e7e5e4;
  border-top-color: #007aff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.table-header {
  display: grid;
  grid-template-columns: 40px minmax(200px, 1fr) minmax(100px, 140px) minmax(120px, 160px) minmax(80px, 120px) 50px;
  gap: 1.5rem;
  padding: 0 0 1rem;
  border-bottom: 1px solid #e7e5e4;
  flex-shrink: 0;
}

.th {
  font-size: 0.75rem;
  font-weight: 600;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.table-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.table-row {
  display: grid;
  grid-template-columns: 40px minmax(200px, 1fr) minmax(100px, 140px) minmax(120px, 160px) minmax(80px, 120px) 50px;
  gap: 1.5rem;
  padding: 1rem 0;
  border-bottom: 1px solid #f5f5f4;
  transition: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
  cursor: pointer;
  will-change: transform, background;
}

.table-row:hover {
  background: linear-gradient(to right, rgba(0, 122, 255, 0.03) 0%, transparent 100%);
  margin: 0 -2rem;
  padding-left: 2rem;
  padding-right: 2rem;
  border-left: 2px solid #007aff;
}

.table-row.row-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.table-row.row-disabled:hover {
  background: none;
  margin: 0;
  padding: 1rem 0;
  border-left: none;
}

.table-row-file {
  opacity: 0.85;
  cursor: default;
}

.table-row-file:hover {
  opacity: 1;
  background: linear-gradient(to right, rgba(16, 185, 129, 0.03) 0%, transparent 100%);
  margin: 0 -2rem;
  padding-left: 2rem;
  padding-right: 2rem;
  border-left: 2px solid #10b981;
}

.td {
  display: flex;
  align-items: center;
  font-size: 0.9375rem;
}

.td-name {
  gap: 0.75rem;
  font-weight: 500;
  color: #2c2c2c;
  overflow: hidden;
}

.td-name.disabled,
.td-size.disabled,
.td-percent.disabled,
.td-files.disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.td-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.badge {
  padding: 0.125rem 0.5rem;
  background: #fef3c7;
  color: #92400e;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
  flex-shrink: 0;
}

.badge.readonly {
  background: #fef3c7;
  color: #92400e;
}

.badge.symlink {
  background: #dbeafe;
  color: #1d4ed8;
}

.td-size {
  color: #2c2c2c;
  font-weight: 500;
}

.td-percent {
  color: #78716c;
}

.percent-container {
  position: relative;
  width: 100%;
  height: 24px;
  background: #f5f5f4;
  border-radius: 6px;
  overflow: hidden;
}

.percent-bar {
  position: absolute;
  left: 0;
  top: 0;
  height: 100%;
  background: linear-gradient(90deg, #007aff 0%, #5856d6 100%);
  border-radius: 6px;
  transition: width 0.3s ease;
}

.percent-bar.file-bar {
  background: linear-gradient(90deg, #10b981 0%, #059669 100%);
}

.percent-text {
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  transform: translateY(-50%);
  text-align: center;
  font-size: 0.75rem;
  font-weight: 600;
  color: #2c2c2c;
  z-index: 1;
}

.td-files {
  color: #78716c;
}

.td-actions {
  justify-content: center;
  gap: 0.5rem;
}

.action-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border: none;
  border-radius: 6px;
  color: #78716c;
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
}

.action-btn:active {
  transform: scale(0.95);
}

.migrate-btn:hover {
  background: #007aff;
  color: white;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(0, 122, 255, 0.2);
}

@media (max-width: 900px) {
  .table-header,
  .table-row {
    grid-template-columns: 40px 1fr 100px 120px 50px;
    gap: 1rem;
  }

  .table-row:hover {
    margin: 0 -1.5rem;
    padding-left: 1.5rem;
    padding-right: 1.5rem;
  }

  .batch-toolbar {
    flex-direction: column;
    align-items: flex-start;
    gap: 0.75rem;
  }

  .batch-actions {
    width: 100%;
  }

  .batch-btn {
    flex: 1;
  }
}

@media (max-width: 600px) {
  .table-header,
  .table-row {
    grid-template-columns: 40px 1fr 50px;
    gap: 0.75rem;
  }

  .th-percent,
  .td-percent,
  .th-files,
  .td-files {
    display: none;
  }
}

/* 风险等级徽章 */
.risk-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  margin-left: 6px;
  cursor: help;
  transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.risk-badge:hover {
  transform: scale(1.2);
}

.risk-safe {
  filter: drop-shadow(0 0 2px rgba(34, 197, 94, 0.3));
}

.risk-moderate {
  filter: drop-shadow(0 0 2px rgba(234, 179, 8, 0.3));
}

.risk-risky {
  filter: drop-shadow(0 0 2px rgba(249, 115, 22, 0.3));
}

.risk-dangerous {
  filter: drop-shadow(0 0 2px rgba(239, 68, 68, 0.3));
}
</style>
