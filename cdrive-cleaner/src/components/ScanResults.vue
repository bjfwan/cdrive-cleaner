<script setup lang="ts">
import { computed, ref } from 'vue';
import TreemapView from './TreemapView.vue';
import ListView from './ListView.vue';
import LargeFilesView from './LargeFilesView.vue';
import MigrateDialog from './MigrateDialog.vue';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
  is_symlink: boolean;
  link_target?: string;
  safety?: {
    risk_level: 'safe' | 'moderate' | 'risky' | 'dangerous';
    safety_score: number;
    can_migrate: boolean;
    reasons: string[];
    recommendations: string[];
    app_type: string;
  };
}

interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
  modified_at: string;
  is_readonly: boolean;
}

interface DiskInfo {
  drive_letter: string;
  label: string;
  file_system: string;
  total_space: number;
  free_space: number;
  used_space: number;
  usage_percent: number;
}

interface ScanResult {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: DirectoryNode[];
  large_files: FileInfo[];
  inaccessible_count: number;
}

interface Props {
  result: ScanResult;
  viewMode: 'treemap' | 'list' | 'large-files';
  canGoBack: boolean;
  currentPath: string;
  deepScanning: boolean;
  availableDisks: DiskInfo[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'update:viewMode': [mode: 'treemap' | 'list' | 'large-files'];
  'navigate': [path: string];
  'goBack': [];
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

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatNumber(num: number): string {
  return num.toLocaleString('zh-CN');
}

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

async function openFile(file: FileInfo) {
  try {
    const { openPath } = await import('@tauri-apps/plugin-opener');
    await openPath(file.path);
  } catch (err) {
    console.error('打开文件失败:', err);
    alert('无法打开文件');
  }
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
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M12.5 15L7.5 10L12.5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
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
      <TreemapView
        v-if="viewMode === 'treemap'"
        :directories="sortedDirectories"
        :total-size="result.total_size"
        :deep-scanning="deepScanning"
        @navigate="$emit('navigate', $event)"
      />

      <ListView
        v-if="viewMode === 'list'"
        :directories="sortedDirectories"
        :total-size="result.total_size"
        :deep-scanning="deepScanning"
        :current-path="currentPath"
        @navigate="$emit('navigate', $event)"
        @migrate-dir="showMigrateDialog"
        @migrate-file="showMigrateFileDialog"
        @batch-migrate="showBatchMigrateDialog"
        @open-file="openFile"
      />

      <LargeFilesView
        v-if="viewMode === 'large-files'"
        :files="sortedLargeFiles"
        :deep-scanning="deepScanning"
        @migrate-file="showMigrateFileDialog"
        @open-file="openFile"
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
</style>
