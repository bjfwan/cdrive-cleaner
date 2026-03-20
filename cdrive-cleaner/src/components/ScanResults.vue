<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from 'vue';
import { IconArrowLeft, IconInfo, IconDeepScan } from './icons';
import type { DirectoryNode, DiskInfo, FileInfo, ScanResult } from '../types';
import { formatBytes, formatNumber, formatTime } from '../utils/format';

const TreemapView = defineAsyncComponent(() => import('./TreemapView.vue'));
const ListView = defineAsyncComponent(() => import('./ListView.vue'));
const LargeFilesView = defineAsyncComponent(() => import('./LargeFilesView.vue'));
const MigrateDialog = defineAsyncComponent(() => import('./MigrateDialog.vue'));

interface Props {
  result: ScanResult;
  viewMode: 'treemap' | 'list' | 'large-files';
  canGoBack: boolean;
  currentPath: string;
  deepScanning: boolean;
  availableDisks: DiskInfo[];
  hasDeepScanned: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'update:viewMode': [mode: 'treemap' | 'list' | 'large-files'];
  'navigate': [path: string];
  'goBack': [];
  'start-deep-scan': [];
  'migrated': [paths: string[]];
}>();

const showMigrate = ref(false);
const selectedDir = ref<DirectoryNode | null>(null);
const selectedFile = ref<FileInfo | null>(null);
const selectedItems = ref<Array<DirectoryNode | FileInfo>>([]);

const sortedDirectories = computed(() => {
  if (!props.result?.directories || props.result.directories.length === 0) {
    return [];
  }
  return [...props.result.directories].sort((a, b) => b.size - a.size);
});

const sortedLargeFiles = computed(() => {
  if (!props.result?.large_files || props.result.large_files.length === 0) {
    return [];
  }
  return [...props.result.large_files].sort((a, b) => b.size - a.size);
});

const availableTargetDisks = computed(() => {
  const currentDriveLetter = props.currentPath.substring(0, 2);
  return props.availableDisks.filter((disk) => disk.drive_letter !== currentDriveLetter);
});

function showMigrateDialog(dir: DirectoryNode) {
  selectedDir.value = dir;
  selectedFile.value = null;
  selectedItems.value = [];
  showMigrate.value = true;
}

function showMigrateFileDialog(file: FileInfo) {
  selectedFile.value = file;
  selectedDir.value = null;
  selectedItems.value = [];
  showMigrate.value = true;
}

function showBatchMigrateDialog(items: Array<DirectoryNode | FileInfo>) {
  selectedItems.value = items;
  selectedDir.value = null;
  selectedFile.value = null;
  showMigrate.value = true;
}

function closeMigrateDialog() {
  showMigrate.value = false;
  selectedDir.value = null;
  selectedFile.value = null;
  selectedItems.value = [];
}
</script>

<template>
  <div class="results">
    <header class="header">
      <div class="header-main">
        <div class="path-shell">
          <button v-if="canGoBack" @click="emit('goBack')" class="back-btn" title="返回上一级">
            <IconArrowLeft :size="18" />
          </button>

          <div class="path-copy">
            <span class="path-kicker">{{ hasDeepScanned ? 'Deep Snapshot' : 'Quick Snapshot' }}</span>
            <h2>{{ currentPath }}</h2>
            <div class="path-meta">
              <span class="path-badge">{{ hasDeepScanned ? '完整目录树' : '顶层目录概览' }}</span>
              <span v-if="deepScanning" class="path-badge path-badge-live">扫描更新中</span>
            </div>
          </div>
        </div>

        <div class="view-switcher" role="tablist" aria-label="结果视图切换">
          <button class="view-btn" :class="{ active: viewMode === 'treemap' }" @click="$emit('update:viewMode', 'treemap')">
            <span>树状图</span>
            <small>热区</small>
          </button>
          <button class="view-btn" :class="{ active: viewMode === 'list' }" @click="$emit('update:viewMode', 'list')">
            <span>列表</span>
            <small>明细</small>
          </button>
          <button class="view-btn" :class="{ active: viewMode === 'large-files' }" @click="$emit('update:viewMode', 'large-files')">
            <span>大文件</span>
            <small>聚焦</small>
          </button>
        </div>
      </div>

      <div class="metric-grid">
        <div class="metric-card">
          <span class="metric-label">占用体积</span>
          <strong>{{ formatBytes(result.total_size) }}</strong>
          <span class="metric-note">当前路径的累计空间占用</span>
        </div>

        <div class="metric-card">
          <span class="metric-label">文件数量</span>
          <strong>{{ formatNumber(result.total_files) }}</strong>
          <span class="metric-note">纳入当前统计的文件总数</span>
        </div>

        <div class="metric-card">
          <span class="metric-label">目录数量</span>
          <strong>{{ formatNumber(result.total_dirs) }}</strong>
          <span class="metric-note">当前层级下可见目录规模</span>
        </div>

        <div class="metric-card">
          <span class="metric-label">扫描耗时</span>
          <strong>{{ formatTime(result.scan_duration_ms) }}</strong>
          <span class="metric-note">{{ hasDeepScanned ? '深度索引结果' : '快速扫描结果' }}</span>
        </div>
      </div>
    </header>

    <div class="body">
      <div v-if="!hasDeepScanned" class="deep-scan-notice">
        <div class="notice-icon-wrapper">
          <IconInfo :size="24" />
        </div>

        <div class="notice-content">
          <div class="notice-title">深度扫描会把这个页面变成真正可操作的分析台</div>
          <div class="notice-text">
            快速扫描适合先看轮廓；开启深度扫描后，你可以获得完整目录树、更精确的统计，以及迁移能力和回溯能力。
          </div>
        </div>

        <button class="notice-action" :disabled="deepScanning" @click="$emit('start-deep-scan')">
          <IconDeepScan :size="18" />
          <span>{{ deepScanning ? '深度扫描中...' : '开始深度扫描' }}</span>
        </button>
      </div>

      <TreemapView
        v-if="viewMode === 'treemap'"
        :directories="sortedDirectories"
        :total-size="result.total_size"
        :deep-scanning="deepScanning"
        :has-deep-scanned="hasDeepScanned"
        @navigate="$emit('navigate', $event)"
      />

      <ListView
        v-if="viewMode === 'list'"
        :directories="sortedDirectories"
        :total-size="result.total_size"
        :deep-scanning="deepScanning"
        :current-path="currentPath"
        :has-deep-scanned="hasDeepScanned"
        @navigate="$emit('navigate', $event)"
        @migrate-dir="showMigrateDialog"
        @migrate-file="showMigrateFileDialog"
        @batch-migrate="showBatchMigrateDialog"
      />

      <LargeFilesView
        v-if="viewMode === 'large-files'"
        :files="sortedLargeFiles"
        :deep-scanning="deepScanning"
        :has-deep-scanned="hasDeepScanned"
        @migrate-file="showMigrateFileDialog"
      />
    </div>

    <div v-if="result.inaccessible_count > 0" class="notice">
      <IconInfo :size="16" />
      <span>{{ result.inaccessible_count }} 个项目无法访问，结果已自动跳过这些路径。</span>
    </div>

    <MigrateDialog
      :show="showMigrate"
      :selected-dir="selectedDir"
      :selected-file="selectedFile"
      :selected-items="selectedItems"
      :available-disks="availableTargetDisks"
      @close="closeMigrateDialog"
      @migrated="emit('migrated', $event)"
    />
  </div>
</template>

<style scoped>
.results {
  position: relative;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  padding: 1rem;
}

.header {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
  flex-shrink: 0;
}

.header-main {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 1rem;
}

.path-shell {
  display: flex;
  align-items: start;
  gap: 0.85rem;
  min-width: 0;
}

.back-btn {
  width: 2.8rem;
  height: 2.8rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-border-light);
  border-radius: 1rem;
  background: rgba(255, 255, 255, 0.66);
  color: var(--color-text-secondary);
  cursor: pointer;
  box-shadow: var(--shadow-xs);
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), color var(--transition-fast);
}

.back-btn:hover {
  transform: translateX(-2px);
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  box-shadow: var(--shadow-sm);
}

.path-copy {
  min-width: 0;
}

.path-kicker {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  margin-bottom: 0.35rem;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.path-kicker::before {
  content: '';
  width: 0.48rem;
  height: 0.48rem;
  border-radius: 50%;
  background: rgba(15, 118, 110, 0.88);
}

.path-copy h2 {
  font-size: 1.7rem;
  line-height: 0.98;
  word-break: break-word;
}

.path-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.45rem;
  margin-top: 0.7rem;
}

.path-badge {
  padding: 0.42rem 0.72rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.06);
  color: var(--color-text-secondary);
  font-size: 0.76rem;
  font-weight: 700;
}

.path-badge-live {
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
}

.view-switcher {
  display: inline-grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  padding: 0.28rem;
  gap: 0.28rem;
  min-width: 18rem;
  border-radius: 1.05rem;
  background: rgba(23, 23, 23, 0.06);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
}

.view-btn {
  border: none;
  border-radius: 0.85rem;
  padding: 0.72rem 0.9rem;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base), color var(--transition-fast), box-shadow var(--transition-base);
}

.view-btn span,
.view-btn small {
  display: block;
}

.view-btn span {
  font-size: 0.9rem;
  font-weight: 800;
}

.view-btn small {
  margin-top: 0.12rem;
  font-size: 0.72rem;
  color: inherit;
  opacity: 0.74;
}

.view-btn:hover {
  color: var(--color-text-primary);
}

.view-btn.active {
  background: rgba(255, 255, 255, 0.92);
  color: var(--color-accent-primary);
  box-shadow: var(--shadow-xs);
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.85rem;
}

.metric-card {
  padding: 0.95rem 1rem;
  border-radius: var(--radius-md);
  background: rgba(255, 255, 255, 0.6);
  border: 1px solid rgba(46, 33, 18, 0.08);
}

.metric-label {
  display: block;
  margin-bottom: 0.38rem;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.metric-card strong {
  display: block;
  font-size: 1.25rem;
  font-weight: 800;
  letter-spacing: -0.03em;
}

.metric-note {
  display: block;
  margin-top: 0.32rem;
  font-size: 0.78rem;
  line-height: 1.5;
  color: var(--color-text-tertiary);
}

.body {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  padding-top: 1rem;
}

.deep-scan-notice {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 1rem;
  align-items: center;
  margin-bottom: 1rem;
  padding: 1rem 1.1rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.82), rgba(229, 241, 255, 0.8));
  border: 1px solid rgba(37, 99, 235, 0.12);
  box-shadow: var(--shadow-xs);
}

.notice-icon-wrapper {
  width: 3rem;
  height: 3rem;
  border-radius: 1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(37, 99, 235, 0.12);
  color: var(--color-info);
}

.notice-title {
  font-size: 1rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.notice-text {
  margin-top: 0.28rem;
  font-size: 0.86rem;
  line-height: 1.6;
  color: var(--color-text-secondary);
}

.notice-action {
  display: inline-flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.9rem 1.15rem;
  border: none;
  border-radius: 1rem;
  background: linear-gradient(135deg, var(--color-accent-primary), var(--color-accent-secondary));
  color: var(--color-text-inverse);
  font-size: 0.9rem;
  font-weight: 800;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  transition: transform var(--transition-base), box-shadow var(--transition-base), opacity var(--transition-fast);
}

.notice-action:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

.notice-action:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.notice {
  position: absolute;
  left: 50%;
  bottom: 1rem;
  transform: translateX(-50%);
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.78rem 1rem;
  border-radius: var(--radius-pill);
  background: rgba(255, 251, 235, 0.9);
  border: 1px solid rgba(217, 119, 6, 0.16);
  color: #92400e;
  box-shadow: var(--shadow-sm);
  backdrop-filter: blur(18px);
  font-size: 0.84rem;
}

@media (max-width: 1100px) {
  .metric-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 900px) {
  .results {
    padding: 0.8rem;
  }

  .header-main,
  .deep-scan-notice {
    grid-template-columns: 1fr;
    display: grid;
  }

  .view-switcher {
    min-width: 0;
    width: 100%;
  }

  .notice {
    position: static;
    transform: none;
    margin-top: 0.85rem;
  }
}

@media (max-width: 640px) {
  .results,
  .header {
    padding: 0.72rem;
  }

  .path-copy h2 {
    font-size: 1.35rem;
  }

  .metric-grid {
    grid-template-columns: 1fr;
  }

  .view-btn {
    padding: 0.65rem 0.4rem;
  }

  .notice-action {
    width: 100%;
    justify-content: center;
  }
}
</style>
