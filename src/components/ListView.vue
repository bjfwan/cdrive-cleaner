<script setup lang="ts">
import { computed, markRaw, shallowRef, watch } from 'vue';
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
import { useSelectionSet } from '../composables/useSelectionSet';
import VirtualList from './VirtualList.vue';

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

// 图标组件 markRaw 化，避免每行 v-for 实例化时被 Vue 重复 reactive 包装。
const ICON_FOLDER = markRaw(IconFolder);
const ICON_FILE = markRaw(IconFile);
const ICON_MIGRATE = markRaw(IconMigrate);
const ICON_RISK_SAFE = markRaw(IconRiskSafe);
const ICON_RISK_LOW = markRaw(IconRiskLow);
const ICON_RISK_MEDIUM = markRaw(IconRiskMedium);
const ICON_RISK_DANGER = markRaw(IconRiskDanger);
const ICON_RISK_UNKNOWN = markRaw(IconRiskUnknown);

interface DirRow {
  kind: 'dir';
  key: string;
  dir: DirectoryNode;
  selectable: boolean;
}
interface FileRow {
  kind: 'file';
  key: string;
  file: FileInfo;
  selectable: boolean;
}
type Row = DirRow | FileRow;

const loadingFiles = shallowRef(false);
const currentFiles = shallowRef<FileInfo[]>([]);
const selectedDirs = useSelectionSet<string>();
const selectedFiles = useSelectionSet<string>();
let fileLoadRequestId = 0;

function isDirSelectable(dir: DirectoryNode) {
  return !dir.is_symlink && (!props.deepScanning || dir.has_children);
}

// 将 directories + currentFiles 合并为统一行结构，供 VirtualList 渲染。
const rows = computed<Row[]>(() => {
  const dirs = props.directories;
  const files = currentFiles.value;
  const total = dirs.length + files.length;
  const out: Row[] = new Array(total);
  for (let i = 0; i < dirs.length; i++) {
    const d = dirs[i];
    out[i] = {
      kind: 'dir',
      key: 'd:' + d.path,
      dir: d,
      selectable: isDirSelectable(d),
    };
  }
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    out[dirs.length + i] = {
      kind: 'file',
      key: 'f:' + f.path,
      file: f,
      selectable: !f.is_symlink,
    };
  }
  return out;
});

const selectableDirCount = computed(() => {
  let n = 0;
  for (const d of props.directories) if (isDirSelectable(d)) n += 1;
  return n;
});

const selectableFileCount = computed(() => {
  let n = 0;
  for (const f of currentFiles.value) if (!f.is_symlink) n += 1;
  return n;
});

const allSelectableCount = computed(() => selectableDirCount.value + selectableFileCount.value);

const selectedTotal = computed(() => selectedDirs.size + selectedFiles.size);

const hasSelection = computed(() => selectedTotal.value > 0);

const allSelected = computed(
  () => allSelectableCount.value > 0 && selectedTotal.value === allSelectableCount.value,
);

function selectedItems(): Array<DirectoryNode | FileInfo> {
  const items: Array<DirectoryNode | FileInfo> = [];
  const dirSet = selectedDirs.value;
  const fileSet = selectedFiles.value;
  for (const dir of props.directories) {
    if (dirSet.has(dir.path)) items.push(dir);
  }
  for (const file of currentFiles.value) {
    if (fileSet.has(file.path)) items.push(file);
  }
  return items;
}

function toggleDirSelection(dir: DirectoryNode) {
  selectedDirs.toggle(dir.path);
}

function toggleFileSelection(file: FileInfo) {
  selectedFiles.toggle(file.path);
}

function selectAll() {
  const dirKeys: string[] = [];
  for (const d of props.directories) {
    if (isDirSelectable(d)) dirKeys.push(d.path);
  }
  selectedDirs.addAll(dirKeys);

  const fileKeys: string[] = [];
  for (const f of currentFiles.value) {
    if (!f.is_symlink) fileKeys.push(f.path);
  }
  selectedFiles.addAll(fileKeys);
}

function clearSelection() {
  selectedDirs.clear();
  selectedFiles.clear();
}

function handleBatchMigrate() {
  if (!hasSelection.value) return;
  emit('batch-migrate', selectedItems());
  clearSelection();
}

function handleItemClick(dir: DirectoryNode) {
  if (!props.hasDeepScanned) return;
  emit('navigate', dir.path);
}

async function loadDirectoryFiles(path: string) {
  const requestId = ++fileLoadRequestId;
  loadingFiles.value = true;

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const files = await invoke<FileInfo[]>('scan_directory_files', { path });
    if (requestId === fileLoadRequestId) {
      currentFiles.value = files;
    }
  } catch {
    if (requestId === fileLoadRequestId) {
      currentFiles.value = [];
      showToast('加载文件失败', '无法读取当前目录的文件列表', 'error');
    }
  } finally {
    if (requestId === fileLoadRequestId) {
      loadingFiles.value = false;
    }
  }
}

watch(
  () => [props.currentPath, props.totalSize] as const,
  ([newPath]) => {
    void loadDirectoryFiles(newPath);
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

function riskIconFor(verdict?: string) {
  switch (verdict) {
    case 'safe':
      return ICON_RISK_SAFE;
    case 'safe_after_action':
      return ICON_RISK_LOW;
    case 'blocked':
      return ICON_RISK_MEDIUM;
    case 'system_critical':
      return ICON_RISK_DANGER;
    default:
      return ICON_RISK_UNKNOWN;
  }
}

function dirSelected(path: string): boolean {
  return selectedDirs.has(path);
}

function fileSelected(path: string): boolean {
  return selectedFiles.has(path);
}

function pctText(size: number): string {
  return props.totalSize ? ((size / props.totalSize) * 100).toFixed(1) : '0.0';
}

function pctWidth(size: number): string {
  return props.totalSize ? ((size / props.totalSize) * 100) + '%' : '0%';
}
</script>

<template>
  <div class="list-view">
    <div v-if="rows.length === 0 && !loadingFiles" class="empty">
      <component :is="ICON_FOLDER" class="empty-icon" :size="44" />
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
          <span class="batch-info">已选择 {{ selectedTotal }} 项</span>
          <span class="batch-note">{{ hasDeepScanned ? '可以直接发起批量迁移' : '完成深度扫描后可启用批量迁移' }}</span>
        </div>

        <div class="batch-actions">
          <button class="batch-btn batch-migrate-btn" @click="handleBatchMigrate" :disabled="!hasDeepScanned">
            <component :is="ICON_MIGRATE" :size="16" />
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
          <VirtualList
            :items="rows"
            :item-size="56"
            :buffer="6"
            v-slot="{ item: row }"
          >
            <!-- 目录行 -->
            <div
              v-if="(row as Row).kind === 'dir'"
              :key="(row as DirRow).key"
              class="table-row"
              :class="{
                'row-disabled': !(row as DirRow).selectable,
                'row-selected': dirSelected((row as DirRow).dir.path),
                'row-link': (row as DirRow).dir.is_symlink,
              }"
            >
              <div class="td td-checkbox" @click.stop>
                <input
                  type="checkbox"
                  class="checkbox"
                  :checked="dirSelected((row as DirRow).dir.path)"
                  :disabled="!(row as DirRow).selectable"
                  @change="toggleDirSelection((row as DirRow).dir)"
                />
              </div>

              <div
                class="td td-name"
                @click="handleItemClick((row as DirRow).dir)"
                :class="{ disabled: !hasDeepScanned }"
                :title="!hasDeepScanned ? '请先进行深度扫描以查看子目录' : ''"
              >
                <component :is="ICON_FOLDER" :size="18" />
                <span class="name-text">{{ (row as DirRow).dir.name }}</span>
                <span
                  v-if="(row as DirRow).dir.is_symlink"
                  class="badge symlink"
                  :title="(row as DirRow).dir.link_target || '迁移后的链接占位'"
                >链接</span>
                <span
                  v-if="(row as DirRow).dir.safety"
                  class="risk-badge"
                  :class="getVerdictClass((row as DirRow).dir.safety!.verdict)"
                  :title="`${getVerdictLabel((row as DirRow).dir.safety!.verdict)} - ${(row as DirRow).dir.safety!.app_type}`"
                >
                  <component :is="riskIconFor((row as DirRow).dir.safety!.verdict)" :size="16" />
                </span>
                <span v-if="deepScanning && !(row as DirRow).dir.has_children" class="badge">扫描中</span>
              </div>

              <div
                class="td td-size"
                @click="handleItemClick((row as DirRow).dir)"
                :class="{ disabled: !hasDeepScanned }"
              >
                {{ formatBytes((row as DirRow).dir.size) }}
              </div>

              <div
                class="td td-percent"
                @click="handleItemClick((row as DirRow).dir)"
                :class="{ disabled: !hasDeepScanned }"
              >
                <div class="percent-container">
                  <div class="percent-bar" :style="{ width: pctWidth((row as DirRow).dir.size) }"></div>
                  <span class="percent-text">{{ pctText((row as DirRow).dir.size) }}%</span>
                </div>
              </div>

              <div
                class="td td-files"
                @click="handleItemClick((row as DirRow).dir)"
                :class="{ disabled: !hasDeepScanned }"
              >
                {{ (row as DirRow).dir.is_symlink ? '链接目录' : formatNumber((row as DirRow).dir.file_count) }}
              </div>

              <div class="td td-actions">
                <button
                  class="action-btn migrate-btn"
                  @click.stop="$emit('migrate-dir', (row as DirRow).dir)"
                  :disabled="!hasDeepScanned || (row as DirRow).dir.is_symlink"
                  :title="(row as DirRow).dir.is_symlink ? '该目录已经迁移为链接占位' : (!hasDeepScanned ? '请先进行深度扫描' : '迁移到其他磁盘')"
                >
                  <component :is="ICON_MIGRATE" :size="16" />
                </button>
              </div>
            </div>

            <!-- 文件行 -->
            <div
              v-else
              :key="(row as FileRow).key"
              class="table-row table-row-file"
              :class="{ 'row-selected': fileSelected((row as FileRow).file.path) }"
            >
              <div class="td td-checkbox" @click.stop>
                <input
                  type="checkbox"
                  class="checkbox"
                  :checked="fileSelected((row as FileRow).file.path)"
                  :disabled="(row as FileRow).file.is_symlink"
                  @change="toggleFileSelection((row as FileRow).file)"
                />
              </div>

              <div class="td td-name">
                <component :is="ICON_FILE" :size="18" />
                <span class="name-text">{{ (row as FileRow).file.name }}</span>
                <span v-if="(row as FileRow).file.is_readonly" class="badge readonly">只读</span>
                <span
                  v-if="(row as FileRow).file.is_symlink"
                  class="badge symlink"
                  :title="(row as FileRow).file.link_target || '迁移后的链接占位'"
                >链接</span>
              </div>

              <div class="td td-size">{{ formatBytes((row as FileRow).file.size) }}</div>

              <div class="td td-percent">
                <div class="percent-container">
                  <div class="percent-bar file-bar" :style="{ width: pctWidth((row as FileRow).file.size) }"></div>
                  <span class="percent-text">{{ pctText((row as FileRow).file.size) }}%</span>
                </div>
              </div>

              <div class="td td-files">
                {{ (row as FileRow).file.is_symlink ? '链接' : ((row as FileRow).file.extension || '-') }}
              </div>

              <div class="td td-actions">
                <button
                  class="action-btn migrate-btn"
                  @click.stop="$emit('migrate-file', (row as FileRow).file)"
                  :disabled="deepScanning || (row as FileRow).file.is_symlink"
                  :title="(row as FileRow).file.is_symlink ? '该条目已经迁移为链接占位' : (deepScanning ? '深度扫描完成后可迁移' : '迁移到其他磁盘')"
                >
                  <component :is="ICON_MIGRATE" :size="16" />
                </button>
              </div>
            </div>
          </VirtualList>
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
  position: relative;
  z-index: 2;
  padding: 0.95rem 1rem;
  background: rgba(244, 239, 232, 0.94);
  border-bottom: 1px solid var(--color-border-light);
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
  padding: 0.3rem 0;
}

.table-row {
  margin: 0 0.6rem;
  padding: 0.82rem 0.4rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  height: 56px;
  box-sizing: border-box;
  transition: background var(--transition-base), box-shadow var(--transition-base), border-color var(--transition-base), opacity var(--transition-fast);
}

.table-row:hover {
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
}

.risk-safe {
  color: #16a34a;
}

.risk-warning {
  color: #ca8a04;
}

.risk-blocked {
  color: #f97316;
}

.risk-danger {
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
