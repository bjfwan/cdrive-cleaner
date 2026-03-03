<script setup lang="ts">
import { ref, watch } from 'vue';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
}

interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
  is_readonly: boolean;
}

interface Props {
  directories: DirectoryNode[];
  totalSize: number;
  deepScanning: boolean;
  currentPath: string;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'navigate': [path: string];
  'migrate-dir': [dir: DirectoryNode];
  'migrate-file': [file: FileInfo];
  'open-file': [file: FileInfo];
}>();

const loadingFiles = ref(false);
const currentFiles = ref<FileInfo[]>([]);

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

function handleItemClick(dir: DirectoryNode) {
  if (props.deepScanning && (!dir.children || dir.children.length === 0)) {
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
</script>

<template>
  <div class="list-view">
    <div v-if="directories.length === 0 && currentFiles.length === 0 && !loadingFiles" class="empty">
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" class="empty-icon">
        <path d="M8 12C8 9.79086 9.79086 8 12 8H20L24 12H36C38.2091 12 40 13.7909 40 16V36C40 38.2091 38.2091 40 36 40H12C9.79086 40 8 38.2091 8 36V12Z" stroke="currentColor" stroke-width="2"/>
        <path d="M18 24H30M24 18V30" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      </svg>
      <h3>此目录为空</h3>
      <p>没有子目录和文件</p>
    </div>
    
    <div v-if="loadingFiles" class="loading">
      <div class="spinner"></div>
      <span>加载文件中...</span>
    </div>
    
    <template v-else-if="directories.length > 0 || currentFiles.length > 0">
      <div class="table-header">
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
          :class="{ 'row-disabled': deepScanning && (!dir.children || dir.children.length === 0) }"
        >
          <div class="td td-name" @click="handleItemClick(dir)">
            <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
              <path d="M2 4.5C2 3.67157 2.67157 3 3.5 3H6L7.5 4.5H14.5C15.3284 4.5 16 5.17157 16 6V13.5C16 14.3284 15.3284 15 14.5 15H3.5C2.67157 15 2 14.3284 2 13.5V4.5Z" fill="#2c2c2c"/>
            </svg>
            <span>{{ dir.name }}</span>
            <span v-if="deepScanning && (!dir.children || dir.children.length === 0)" class="badge">扫描中</span>
          </div>
          <div class="td td-size" @click="handleItemClick(dir)">{{ formatBytes(dir.size) }}</div>
          <div class="td td-percent" @click="handleItemClick(dir)">
            <div class="percent-container">
              <div class="percent-bar" :style="{ width: `${(dir.size / totalSize) * 100}%` }"></div>
              <span class="percent-text">{{ ((dir.size / totalSize) * 100).toFixed(1) }}%</span>
            </div>
          </div>
          <div class="td td-files" @click="handleItemClick(dir)">{{ formatNumber(dir.file_count) }}</div>
          <div class="td td-actions">
            <button class="action-btn migrate-btn" @click.stop="$emit('migrate-dir', dir)" title="迁移到其他磁盘">
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
            <span v-if="file.is_readonly" class="badge readonly">只读</span>
          </div>
          <div class="td td-size">{{ formatBytes(file.size) }}</div>
          <div class="td td-percent">
            <div class="percent-container">
              <div class="percent-bar file-bar" :style="{ width: `${(file.size / totalSize) * 100}%` }"></div>
              <span class="percent-text">{{ ((file.size / totalSize) * 100).toFixed(1) }}%</span>
            </div>
          </div>
          <div class="td td-files">{{ file.extension || '-' }}</div>
          <div class="td td-actions">
            <button class="action-btn open-btn" @click.stop="$emit('open-file', file)" title="打开文件">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path d="M14 9V13C14 13.5523 13.5523 14 13 14H3C2.44772 14 2 13.5523 2 13V9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
                <path d="M8 2V10M8 10L5 7M8 10L11 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </button>
            <button class="action-btn migrate-btn" @click.stop="$emit('migrate-file', file)" title="迁移到其他磁盘">
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
</template>

<style scoped>
.list-view {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
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
  transition: all 0.2s ease;
}

.open-btn:hover {
  background: #10b981;
  color: white;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(16, 185, 129, 0.2);
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
    grid-template-columns: 1fr 100px 120px 80px;
    gap: 1rem;
  }

  .table-row:hover {
    margin: 0 -1.5rem;
    padding-left: 1.5rem;
    padding-right: 1.5rem;
  }
}

@media (max-width: 600px) {
  .table-header,
  .table-row {
    grid-template-columns: 1fr 90px;
    gap: 0.75rem;
  }

  .th-percent,
  .td-percent,
  .th-files,
  .td-files {
    display: none;
  }
}
</style>
