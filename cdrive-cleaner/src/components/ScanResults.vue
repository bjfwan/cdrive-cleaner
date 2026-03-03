<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import VChart from 'vue-echarts';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
  is_symlink: boolean;
  link_target?: string;
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
const loadingFiles = ref(false);
const currentFiles = ref<FileInfo[]>([]);
const targetDisk = ref<string>('');
const migrating = ref(false);
const migrationError = ref<string>('');

console.log('ScanResults 接收到的数据:', props.result);
console.log('directories 数量:', props.result?.directories?.length || 0);
if (props.result?.directories?.length > 0) {
  console.log('第一个目录:', props.result.directories[0]);
}

function handleItemClick(dir: DirectoryNode) {
  if (props.deepScanning && (!dir.children || dir.children.length === 0)) {
    return;
  }
  emit('navigate', dir.path);
}

function handleChartClick(params: any) {
  if (params.data && params.data.path) {
    if (props.deepScanning && params.data.hasChildren === false) {
      return;
    }
    emit('navigate', params.data.path);
  }
}

function showMigrateDialog(dir: DirectoryNode) {
  selectedDir.value = dir;
  selectedFile.value = null;
  targetDisk.value = '';
  showMigrate.value = true;
}

function closeMigrateDialog() {
  showMigrate.value = false;
  selectedDir.value = null;
  selectedFile.value = null;
  targetDisk.value = '';
  migrating.value = false;
  migrationError.value = '';
}

async function startMigration() {
  if (!targetDisk.value) {
    migrationError.value = '请选择目标磁盘';
    return;
  }

  const sourcePath = selectedFile.value?.path || selectedDir.value?.path;
  if (!sourcePath) {
    migrationError.value = '未选择要迁移的项目';
    return;
  }

  migrating.value = true;
  migrationError.value = '';

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const result = await invoke('migrate_file', {
      source: sourcePath,
      targetDisk: targetDisk.value,
      linkType: null
    });
    
    console.log('迁移成功:', result);
    alert('迁移成功！');
    closeMigrateDialog();
  } catch (err) {
    console.error('迁移失败:', err);
    migrationError.value = String(err);
  } finally {
    migrating.value = false;
  }
}

const treemapOption = computed(() => {
  const colors = [
    '#007aff', '#5856d6', '#af52de', '#ff2d55', 
    '#ff3b30', '#ff9500', '#ffcc00', '#34c759',
    '#00c7be', '#30b0c7', '#32ade6'
  ];

  const directories = props.result?.directories || [];
  console.log('treemapOption: directories 数量', directories.length);

  return {
    tooltip: {
      formatter: (info: any) => {
        const status = props.deepScanning && !info.data.hasChildren ? '<br/>(扫描中...)' : '';
        return `${info.name}<br/>${formatBytes(info.value)}${status}`;
      },
      backgroundColor: 'rgba(255, 255, 255, 0.95)',
      borderColor: '#e7e5e4',
      borderWidth: 1,
      textStyle: {
        color: '#2c2c2c',
        fontSize: 13
      }
    },
    series: [{
      type: 'treemap',
      left: 0,
      right: 0,
      top: 0,
      bottom: 0,
      squareRatio: 0.6,
      leafDepth: 1,
      visibleMin: 100,
      data: directories.map((dir, index) => ({
        name: dir.name,
        value: dir.size,
        path: dir.path,
        hasChildren: dir.children && dir.children.length > 0,
        itemStyle: {
          color: colors[index % colors.length],
          opacity: props.deepScanning && (!dir.children || dir.children.length === 0) ? 0.5 : 1
        }
      })),
      roam: false,
      nodeClick: 'link',
      breadcrumb: { show: false },
      label: {
        show: true,
        formatter: (params: any) => {
          const area = params.width * params.height;
          if (area < 1500) return '';
          if (area < 4000) return params.name.substring(0, 8);
          return params.name;
        },
        fontSize: 12,
        color: '#fff',
        fontWeight: 500,
        overflow: 'truncate'
      },
      upperLabel: {
        show: true,
        height: 32,
        color: '#fff',
        fontSize: 13,
        fontWeight: 600
      },
      itemStyle: {
        borderColor: '#fff',
        borderWidth: 3,
        gapWidth: 3,
        shadowBlur: 8,
        shadowColor: 'rgba(0, 0, 0, 0.1)'
      },
      emphasis: {
        itemStyle: {
          shadowBlur: 12,
          shadowColor: 'rgba(0, 0, 0, 0.2)'
        }
      },
      levels: [
        {
          itemStyle: {
            borderWidth: 0,
            gapWidth: 5
          }
        }
      ]
    }]
  };
});

const sortedDirectories = computed(() => {
  if (!props.result?.directories || props.result.directories.length === 0) {
    console.warn('sortedDirectories: directories 为空或未定义');
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

function formatDate(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    return date.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit'
    });
  } catch {
    return dateStr;
  }
}

function showMigrateFileDialog(file: FileInfo) {
  selectedFile.value = file;
  selectedDir.value = null;
  targetDisk.value = '';
  showMigrate.value = true;
}

async function openFile(file: FileInfo) {
  try {
    const { open } = await import('@tauri-apps/plugin-opener');
    await open(file.path);
  } catch (err) {
    console.error('打开文件失败:', err);
    alert('无法打开文件');
  }
}

async function loadDirectoryFiles(path: string) {
  loadingFiles.value = true;
  console.log('[调试] 开始加载目录文件:', path);
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const files = await invoke<FileInfo[]>('scan_directory_files', { path });
    console.log('[调试] 加载到', files.length, '个文件');
    currentFiles.value = files;
  } catch (err) {
    console.error('加载文件失败:', err);
    currentFiles.value = [];
  } finally {
    loadingFiles.value = false;
  }
}

watch(() => props.currentPath, (newPath) => {
  loadDirectoryFiles(newPath);
}, { immediate: true });

watch(() => props.viewMode, (newMode) => {
  if (newMode === 'list') {
    loadDirectoryFiles(props.currentPath);
  }
});
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
      <div v-if="viewMode === 'treemap'" class="view-treemap">
        <div class="treemap-card">
          <div class="treemap-container">
            <v-chart 
              :option="treemapOption" 
              class="chart" 
              autoresize 
              @click="handleChartClick"
            />
          </div>
        </div>
        
        <div class="list-preview">
          <h3 class="preview-title">最大的目录</h3>
          <div class="preview-items">
            <div 
              v-for="(dir, index) in sortedDirectories.slice(0, 10)" 
              :key="dir.path"
              class="preview-item"
              @click="handleItemClick(dir)"
            >
              <div class="preview-rank">{{ index + 1 }}</div>
              <div class="preview-info">
                <div class="preview-name">{{ dir.name }}</div>
                <div class="preview-bar-container">
                  <div 
                    class="preview-bar" 
                    :style="{ width: `${(dir.size / result.total_size) * 100}%` }"
                  ></div>
                </div>
              </div>
              <div class="preview-size">{{ formatBytes(dir.size) }}</div>
            </div>
          </div>
        </div>
      </div>

      <div v-if="viewMode === 'list'" class="view-list">
        <div v-if="sortedDirectories.length === 0 && currentFiles.length === 0 && !loadingFiles" class="empty-list">
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none" class="empty-icon">
            <path d="M8 12C8 9.79086 9.79086 8 12 8H20L24 12H36C38.2091 12 40 13.7909 40 16V36C40 38.2091 38.2091 40 36 40H12C9.79086 40 8 38.2091 8 36V12Z" stroke="currentColor" stroke-width="2"/>
            <path d="M18 24H30M24 18V30" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          </svg>
          <h3>此目录为空</h3>
          <p>没有子目录和文件</p>
        </div>
        
        <div v-if="loadingFiles" class="loading-files">
          <div class="spinner-small"></div>
          <span>加载文件中...</span>
        </div>
        
        <template v-else-if="sortedDirectories.length > 0 || currentFiles.length > 0">
          <div class="table-header">
            <div class="th th-name">名称</div>
            <div class="th th-size">大小</div>
            <div class="th th-percent">占比</div>
            <div class="th th-files">文件数/类型</div>
            <div class="th th-actions">操作</div>
          </div>
          <div class="table-body">
          <div 
            v-for="dir in sortedDirectories" 
            :key="dir.path"
            class="table-row"
            :class="{ 'row-disabled': deepScanning && (!dir.children || dir.children.length === 0) }"
          >
            <div class="td td-name" @click="handleItemClick(dir)">
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path d="M2 4.5C2 3.67157 2.67157 3 3.5 3H6L7.5 4.5H14.5C15.3284 4.5 16 5.17157 16 6V13.5C16 14.3284 15.3284 15 14.5 15H3.5C2.67157 15 2 14.3284 2 13.5V4.5Z" fill="#2c2c2c"/>
              </svg>
              <span>{{ dir.name }}</span>
              <span v-if="deepScanning && (!dir.children || dir.children.length === 0)" class="scanning-badge">扫描中</span>
            </div>
            <div class="td td-size" @click="handleItemClick(dir)">{{ formatBytes(dir.size) }}</div>
            <div class="td td-percent" @click="handleItemClick(dir)">
              <div class="percent-bar-container">
                <div 
                  class="percent-bar" 
                  :style="{ width: `${(dir.size / result.total_size) * 100}%` }"
                ></div>
                <span class="percent-text">{{ ((dir.size / result.total_size) * 100).toFixed(1) }}%</span>
              </div>
            </div>
            <div class="td td-files" @click="handleItemClick(dir)">{{ formatNumber(dir.file_count) }}</div>
            <div class="td td-actions">
              <button 
                class="migrate-btn"
                @click.stop="showMigrateDialog(dir)"
                title="迁移到其他磁盘"
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                  <path d="M8 2L12 6H9V10H7V6H4L8 2Z" fill="currentColor"/>
                  <path d="M3 12H13V14H3V12Z" fill="currentColor"/>
                </svg>
              </button>
            </div>
          </div>
          
          <div 
            v-for="file in currentFiles" 
            :key="file.path"
            class="table-row table-row-file"
          >
            <div class="td td-name">
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path d="M4 2H10L14 6V14C14 15.1046 13.1046 16 12 16H4C2.89543 16 2 15.1046 2 14V4C2 2.89543 2.89543 2 4 2Z" fill="#78716c"/>
                <path d="M10 2V6H14" stroke="#78716c" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
              <span>{{ file.name }}</span>
              <span v-if="file.is_readonly" class="readonly-badge">只读</span>
            </div>
            <div class="td td-size">{{ formatBytes(file.size) }}</div>
            <div class="td td-percent">
              <div class="percent-bar-container">
                <div 
                  class="percent-bar percent-bar-file" 
                  :style="{ width: `${(file.size / result.total_size) * 100}%` }"
                ></div>
                <span class="percent-text">{{ ((file.size / result.total_size) * 100).toFixed(1) }}%</span>
              </div>
            </div>
            <div class="td td-files">{{ file.extension || '-' }}</div>
            <div class="td td-actions">
              <button 
                class="action-btn open-btn"
                @click.stop="openFile(file)"
                title="打开文件"
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                  <path d="M14 9V13C14 13.5523 13.5523 14 13 14H3C2.44772 14 2 13.5523 2 13V9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                  <path d="M8 2V10M8 10L5 7M8 10L11 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
              </button>
              <button 
                class="action-btn migrate-btn"
                @click.stop="showMigrateFileDialog(file)"
                title="迁移到其他磁盘"
              >
                <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                  <path d="M8 2L12 6H9V10H7V6H4L8 2Z" fill="currentColor"/>
                  <path d="M3 12H13V14H3V12Z" fill="currentColor"/>
                </svg>
              </button>
            </div>
          </div>
        </div>
        </template>
      </div>

      <div v-if="viewMode === 'large-files'" class="view-large-files">
        <div v-if="!result.large_files || result.large_files.length === 0" class="empty-list">
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none" class="empty-icon">
            <rect x="10" y="8" width="28" height="32" rx="2" stroke="currentColor" stroke-width="2"/>
            <path d="M16 16H32M16 22H32M16 28H24" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          </svg>
          <h3>没有找到大文件</h3>
          <p>扫描中未发现大于 100MB 的文件</p>
        </div>
        
        <template v-else>
          <div class="table-header">
            <div class="th th-name">文件名</div>
            <div class="th th-path">路径</div>
            <div class="th th-size">大小</div>
            <div class="th th-modified">修改时间</div>
            <div class="th th-actions">操作</div>
          </div>
          <div class="table-body">
            <div 
              v-for="file in sortedLargeFiles" 
              :key="file.path"
              class="table-row"
            >
              <div class="td td-name">
                <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                  <path d="M4 2H10L14 6V14C14 15.1046 13.1046 16 12 16H4C2.89543 16 2 15.1046 2 14V4C2 2.89543 2.89543 2 4 2Z" fill="#78716c"/>
                  <path d="M10 2V6H14" stroke="#78716c" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                <span :title="file.name">{{ file.name }}</span>
              </div>
              <div class="td td-path" :title="file.path">{{ file.path }}</div>
              <div class="td td-size">{{ formatBytes(file.size) }}</div>
              <div class="td td-modified">{{ formatDate(file.modified_at) }}</div>
              <div class="td td-actions">
                <button 
                  class="action-btn open-btn"
                  @click.stop="openFile(file)"
                  title="打开文件"
                >
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M14 9V13C14 13.5523 13.5523 14 13 14H3C2.44772 14 2 13.5523 2 13V9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                    <path d="M8 2V10M8 10L5 7M8 10L11 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                  </svg>
                </button>
                <button 
                  class="action-btn migrate-btn"
                  @click.stop="showMigrateFileDialog(file)"
                  title="迁移到其他磁盘"
                >
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path d="M8 2L12 6H9V10H7V6H4L8 2Z" fill="currentColor"/>
                    <path d="M3 12H13V14H3V12Z" fill="currentColor"/>
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>

    <div v-if="result.inaccessible_count > 0" class="notice">
      {{ result.inaccessible_count }} 个项目无法访问
    </div>

    <div v-if="showMigrate" class="migrate-dialog-overlay" @click="closeMigrateDialog">
      <div class="migrate-dialog" @click.stop>
        <div class="dialog-header">
          <h3>{{ selectedFile ? '迁移文件' : '迁移目录' }}</h3>
          <button class="close-btn" @click="closeMigrateDialog">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M15 5L5 15M5 5L15 15" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
          </button>
        </div>
        <div class="dialog-body">
          <div class="info-section">
            <div class="info-row">
              <span class="info-label">源路径：</span>
              <span class="info-value">{{ selectedFile?.path || selectedDir?.path }}</span>
            </div>
            <div class="info-row">
              <span class="info-label">大小：</span>
              <span class="info-value">{{ formatBytes(selectedFile?.size || selectedDir?.size || 0) }}</span>
            </div>
            <div v-if="selectedDir" class="info-row">
              <span class="info-label">文件数：</span>
              <span class="info-value">{{ formatNumber(selectedDir?.file_count || 0) }}</span>
            </div>
            <div v-if="selectedFile" class="info-row">
              <span class="info-label">类型：</span>
              <span class="info-value">{{ selectedFile?.extension || '无扩展名' }}</span>
            </div>
            <div v-if="selectedFile" class="info-row">
              <span class="info-label">修改时间：</span>
              <span class="info-value">{{ formatDate(selectedFile?.modified_at || '') }}</span>
            </div>
          </div>
          <div class="form-section">
            <label class="form-label">目标磁盘</label>
            <select class="form-select" v-model="targetDisk">
              <option value="">选择目标磁盘...</option>
              <option 
                v-for="disk in availableTargetDisks" 
                :key="disk.drive_letter"
                :value="disk.drive_letter + '\\'"
              >
                {{ disk.drive_letter }} - {{ disk.label }} (可用: {{ formatBytes(disk.free_space) }})
              </option>
            </select>
          </div>
          <div class="warning-section">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 2L2 17H18L10 2Z" stroke="#ff9500" stroke-width="2" stroke-linejoin="round"/>
              <path d="M10 8V12" stroke="#ff9500" stroke-width="2" stroke-linecap="round"/>
              <circle cx="10" cy="15" r="0.5" fill="#ff9500"/>
            </svg>
            <span>迁移后将在原位置创建符号链接，程序可正常访问</span>
          </div>
          <div v-if="migrationError" class="error-section">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <circle cx="10" cy="10" r="8" stroke="#ef4444" stroke-width="2"/>
              <path d="M10 6V10M10 14H10.01" stroke="#ef4444" stroke-width="2" stroke-linecap="round"/>
            </svg>
            <span>{{ migrationError }}</span>
          </div>
        </div>
        <div class="dialog-footer">
          <button class="btn btn-secondary" @click="closeMigrateDialog" :disabled="migrating">取消</button>
          <button class="btn btn-primary" @click="startMigration" :disabled="!targetDisk || migrating">
            <span v-if="migrating">迁移中...</span>
            <span v-else>开始迁移</span>
          </button>
        </div>
      </div>
    </div>
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

.view-treemap {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 380px;
  gap: 2rem;
  overflow: hidden;
}

.treemap-card {
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 16px;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  min-height: 400px;
}

.treemap-container {
  width: 100%;
  height: 100%;
  position: relative;
  padding: 1.5rem;
}

.chart {
  position: absolute;
  top: 1.5rem;
  left: 1.5rem;
  right: 1.5rem;
  bottom: 1.5rem;
  width: calc(100% - 3rem);
  height: calc(100% - 3rem);
}

.list-preview {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.preview-title {
  font-size: 1rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 1rem;
  letter-spacing: -0.01em;
}

.preview-items {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.preview-item {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.875rem;
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.preview-item:hover {
  border-color: #007aff;
  box-shadow: 0 2px 8px rgba(0, 122, 255, 0.1);
  transform: translateX(4px);
}

.preview-rank {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 600;
  color: #78716c;
  flex-shrink: 0;
}

.preview-info {
  flex: 1;
  min-width: 0;
}

.preview-name {
  font-size: 0.875rem;
  font-weight: 500;
  color: #2c2c2c;
  margin-bottom: 0.375rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-bar-container {
  height: 4px;
  background: #f5f5f4;
  border-radius: 2px;
  overflow: hidden;
}

.preview-bar {
  height: 100%;
  background: linear-gradient(90deg, #007aff 0%, #5856d6 100%);
  border-radius: 2px;
  transition: width 0.3s ease;
}

.preview-size {
  font-size: 0.875rem;
  font-weight: 600;
  color: #2c2c2c;
  flex-shrink: 0;
}

.view-list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.view-large-files {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.view-large-files .table-header {
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 80px;
}

.view-large-files .table-row {
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 80px;
}

.td-path {
  color: #78716c;
  font-size: 0.875rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-modified {
  color: #78716c;
  font-size: 0.875rem;
}

.empty-list {
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

.empty-list h3 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.5rem;
}

.empty-list p {
  font-size: 0.9375rem;
  color: #78716c;
}

.table-header {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) minmax(100px, 140px) minmax(120px, 160px) minmax(80px, 120px) 80px;
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
  grid-template-columns: minmax(200px, 1fr) minmax(100px, 140px) minmax(120px, 160px) minmax(80px, 120px) 80px;
  gap: 1.5rem;
  padding: 1rem 0;
  border-bottom: 1px solid #f5f5f4;
  transition: all 0.2s ease;
  cursor: pointer;
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

.loading-files {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 3rem 2rem;
  color: #78716c;
  font-size: 0.9375rem;
}

.readonly-badge {
  padding: 0.125rem 0.5rem;
  background: #fef3c7;
  color: #92400e;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
}

.percent-bar-file {
  background: linear-gradient(90deg, #10b981 0%, #059669 100%);
}

.scanning-badge {
  padding: 0.125rem 0.5rem;
  background: #fef3c7;
  color: #92400e;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
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

.td-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-size {
  color: #2c2c2c;
  font-weight: 500;
}

.td-percent {
  color: #78716c;
}

.percent-bar-container {
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
  transition: all 0.2s ease;
}

.open-btn:hover {
  background: #10b981;
  color: white;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(16, 185, 129, 0.2);
}

.migrate-btn {
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
  transition: all 0.2s ease;
}

.migrate-btn:hover {
  background: #007aff;
  color: white;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(0, 122, 255, 0.2);
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

.migrate-dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}

.migrate-dialog {
  background: white;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  width: 90%;
  max-width: 500px;
  max-height: 90vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.dialog-header {
  padding: 1.5rem 2rem;
  border-bottom: 1px solid #e7e5e4;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.dialog-header h3 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin: 0;
}

.close-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border: none;
  border-radius: 8px;
  color: #78716c;
  cursor: pointer;
  transition: all 0.2s ease;
}

.close-btn:hover {
  background: #e7e5e4;
  color: #2c2c2c;
}

.dialog-body {
  padding: 2rem;
  overflow-y: auto;
  flex: 1;
}

.info-section {
  background: #fafaf9;
  border-radius: 12px;
  padding: 1.25rem;
  margin-bottom: 1.5rem;
}

.info-row {
  display: flex;
  justify-content: space-between;
  padding: 0.5rem 0;
  font-size: 0.9375rem;
}

.info-row:not(:last-child) {
  border-bottom: 1px solid #e7e5e4;
}

.info-label {
  color: #78716c;
  font-weight: 500;
}

.info-value {
  color: #2c2c2c;
  font-weight: 600;
}

.form-section {
  margin-bottom: 1.5rem;
}

.form-label {
  display: block;
  font-size: 0.875rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.5rem;
}

.form-select {
  width: 100%;
  padding: 0.75rem 1rem;
  font-size: 0.9375rem;
  background: white;
  border: 1.5px solid #e7e5e4;
  border-radius: 10px;
  color: #2c2c2c;
  cursor: pointer;
  transition: all 0.2s ease;
}

.form-select:hover {
  border-color: #007aff;
}

.form-select:focus {
  outline: none;
  border-color: #007aff;
  box-shadow: 0 0 0 3px rgba(0, 122, 255, 0.1);
}

.warning-section {
  display: flex;
  gap: 0.75rem;
  padding: 1rem;
  background: #fff7ed;
  border: 1px solid #fed7aa;
  border-radius: 10px;
  font-size: 0.875rem;
  color: #92400e;
}

.error-section {
  display: flex;
  gap: 0.75rem;
  padding: 1rem;
  background: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: 10px;
  font-size: 0.875rem;
  color: #991b1b;
}

.dialog-footer {
  padding: 1.5rem 2rem;
  border-top: 1px solid #e7e5e4;
  display: flex;
  gap: 1rem;
  justify-content: flex-end;
}

.btn {
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  font-weight: 500;
  border: none;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.btn-secondary {
  background: #f5f5f4;
  color: #78716c;
}

.btn-secondary:hover {
  background: #e7e5e4;
  color: #2c2c2c;
}

.btn-primary {
  background: linear-gradient(135deg, #007aff 0%, #0051d5 100%);
  color: white;
  box-shadow: 0 4px 12px rgba(0, 122, 255, 0.25);
}

.btn-primary:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 16px rgba(0, 122, 255, 0.35);
}

.btn-primary:active {
  transform: translateY(0);
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

  .view-treemap {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr auto;
    gap: 1.5rem;
  }

  .treemap-card {
    min-height: 350px;
  }

  .list-preview {
    max-height: 300px;
  }

  .treemap-container {
    padding: 1rem;
  }

  .chart {
    top: 1rem;
    left: 1rem;
    right: 1rem;
    bottom: 1rem;
    width: calc(100% - 2rem);
    height: calc(100% - 2rem);
  }

  .table-header,
  .table-row {
    grid-template-columns: 1fr 100px 120px 80px;
    gap: 1rem;
  }

  .view-large-files .table-header,
  .view-large-files .table-row {
    grid-template-columns: 1fr 100px 120px;
    gap: 1rem;
  }

  .view-large-files .th-path,
  .view-large-files .td-path,
  .view-large-files .th-modified,
  .view-large-files .td-modified {
    display: none;
  }

  .table-row:hover {
    margin: 0 -1.5rem;
    padding-left: 1.5rem;
    padding-right: 1.5rem;
  }

  .notice {
    bottom: 0.75rem;
    font-size: 0.8125rem;
    padding: 0.625rem 1rem;
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

  .view-treemap {
    gap: 1rem;
  }

  .treemap-card {
    min-height: 250px;
  }

  .list-preview {
    max-height: 250px;
  }

  .preview-item {
    padding: 0.75rem;
    gap: 0.75rem;
  }

  .preview-rank {
    width: 24px;
    height: 24px;
    font-size: 0.8125rem;
  }

  .preview-name {
    font-size: 0.8125rem;
  }

  .preview-size {
    font-size: 0.8125rem;
  }

  .treemap-container {
    padding: 0.75rem;
  }

  .chart {
    top: 0.75rem;
    left: 0.75rem;
    right: 0.75rem;
    bottom: 0.75rem;
    width: calc(100% - 1.5rem);
    height: calc(100% - 1.5rem);
  }

  .table-header,
  .table-row {
    grid-template-columns: 1fr 90px;
    gap: 0.75rem;
  }

  .view-large-files .table-header,
  .view-large-files .table-row {
    grid-template-columns: 1fr 90px;
    gap: 0.75rem;
  }

  .view-large-files .th-path,
  .view-large-files .td-path,
  .view-large-files .th-modified,
  .view-large-files .td-modified {
    display: none;
  }

  .th-percent,
  .td-percent,
  .th-files,
  .td-files {
    display: none;
  }
}
</style>
