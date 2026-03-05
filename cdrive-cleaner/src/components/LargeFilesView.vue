<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { IconFile, IconDocument, IconMigrate } from './icons';

interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
  modified_at: string;
  is_readonly: boolean;
}

interface Props {
  files: FileInfo[];
  deepScanning?: boolean;
  hasDeepScanned: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'migrate-file': [file: FileInfo];
}>();

const largeFileThreshold = ref(100); // 默认 100 MB

onMounted(() => {
  loadThreshold();
});

function loadThreshold() {
  const saved = localStorage.getItem('cdrive-cleaner-settings');
  if (saved) {
    try {
      const settings = JSON.parse(saved);
      if (settings.largeFileThreshold) {
        largeFileThreshold.value = settings.largeFileThreshold;
      }
    } catch (e) {
      console.error('Failed to load threshold:', e);
    }
  }
}

// 根据设置的阈值过滤文件
const filteredFiles = computed(() => {
  const thresholdBytes = largeFileThreshold.value * 1024 * 1024;
  return props.files.filter(file => file.size >= thresholdBytes);
});

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
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
</script>

<template>
  <div class="large-files-view">
    <div v-if="!filteredFiles || filteredFiles.length === 0" class="empty">
      <IconDocument class="empty-icon" :size="48" />
      <h3>没有找到大文件</h3>
      <p>扫描中未发现大于 {{ largeFileThreshold }}MB 的文件</p>
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
          v-for="file in filteredFiles" 
          :key="file.path"
          class="table-row"
        >
          <div class="td td-name">
            <IconFile :size="18" />
            <span :title="file.name">{{ file.name }}</span>
          </div>
          <div class="td td-path" :title="file.path">{{ file.path }}</div>
          <div class="td td-size">{{ formatBytes(file.size) }}</div>
          <div class="td td-modified">{{ formatDate(file.modified_at) }}</div>
          <div class="td td-actions">
            <button class="action-btn migrate-btn" @click.stop="$emit('migrate-file', file)" :disabled="!hasDeepScanned" :title="!hasDeepScanned ? '请先进行深度扫描' : '迁移到其他磁盘'">
              <IconMigrate :size="16" />
            </button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.large-files-view {
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

.table-header {
  display: grid;
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 50px;
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
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 50px;
  gap: 1.5rem;
  padding: 1rem 0;
  border-bottom: 1px solid #f5f5f4;
  transition: all 0.2s ease;
}

.table-row:hover {
  background: linear-gradient(to right, rgba(0, 122, 255, 0.03) 0%, transparent 100%);
  margin: 0 -2rem;
  padding-left: 2rem;
  padding-right: 2rem;
  border-left: 2px solid #007aff;
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

.td-path {
  color: #78716c;
  font-size: 0.875rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-size {
  color: #2c2c2c;
  font-weight: 500;
}

.td-modified {
  color: #78716c;
  font-size: 0.875rem;
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

.migrate-btn:hover {
  background: #007aff;
  color: white;
  transform: translateY(-2px);
  box-shadow: 0 4px 8px rgba(0, 122, 255, 0.2);
}

@media (max-width: 900px) {
  .table-header,
  .table-row {
    grid-template-columns: 1fr 100px 120px;
    gap: 1rem;
  }

  .th-path,
  .td-path,
  .th-modified,
  .td-modified {
    display: none;
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
}
</style>
