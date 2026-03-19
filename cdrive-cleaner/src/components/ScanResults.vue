<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from 'vue';
import { IconArrowLeft, IconInfo, IconDeepScan } from './icons';
import type { DirectoryNode, FileInfo, DiskInfo, ScanResult } from '../types';
import { formatBytes, formatNumber } from '../utils/format';

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
  return props.availableDisks.filter(disk => disk.drive_letter !== currentDriveLetter);
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
      <div class="header-info">
        <div class="breadcrumb">
          <button 
            v-if="canGoBack"
            @click="emit('goBack')"
            class="back-btn"
          >
            <IconArrowLeft :size="20" />
          </button>
          <h2>{{ currentPath }}</h2>
        </div>
        <div class="stats">
          <span class="stat-item">{{ formatBytes(result.total_size) }}</span>
          <span class="separator">·</span>
          <span class="stat-item">{{ formatNumber(result.total_files) }} 个文件</span>
          <span class="separator">·</span>
          <span class="stat-item">{{ formatNumber(result.total_dirs) }} 个目录</span>
        </div>
      </div>
      <div class="view-switcher">
        <button 
          class="view-btn"
          :class="{ active: viewMode === 'treemap' }"
          @click="$emit('update:viewMode', 'treemap')"
        >
          树状图
        </button>
        <button 
          class="view-btn"
          :class="{ active: viewMode === 'list' }"
          @click="$emit('update:viewMode', 'list')"
        >
          列表
        </button>
        <button 
          class="view-btn"
          :class="{ active: viewMode === 'large-files' }"
          @click="$emit('update:viewMode', 'large-files')"
        >
          大文件
        </button>
      </div>
    </header>

    <div class="body">
      <div v-if="!hasDeepScanned" class="deep-scan-notice">
        <div class="notice-icon-wrapper">
          <IconInfo :size="28" />
        </div>
        <div class="notice-content">
          <div class="notice-title">深度扫描解锁完整功能</div>
          <div class="notice-text">
            快速扫描仅显示顶层目录概览。点击顶部「深度扫描」按钮，即可查看完整目录树、精确文件统计，并启用迁移功能。
          </div>
        </div>
        <button class="notice-action" @click="$emit('start-deep-scan')">
          <IconDeepScan :size="18" />
          <span>开始深度扫描</span>
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
      {{ result.inaccessible_count }} 个项目无法访问
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
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

.header {
  padding: 1.5rem 2rem;
  border-bottom: 1px solid #e7e5e4;
  background: linear-gradient(to bottom, #ffffff 0%, #fafaf9 100%);
  flex-shrink: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1.5rem;
  flex-wrap: wrap;
}

.header-info h2 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.5rem;
  letter-spacing: -0.01em;
}

.breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 0.5rem;
}

.back-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border: none;
  border-radius: 8px;
  color: #2c2c2c;
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.back-btn:hover {
  background: #e7e5e4;
  color: #007aff;
}

.stats {
  font-size: 0.875rem;
  color: #78716c;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.separator {
  color: #d6d3d1;
}

.view-switcher {
  display: flex;
  gap: 0.5rem;
  background: #f5f5f4;
  padding: 0.25rem;
  border-radius: 10px;
  flex-shrink: 0;
}

.view-btn {
  padding: 0.5rem 1.25rem;
  font-size: 0.875rem;
  font-weight: 500;
  background: transparent;
  color: #78716c;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.view-btn:hover {
  color: #007aff;
  background: rgba(0, 122, 255, 0.05);
}

.view-btn.active {
  background: white;
  color: #007aff;
  box-shadow: 0 2px 8px rgba(0, 122, 255, 0.15);
}

.body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 2rem;
  overflow: hidden;
}

.notice {
  position: absolute;
  bottom: 1rem;
  left: 50%;
  transform: translateX(-50%);
  padding: 0.75rem 1.25rem;
  background: rgba(255, 251, 235, 0.95);
  color: #92400e;
  border: 1px solid #fde68a;
  border-radius: 8px;
  font-size: 0.875rem;
  backdrop-filter: blur(8px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  z-index: 10;
}

@media (max-width: 900px) {
  .header {
    flex-direction: column;
    align-items: flex-start;
    padding: 1.25rem 1.5rem;
  }

  .view-switcher {
    width: 100%;
  }

  .view-btn {
    flex: 1;
  }

  .body {
    padding: 1.5rem;
  }

  .notice {
    display: none;
  }
}

@media (max-width: 600px) {
  .header-info h2 {
    font-size: 1.125rem;
  }

  .stats {
    font-size: 0.8125rem;
  }

  .body {
    padding: 1rem;
  }
}

.deep-scan-notice {
  display: flex;
  align-items: center;
  gap: 1.25rem;
  padding: 1.75rem 2rem;
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.04) 0%, rgba(37, 99, 235, 0.02) 100%);
  border: 1px solid rgba(59, 130, 246, 0.15);
  border-radius: var(--radius-lg);
  margin-bottom: 1.5rem;
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.06);
  transition: all var(--transition-base);
  position: relative;
  overflow: hidden;
}

.deep-scan-notice::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  width: 4px;
  height: 100%;
  background: linear-gradient(to bottom, var(--color-info) 0%, #2563eb 100%);
}

.deep-scan-notice:hover {
  border-color: rgba(59, 130, 246, 0.25);
  box-shadow: 0 4px 16px rgba(59, 130, 246, 0.12);
  transform: translateY(-1px);
}

.notice-icon-wrapper {
  flex-shrink: 0;
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, rgba(59, 130, 246, 0.1) 0%, rgba(37, 99, 235, 0.08) 100%);
  border: 1px solid rgba(59, 130, 246, 0.2);
  border-radius: var(--radius-md);
  color: var(--color-info);
}

.notice-content {
  flex: 1;
  min-width: 0;
}

.notice-title {
  font-family: var(--font-serif);
  font-size: 1rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.5rem;
  letter-spacing: -0.01em;
}

.notice-text {
  font-size: 0.9375rem;
  color: var(--color-text-secondary);
  line-height: 1.6;
}

.notice-action {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  font-weight: 600;
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-info) 0%, #2563eb 100%);
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: -0.01em;
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.25);
}

.notice-action:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 16px rgba(59, 130, 246, 0.35);
}

.notice-action:active {
  transform: translateY(0);
}
</style>
