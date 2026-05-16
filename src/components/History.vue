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
import type { MigrationRecord, MigrationStats } from '../types'
import { formatBytes, formatDate } from '../utils/format'
import { useToast } from '../composables/useToast'

const showToast = useToast()

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
  } catch (err) {
    console.error('[History] loadHistory failed:', err)
    showToast('加载历史记录失败', String(err), 'error')
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
    await invoke('rollback_migration', { migrationId: recordId })
    await loadHistory()
    showToast('回滚成功', '文件已恢复到原始位置', 'success')
  } catch (error) {
    showToast('回滚失败', String(error), 'error')
  } finally {
    rollingBack.value = null
    selectedRecord.value = null
  }
}

const formatSize = formatBytes

onMounted(() => {
  loadHistory()
})
</script>

<style scoped>
.history-container {
  height: 100%;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  background: transparent;
  font-family: var(--font-sans);
}

.history-header {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 1rem;
  /* 右侧多 3.4rem 给外层 modal-close ✕ 按钮让位（2.7rem 宽 + 间距） */
  padding: 1.6rem 5.2rem 1.2rem 1.8rem;
  background: rgba(251, 247, 241, 0.92);
  border-bottom: 1px solid var(--color-border-light);
}

.header-content {
  min-width: 0;
}

.header-content h1 {
  font-size: 2rem;
  margin-bottom: 0.3rem;
}

.header-subtitle {
  font-size: 0.9rem;
  line-height: 1.6;
  color: var(--color-text-tertiary);
}

.refresh-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.72rem 1rem;
  border-radius: 1rem;
  border: 1px solid var(--color-border-light);
  background: rgba(255, 255, 255, 0.72);
  color: var(--color-text-secondary);
  font-size: 0.85rem;
  font-weight: 800;
  cursor: pointer;
  box-shadow: var(--shadow-xs);
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), color var(--transition-fast), opacity var(--transition-fast);
}

.refresh-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: var(--color-surface-hover);
  box-shadow: var(--shadow-sm);
  color: var(--color-text-primary);
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.refresh-btn svg.spinning,
.loading-spinner {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
  gap: 1rem;
  padding: 1.3rem 1.8rem 0;
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1.2rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.78), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
  transition: transform var(--transition-base), box-shadow var(--transition-base);
}

.stat-card:hover {
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}

.stat-card.active {
  background: linear-gradient(135deg, rgba(15, 159, 110, 0.08), rgba(93, 201, 194, 0.1));
}

.stat-card.rolled {
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.06), rgba(248, 113, 113, 0.1));
}

.stat-icon {
  width: 2.9rem;
  height: 2.9rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 1rem;
  background: rgba(255, 255, 255, 0.72);
  color: var(--color-text-secondary);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
  flex-shrink: 0;
}

.stat-icon.total {
  color: var(--color-info);
}

.stat-icon.size {
  color: var(--color-highlight);
}

.stat-card.active .stat-icon {
  color: var(--color-success);
}

.stat-card.rolled .stat-icon {
  color: var(--color-error);
}

.stat-value {
  font-size: 1.55rem;
  font-weight: 800;
  letter-spacing: 0;
  color: var(--color-text-primary);
}

.stat-label {
  margin-top: 0.2rem;
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
}

.loading-state,
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.7rem;
  padding: 5rem 2rem;
}

.loading-spinner {
  width: 2.8rem;
  height: 2.8rem;
  border-radius: 50%;
  border: 3px solid rgba(23, 23, 23, 0.12);
  border-top-color: var(--color-highlight);
}

.loading-state p,
.empty-state p {
  color: var(--color-text-tertiary);
}

.empty-icon {
  color: rgba(73, 68, 60, 0.34);
}

.records-list {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.3rem 1.8rem 1.8rem;
}

.record-card {
  padding: 1.35rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.78), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
  transition: transform var(--transition-base), box-shadow var(--transition-base), border-color var(--transition-base), opacity var(--transition-fast);
}

.record-card:hover {
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}

.record-card.is-rolled-back {
  opacity: 0.72;
}

.record-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
}

.record-meta {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
  font-weight: 700;
}

.record-id {
  color: var(--color-text-primary);
}

.record-dot {
  opacity: 0.4;
}

.record-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0.42rem 0.78rem;
  border-radius: var(--radius-pill);
  font-size: 0.78rem;
  font-weight: 800;
}

.record-badge.active {
  background: rgba(15, 159, 110, 0.12);
  color: var(--color-success);
}

.record-badge.rolled_back {
  background: rgba(220, 38, 38, 0.12);
  color: var(--color-error);
}

.badge-dot {
  width: 0.42rem;
  height: 0.42rem;
  border-radius: 50%;
  background: currentColor;
}

.record-paths {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 1rem;
  align-items: center;
  margin-bottom: 1rem;
  padding: 1rem;
  border-radius: var(--radius-md);
  background: rgba(255, 255, 255, 0.6);
}

.path-label {
  display: block;
  margin-bottom: 0.45rem;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.path-value,
.info-value {
  display: block;
  font-size: 0.84rem;
  line-height: 1.6;
  color: var(--color-text-primary);
  word-break: break-all;
  font-family: var(--font-mono);
}

.path-arrow {
  color: var(--color-text-tertiary);
}

.record-details {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem 1.4rem;
  margin-bottom: 1rem;
}

.detail-item {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  color: var(--color-text-secondary);
  font-size: 0.84rem;
}

.detail-item svg {
  color: var(--color-text-tertiary);
}

.record-actions {
  display: flex;
  justify-content: flex-end;
}

.rollback-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.72rem 1rem;
  border-radius: 1rem;
  border: 1px solid rgba(220, 38, 38, 0.18);
  background: rgba(220, 38, 38, 0.06);
  color: var(--color-error);
  font-size: 0.84rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), opacity var(--transition-fast);
}

.rollback-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: rgba(220, 38, 38, 0.1);
  box-shadow: var(--shadow-xs);
}

.rollback-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.modal-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  background: rgba(18, 18, 18, 0.34);
  backdrop-filter: blur(18px);
  z-index: 3000;
}

.modal-dialog {
  position: relative;
  width: min(100%, 34rem);
  border-radius: var(--radius-xl);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.88), rgba(247, 241, 232, 0.94));
  border: 1px solid rgba(255, 255, 255, 0.28);
  box-shadow: var(--shadow-xl);
}

.modal-close {
  position: absolute;
  top: 1rem;
  right: 1rem;
  width: 2.5rem;
  height: 2.5rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-border-light);
  border-radius: 0.9rem;
  background: rgba(255, 255, 255, 0.74);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base), color var(--transition-fast);
}

.modal-close:hover {
  transform: translateY(-1px);
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.modal-header {
  padding: 1.8rem 1.8rem 1.2rem;
  text-align: center;
  border-bottom: 1px solid var(--color-border-light);
}

.modal-icon {
  width: 3.4rem;
  height: 3.4rem;
  margin: 0 auto 1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 1.1rem;
  background: rgba(220, 38, 38, 0.12);
  color: var(--color-error);
}

.modal-header h3 {
  font-size: 1.45rem;
  margin-bottom: 0.35rem;
}

.modal-header p {
  color: var(--color-text-tertiary);
}

.modal-content {
  padding: 1.3rem 1.8rem;
}

.info-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-bottom: 1.2rem;
}

.info-row {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.info-label {
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.warning-box {
  display: flex;
  gap: 0.75rem;
  padding: 0.95rem 1rem;
  border-radius: var(--radius-md);
  background: rgba(217, 119, 6, 0.08);
  border: 1px solid rgba(217, 119, 6, 0.16);
}

.warning-box svg {
  color: var(--color-warning);
  flex-shrink: 0;
  margin-top: 0.15rem;
}

.warning-title {
  margin-bottom: 0.2rem;
  font-size: 0.84rem;
  font-weight: 800;
  color: #92400e;
}

.warning-text {
  font-size: 0.8rem;
  line-height: 1.55;
  color: #78350f;
}

.modal-actions {
  display: flex;
  gap: 0.75rem;
  padding: 1.2rem 1.8rem 1.8rem;
  border-top: 1px solid var(--color-border-light);
}

.btn-secondary,
.btn-danger {
  flex: 1;
  padding: 0.82rem 1rem;
  border-radius: 1rem;
  border: none;
  font-size: 0.88rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), opacity var(--transition-fast);
}

.btn-secondary {
  background: rgba(255, 255, 255, 0.74);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-light);
}

.btn-secondary:hover {
  background: var(--color-surface-hover);
}

.btn-danger {
  background: linear-gradient(135deg, var(--color-error), #b91c1c);
  color: var(--color-text-inverse);
  box-shadow: var(--shadow-sm);
}

.btn-danger:hover {
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

@media (max-width: 768px) {
  .history-header,
  .stats-grid,
  .records-list,
  .modal-header,
  .modal-content,
  .modal-actions {
    padding-left: 1rem;
    padding-right: 1rem;
  }

  .history-header {
    flex-direction: column;
    align-items: stretch;
  }

  .record-header,
  .record-paths {
    grid-template-columns: 1fr;
    display: grid;
  }

  .path-arrow {
    transform: rotate(90deg);
  }

  .modal-actions {
    flex-direction: column;
  }
}
</style>
