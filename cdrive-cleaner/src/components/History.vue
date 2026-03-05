<template>
  <div class="history-container">
    <div class="history-header">
      <div class="header-content">
        <h1>迁移历史</h1>
        <p class="header-subtitle">查看和管理所有文件迁移记录</p>
      </div>
      <button @click="refreshHistory" class="refresh-btn" :disabled="loading">
        <IconRefresh :size="16" :spinning="loading" />
        <span>刷新</span>
      </button>
    </div>

    <div v-if="stats" class="stats-grid">
      <div class="stat-card">
        <div class="stat-icon total">
          <IconChart :size="20" />
        </div>
        <div class="stat-content">
          <div class="stat-value">{{ stats.total_count }}</div>
          <div class="stat-label">总迁移数</div>
        </div>
      </div>
      
      <div class="stat-card">
        <div class="stat-icon size">
          <IconDisk :size="20" />
        </div>
        <div class="stat-content">
          <div class="stat-value">{{ formatSize(stats.total_size) }}</div>
          <div class="stat-label">总迁移大小</div>
        </div>
      </div>
      
      <div class="stat-card active">
        <div class="stat-icon">
          <IconSuccess :size="20" />
        </div>
        <div class="stat-content">
          <div class="stat-value">{{ stats.active_count }}</div>
          <div class="stat-label">活跃迁移</div>
        </div>
      </div>
      
      <div class="stat-card rolled">
        <div class="stat-icon">
          <IconRollback :size="20" />
        </div>
        <div class="stat-content">
          <div class="stat-value">{{ stats.rolled_back_count }}</div>
          <div class="stat-label">已回滚</div>
        </div>
      </div>
    </div>

    <div v-if="loading" class="loading-state">
      <div class="loading-spinner"></div>
      <p>加载中...</p>
    </div>
    
    <div v-else-if="records.length === 0" class="empty-state">
      <IconDocument class="empty-icon" :size="64" />
      <h3>暂无迁移记录</h3>
      <p>开始迁移文件后，记录将显示在这里</p>
    </div>

    <div v-else class="records-list">
      <div 
        v-for="record in records" 
        :key="record.id" 
        class="record-card"
        :class="{ 'is-rolled-back': record.status === 'rolled_back' }"
      >
        <div class="record-header">
          <div class="record-meta">
            <span class="record-id">ID {{ record.id }}</span>
            <span class="record-dot">·</span>
            <span class="record-time">{{ formatDate(record.created_at) }}</span>
          </div>
          <div class="record-badge" :class="record.status">
            <span class="badge-dot"></span>
            <span>{{ record.status === 'active' ? '活跃' : '已回滚' }}</span>
          </div>
        </div>
        
        <div class="record-paths">
          <div class="path-row">
            <div class="path-label">源路径</div>
            <div class="path-value">{{ record.source_path }}</div>
          </div>
          <div class="path-arrow">
            <IconArrowRight :size="20" />
          </div>
          <div class="path-row">
            <div class="path-label">目标路径</div>
            <div class="path-value">{{ record.target_path }}</div>
          </div>
        </div>

        <div class="record-details">
          <div class="detail-item">
            <IconSizeIcon :size="16" />
            <span>{{ formatSize(record.file_size) }}</span>
          </div>
          <div class="detail-item">
            <IconDocument :size="16" />
            <span>{{ record.link_type }}</span>
          </div>
        </div>

        <div v-if="record.status === 'active'" class="record-actions">
          <button 
            @click="confirmRollback(record)"
            class="rollback-btn"
            :disabled="rollingBack === record.id"
          >
            <IconRollback :size="16" />
            <span v-if="rollingBack !== record.id">回滚</span>
            <span v-else>回滚中...</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="showConfirm" class="modal-overlay" @click="showConfirm = false">
      <div class="modal-dialog" @click.stop>
        <button class="modal-close" @click="showConfirm = false">
          <IconClose :size="20" />
        </button>
        
        <div class="modal-header">
          <div class="modal-icon">
            <IconWarning :size="24" />
          </div>
          <h3>确认回滚操作</h3>
          <p>此操作将撤销迁移并恢复原始文件位置</p>
        </div>
        
        <div class="modal-content">
          <div class="info-section">
            <div class="info-row">
              <span class="info-label">源路径</span>
              <span class="info-value">{{ selectedRecord?.source_path }}</span>
            </div>
            <div class="info-row">
              <span class="info-label">目标路径</span>
              <span class="info-value">{{ selectedRecord?.target_path }}</span>
            </div>
          </div>
          
          <div class="warning-box">
            <IconWarning :size="20" />
            <div>
              <div class="warning-title">注意事项</div>
              <div class="warning-text">将删除符号链接并将文件从目标位置复制回源位置</div>
            </div>
          </div>
        </div>
        
        <div class="modal-actions">
          <button @click="showConfirm = false" class="btn-secondary">取消</button>
          <button @click="executeRollback" class="btn-danger">确认回滚</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { 
  IconRefresh, IconChart, IconSizeIcon, IconSuccess, IconRollback, 
  IconDocument, IconArrowRight, IconDisk, IconClose, IconWarning 
} from './icons'

interface MigrationRecord {
  id: number
  source_path: string
  target_path: string
  link_type: string
  file_size: number
  created_at: string
  status: string
}

interface MigrationStats {
  total_count: number
  total_size: number
  active_count: number
  rolled_back_count: number
}

const records = ref<MigrationRecord[]>([])
const stats = ref<MigrationStats | null>(null)
const loading = ref(false)
const rollingBack = ref<number | null>(null)
const showConfirm = ref(false)
const selectedRecord = ref<MigrationRecord | null>(null)

const loadHistory = async () => {
  loading.value = true
  try {
    const [historyData, statsData] = await Promise.all([
      invoke<MigrationRecord[]>('get_migration_history'),
      invoke<MigrationStats>('get_migration_stats')
    ])
    records.value = historyData
    stats.value = statsData
  } catch (error) {
    console.error('Failed to load history:', error)
  } finally {
    loading.value = false
  }
}

const refreshHistory = () => {
  loadHistory()
}

const confirmRollback = (record: MigrationRecord) => {
  selectedRecord.value = record
  showConfirm.value = true
}

const executeRollback = async () => {
  if (!selectedRecord.value) return
  
  const recordId = selectedRecord.value.id
  rollingBack.value = recordId
  showConfirm.value = false
  
  try {
    const result = await invoke('rollback_migration', { migrationId: recordId })
    console.log('Rollback result:', result)
    await loadHistory()
  } catch (error) {
    console.error('Rollback failed:', error)
    alert(`回滚失败: ${error}`)
  } finally {
    rollingBack.value = null
    selectedRecord.value = null
  }
}

const formatSize = (bytes: number): string => {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return (bytes / Math.pow(k, i)).toFixed(2) + ' ' + sizes[i]
}

const formatDate = (dateStr: string): string => {
  const date = new Date(dateStr)
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  })
}

onMounted(() => {
  loadHistory()
})
</script>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;500;600;700&display=swap');

.history-container {
  min-height: 100vh;
  background: linear-gradient(to bottom, #fafaf9 0%, #ffffff 100%);
  font-family: 'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif;
}

.history-header {
  padding: 3rem 3rem 2rem;
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  border-bottom: 1px solid #e7e5e4;
  background: white;
  gap: 2rem;
}

.header-content {
  flex: 1;
  min-width: 0;
}

.header-content h1 {
  font-size: 2rem;
  font-weight: 700;
  color: #1c1917;
  margin: 0 0 0.5rem 0;
  letter-spacing: -0.03em;
}

.header-subtitle {
  font-size: 0.9375rem;
  color: #78716c;
  margin: 0;
  font-weight: 400;
}

.refresh-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 1.25rem;
  background: white;
  color: #1c1917;
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: inherit;
  flex-shrink: 0;
  margin-right: 3rem;
}

.refresh-btn:hover:not(:disabled) {
  background: #fafaf9;
  border-color: #d6d3d1;
  transform: translateY(-1px);
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.refresh-btn svg.spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 1.25rem;
  padding: 2rem 3rem;
  background: white;
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1.5rem;
  background: #fafaf9;
  border: 1px solid #e7e5e4;
  border-radius: 14px;
  transition: all 0.2s ease;
}

.stat-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.06);
}

.stat-card.active {
  background: linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%);
  border-color: #86efac;
}

.stat-card.rolled {
  background: linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%);
  border-color: #fca5a5;
}

.stat-icon {
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  border-radius: 12px;
  color: #78716c;
  flex-shrink: 0;
}

.stat-icon.total {
  color: #0ea5e9;
}

.stat-icon.size {
  color: #8b5cf6;
}

.stat-card.active .stat-icon {
  color: #10b981;
}

.stat-card.rolled .stat-icon {
  color: #ef4444;
}

.stat-content {
  flex: 1;
}

.stat-value {
  font-size: 1.75rem;
  font-weight: 700;
  color: #1c1917;
  margin-bottom: 0.25rem;
  letter-spacing: -0.02em;
}

.stat-label {
  font-size: 0.8125rem;
  color: #78716c;
  font-weight: 500;
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 6rem 2rem;
  gap: 1.5rem;
}

.loading-spinner {
  width: 48px;
  height: 48px;
  border: 3px solid #e7e5e4;
  border-top-color: #1c1917;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.loading-state p {
  font-size: 1rem;
  color: #78716c;
  font-weight: 500;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 6rem 2rem;
  text-align: center;
}

.empty-icon {
  color: #d6d3d1;
  margin-bottom: 1.5rem;
}

.empty-state h3 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #1c1917;
  margin: 0 0 0.5rem 0;
}

.empty-state p {
  font-size: 0.9375rem;
  color: #78716c;
  margin: 0;
}

.records-list {
  padding: 2rem 3rem 3rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.record-card {
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 16px;
  padding: 1.75rem;
  transition: all 0.2s ease;
}

.record-card:hover {
  border-color: #d6d3d1;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.04);
}

.record-card.is-rolled-back {
  opacity: 0.6;
  background: #fafaf9;
}

.record-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.25rem;
}

.record-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8125rem;
  color: #78716c;
  font-weight: 500;
}

.record-id {
  color: #1c1917;
  font-weight: 600;
}

.record-dot {
  color: #d6d3d1;
}

.record-badge {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.375rem 0.875rem;
  border-radius: 8px;
  font-size: 0.8125rem;
  font-weight: 600;
}

.record-badge.active {
  background: #ecfdf5;
  color: #059669;
}

.record-badge.rolled_back {
  background: #fef2f2;
  color: #dc2626;
}

.badge-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.record-paths {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  gap: 1.25rem;
  align-items: center;
  padding: 1.25rem;
  background: #fafaf9;
  border-radius: 12px;
  margin-bottom: 1.25rem;
}

.path-row {
  min-width: 0;
}

.path-label {
  display: block;
  font-size: 0.75rem;
  font-weight: 600;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.5rem;
}

.path-value {
  display: block;
  font-size: 0.875rem;
  color: #1c1917;
  font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
  word-break: break-all;
  line-height: 1.5;
}

.path-arrow {
  color: #a8a29e;
  flex-shrink: 0;
}

.record-details {
  display: flex;
  gap: 1.5rem;
  margin-bottom: 1.25rem;
}

.detail-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.875rem;
  color: #57534e;
}

.detail-item svg {
  color: #a8a29e;
}

.record-actions {
  display: flex;
  justify-content: flex-end;
}

.rollback-btn {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 1.25rem;
  background: white;
  color: #dc2626;
  border: 1px solid #fca5a5;
  border-radius: 10px;
  font-size: 0.875rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  font-family: inherit;
}

.rollback-btn:hover:not(:disabled) {
  background: #fef2f2;
  border-color: #f87171;
  transform: translateY(-1px);
}

.rollback-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 3000;
  backdrop-filter: blur(8px);
  animation: fadeIn 0.2s ease;
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.modal-dialog {
  background: white;
  border-radius: 20px;
  width: 90%;
  max-width: 520px;
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.2);
  animation: slideUp 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  position: relative;
}

.modal-close {
  position: absolute;
  top: 1.25rem;
  right: 1.25rem;
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
  z-index: 10;
}

.modal-close:hover {
  background: #e7e5e4;
  color: #1c1917;
  transform: scale(1.05);
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(20px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.modal-header {
  padding: 2rem 2rem 1.5rem;
  text-align: center;
  border-bottom: 1px solid #f5f5f4;
}

.modal-icon {
  width: 56px;
  height: 56px;
  margin: 0 auto 1.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%);
  border-radius: 16px;
  color: #dc2626;
}

.modal-header h3 {
  font-size: 1.375rem;
  font-weight: 700;
  color: #1c1917;
  margin: 0 0 0.5rem 0;
  letter-spacing: -0.02em;
}

.modal-header p {
  font-size: 0.9375rem;
  color: #78716c;
  margin: 0;
}

.modal-content {
  padding: 1.5rem 2rem;
}

.info-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.info-row {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.info-label {
  font-size: 0.75rem;
  font-weight: 600;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.info-value {
  font-size: 0.875rem;
  color: #1c1917;
  font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
  word-break: break-all;
  line-height: 1.5;
}

.warning-box {
  display: flex;
  gap: 0.875rem;
  padding: 1rem;
  background: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: 12px;
}

.warning-box svg {
  color: #f59e0b;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.warning-title {
  font-size: 0.875rem;
  font-weight: 600;
  color: #92400e;
  margin-bottom: 0.25rem;
}

.warning-text {
  font-size: 0.8125rem;
  color: #78350f;
  line-height: 1.5;
}

.modal-actions {
  display: flex;
  gap: 0.75rem;
  padding: 1.5rem 2rem 2rem;
  border-top: 1px solid #f5f5f4;
}

.btn-secondary,
.btn-danger {
  flex: 1;
  padding: 0.75rem 1.5rem;
  border-radius: 12px;
  font-size: 0.9375rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
  border: none;
  font-family: inherit;
}

.btn-secondary {
  background: #f5f5f4;
  color: #57534e;
}

.btn-secondary:hover {
  background: #e7e5e4;
}

.btn-danger {
  background: #dc2626;
  color: white;
}

.btn-danger:hover {
  background: #b91c1c;
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(220, 38, 38, 0.3);
}

@media (max-width: 768px) {
  .history-header {
    padding: 2rem 1.5rem 1.5rem;
    flex-direction: column;
    gap: 1rem;
    align-items: stretch;
  }

  .refresh-btn {
    margin-right: 0;
    align-self: flex-start;
  }

  .stats-grid {
    grid-template-columns: 1fr;
    padding: 1.5rem;
  }

  .records-list {
    padding: 1.5rem;
  }

  .record-paths {
    grid-template-columns: 1fr;
    gap: 1rem;
  }

  .path-arrow {
    transform: rotate(90deg);
    margin: 0.5rem 0;
  }

  .modal-dialog {
    max-width: 95%;
  }
}
</style>
