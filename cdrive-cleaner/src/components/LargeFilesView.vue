<script setup lang="ts">
interface FileInfo {
  path: string;
  name: string;
  size: number;
  modified_at: string;
}

interface Props {
  files: FileInfo[];
}

defineProps<Props>();
const emit = defineEmits<{
  'migrate-file': [file: FileInfo];
  'open-file': [file: FileInfo];
}>();

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
    <div v-if="!files || files.length === 0" class="empty">
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
          v-for="file in files" 
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
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 80px;
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
  grid-template-columns: minmax(150px, 1fr) minmax(200px, 2fr) minmax(100px, 140px) minmax(120px, 160px) 80px;
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
