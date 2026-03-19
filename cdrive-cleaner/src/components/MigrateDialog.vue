<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { IconClose, IconWarning, IconError, IconShield } from './icons';
import type { DirectoryNode, FileInfo, DiskInfo, MigrationSafety, MigrationResult } from '../types';
import { formatBytes, formatNumber, formatSpeed, formatDuration as formatTime } from '../utils/format';
import { getSettings } from '../utils/settings';
import { useToast } from '../composables/useToast';

const showToast = useToast();

interface Props {
  show: boolean;
  selectedDir: DirectoryNode | null;
  selectedFile: FileInfo | null;
  selectedItems: Array<DirectoryNode | FileInfo>;
  availableDisks: DiskInfo[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'close': [];
  'migrated': [paths: string[]];
}>();

const targetDisk = ref<string>('');
const migrating = ref(false);
const migrationError = ref<string>('');
const migrationSuccess = ref(false);
const migrationResult = ref<MigrationResult | null>(null);
const currentMigratingIndex = ref(0);
const migrationResults = ref<Array<{ path: string; success: boolean; error?: string }>>([]);

const migrationStartTime = ref(0);
const migratedSize = ref(0);
const migrationSpeed = ref(0);
const estimatedTimeRemaining = ref(0);
const elapsedTime = ref(0);
const updateTimer = ref<number | null>(null);
const migrationProgressPercent = ref(0);
const migrationStatus = ref<'copying' | 'verifying' | 'creating_link' | 'cleaning_up'>('copying');
const currentMigratingFile = ref('');

// 安全性分析
const safetyAnalysis = ref<MigrationSafety | null>(null);
const analyzingSafety = ref(false);
const safetyAnalysisStartTime = ref(0);
const safetyAnalysisDuration = ref(0);
const safetyScannedDirs = ref(0);
const safetyDirsPerSecond = ref(0);

// 监听对话框打开，进行安全性分析和加载默认设置
watch(() => props.show, async (newShow) => {
  if (newShow) {
    // 加载默认目标磁盘
    loadDefaultTargetDisk();
    
    // 进行安全性分析
    if (!isBatchMode.value && itemPath.value) {
      await analyzeSafety();
    }
  }
});

function loadDefaultTargetDisk() {
  const settings = getSettings();
  if (settings.defaultTargetDisk) {
    targetDisk.value = settings.defaultTargetDisk;
  }
}

async function analyzeSafety() {
  if (!itemPath.value) return;
  
  analyzingSafety.value = true;
  safetyAnalysis.value = null;
  safetyAnalysisStartTime.value = Date.now();
  safetyScannedDirs.value = 0;
  safetyDirsPerSecond.value = 0;
  let unlisten: (() => void) | null = null;
  
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const { listen } = await import('@tauri-apps/api/event');
    
    // 监听进度事件
    unlisten = await listen<{ scanned_dirs: number; dirs_per_second: number }>('safety-analysis-progress', (event) => {
      const progress = event.payload;
      safetyScannedDirs.value = progress.scanned_dirs;
      safetyDirsPerSecond.value = progress.dirs_per_second;
    });
    
    const result = await invoke<MigrationSafety>('analyze_migration_safety', {
      path: itemPath.value,
      size: itemSize.value
    });
    
    safetyAnalysisDuration.value = Date.now() - safetyAnalysisStartTime.value;
    safetyAnalysis.value = result;
  } catch {
    showToast('安全性分析失败', '无法完成迁移安全性评估', 'warning');
  } finally {
    unlisten?.();
    analyzingSafety.value = false;
  }
}

const canMigrate = computed(() => {
  // 批量模式暂时允许迁移
  if (isBatchMode.value) return true;
  
  // 如果正在分析，不允许迁移
  if (analyzingSafety.value) return false;
  
  // 如果有安全性分析结果，根据结果判断
  if (safetyAnalysis.value) {
    return safetyAnalysis.value.can_migrate;
  }
  
  // 默认允许
  return true;
});

const riskLevelText = computed(() => {
  if (!safetyAnalysis.value) return '';
  
  const levelMap = {
    safe: '安全',
    moderate: '中等风险',
    risky: '高风险',
    dangerous: '危险'
  };
  
  return levelMap[safetyAnalysis.value.risk_level] || '';
});

const riskLevelClass = computed(() => {
  if (!safetyAnalysis.value) return '';
  return `risk-${safetyAnalysis.value.risk_level}`;
});

const isBatchMode = computed(() => props.selectedItems && props.selectedItems.length > 0);
const itemName = computed(() => {
  if (isBatchMode.value) {
    return `批量迁移 (${props.selectedItems.length} 项)`;
  }
  return props.selectedFile ? '迁移文件' : '迁移目录';
});
const itemPath = computed(() => props.selectedFile?.path || props.selectedDir?.path);
const itemSize = computed(() => {
  if (isBatchMode.value) {
    return props.selectedItems.reduce((sum, item) => sum + (item.size || 0), 0);
  }
  return props.selectedFile?.size || props.selectedDir?.size || 0;
});

const migrationStatusText = computed(() => {
  switch (migrationStatus.value) {
    case 'verifying':
      return '正在校验复制结果';
    case 'creating_link':
      return '正在创建原路径链接';
    case 'cleaning_up':
      return '正在清理原始数据';
    default:
      return '正在复制文件到目标磁盘';
  }
});

const successMessage = computed(() => {
  if (isBatchMode.value) {
    return '';
  }

  if (migrationResult.value?.link_type && migrationResult.value.link_type !== 'none') {
    return '原路径会保留为链接占位，因此在 C 盘还能看到同名条目，但实际数据已经迁移走，不再占用原始空间。';
  }

  return '文件已成功迁移到目标磁盘，原路径已移除。';
});

const willKeepSourceLink = computed(() => {
  if (migrationResult.value) {
    return migrationResult.value.link_type !== 'none';
  }

  return getSettings().createSymlink;
});

const linkTypeText = computed(() => {
  switch (migrationResult.value?.link_type) {
    case 'junction':
      return 'Junction';
    case 'symlink':
      return '符号链接';
    case 'hardlink':
      return '硬链接';
    case 'none':
      return '不保留链接';
    default:
      return '自动';
  }
});

function startProgressTimer() {
  if (updateTimer.value) {
    clearInterval(updateTimer.value);
  }
  updateTimer.value = window.setInterval(() => {
    if (migrationStartTime.value > 0) {
      elapsedTime.value = (Date.now() - migrationStartTime.value) / 1000;
    }
  }, 500);
}

function stopProgressTimer() {
  if (updateTimer.value) {
    clearInterval(updateTimer.value);
    updateTimer.value = null;
  }
}

function close() {
  stopProgressTimer();
  targetDisk.value = '';
  migrating.value = false;
  migrationError.value = '';
  migrationSuccess.value = false;
  migrationResult.value = null;
  currentMigratingIndex.value = 0;
  migrationResults.value = [];
  migrationStartTime.value = 0;
  migratedSize.value = 0;
  migrationSpeed.value = 0;
  estimatedTimeRemaining.value = 0;
  elapsedTime.value = 0;
  migrationProgressPercent.value = 0;
  migrationStatus.value = 'copying';
  currentMigratingFile.value = '';
  safetyAnalysis.value = null;
  analyzingSafety.value = false;
  emit('close');
}

async function startMigration() {
  if (!targetDisk.value) {
    migrationError.value = '请选择目标磁盘';
    return;
  }

  if (isBatchMode.value) {
    await startBatchMigration();
  } else {
    await startSingleMigration();
  }
}

async function startSingleMigration() {
  if (!itemPath.value) {
    migrationError.value = '未选择要迁移的项目';
    return;
  }

  migrating.value = true;
  migrationError.value = '';
  migrationStartTime.value = Date.now();
  migratedSize.value = 0;
  elapsedTime.value = 0;
  migrationProgressPercent.value = 0;
  migrationStatus.value = 'copying';
  currentMigratingFile.value = '';
  startProgressTimer();
  let unlisten: (() => void) | null = null;

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const { listen } = await import('@tauri-apps/api/event');
    const { createSymlink } = getSettings();

    unlisten = await listen<{ status: 'copying' | 'verifying' | 'creating_link' | 'cleaning_up'; copied_bytes: number; total_bytes: number; copied_files: number; total_files: number; progress_percent: number; current_file: string }>('migration-progress', (event) => {
      const p = event.payload;
      migrationStatus.value = p.status;
      migratedSize.value = p.copied_bytes;
      migrationProgressPercent.value = p.progress_percent;
      currentMigratingFile.value = p.current_file;
      const elapsed = (Date.now() - migrationStartTime.value) / 1000;
      if (elapsed > 0) {
        migrationSpeed.value = p.copied_bytes / elapsed;
        const remaining = p.total_bytes - p.copied_bytes;
        estimatedTimeRemaining.value = migrationSpeed.value > 0 ? remaining / migrationSpeed.value : 0;
      }
    });

    const result = await invoke<MigrationResult>('migrate_file', {
      source: itemPath.value,
      targetDisk: targetDisk.value,
      linkType: createSymlink ? null : 'none'
    });

    if (!result.success) {
      throw new Error(result.error || '迁移失败');
    }
    stopProgressTimer();
    migrationSuccess.value = true;
    migrationResult.value = result;
    migrating.value = false;
    emit('migrated', [result.source_path]);
  } catch (err) {
    stopProgressTimer();
    migrationError.value = String(err);
    migrating.value = false;
  } finally {
    unlisten?.();
  }
}

async function startBatchMigration() {
  migrating.value = true;
  migrationError.value = '';
  migrationResults.value = [];
  currentMigratingIndex.value = 0;
  migrationStartTime.value = Date.now();
  migratedSize.value = 0;
  migrationProgressPercent.value = 0;
  migrationStatus.value = 'copying';

  const { invoke } = await import('@tauri-apps/api/core');
  const totalSize = itemSize.value;
  const { createSymlink } = getSettings();
  const succeededPaths: string[] = [];

  for (let i = 0; i < props.selectedItems.length; i++) {
    currentMigratingIndex.value = i;
    const item = props.selectedItems[i];
    
    try {
      const result = await invoke<MigrationResult>('migrate_file', {
        source: item.path,
        targetDisk: targetDisk.value,
        linkType: createSymlink ? null : 'none'
      });
      if (!result.success) {
        throw new Error(result.error || '迁移失败');
      }
      
      migratedSize.value += item.size || 0;
      migrationProgressPercent.value = (migratedSize.value / totalSize) * 100;
      
      const elapsedSeconds = (Date.now() - migrationStartTime.value) / 1000;
      if (elapsedSeconds > 0) {
        migrationSpeed.value = migratedSize.value / elapsedSeconds;
        const remainingSize = totalSize - migratedSize.value;
        estimatedTimeRemaining.value = remainingSize / migrationSpeed.value;
      }
      
      migrationResults.value.push({
        path: item.path,
        success: true
      });
      succeededPaths.push(item.path);
    } catch (err) {
      migrationResults.value.push({
        path: item.path,
        success: false,
        error: String(err)
      });
    }
  }

  migrating.value = false;
  migrationSuccess.value = migrationResults.value.some(r => r.success);

  const failedCount = migrationResults.value.filter(r => !r.success).length;
  if (failedCount > 0) {
    migrationError.value = `${failedCount} 项迁移失败`;
  }
  if (succeededPaths.length > 0) {
    emit('migrated', succeededPaths);
  }
}
</script>

<template>
  <div v-if="show" class="overlay" @click="close">
    <div class="dialog" @click.stop>
      <div v-if="!migrationSuccess" class="content">
        <div class="header">
          <h3>{{ itemName }}</h3>
          <button class="close-btn" @click="close">
            <IconClose :size="20" />
          </button>
        </div>
        
        <div class="body">
          <div v-if="isBatchMode" class="info-section">
            <div class="info-row">
              <span class="label">选中项目</span>
              <span class="value">{{ selectedItems.length }} 项</span>
            </div>
            <div class="info-row">
              <span class="label">总大小</span>
              <span class="value">{{ formatBytes(itemSize) }}</span>
            </div>
            <div class="batch-items-preview">
              <div v-for="(item, index) in selectedItems.slice(0, 5)" :key="item.path" class="batch-item">
                <span class="batch-item-name">{{ item.name }}</span>
                <span class="batch-item-size">{{ formatBytes(item.size) }}</span>
              </div>
              <div v-if="selectedItems.length > 5" class="batch-more">
                还有 {{ selectedItems.length - 5 }} 项...
              </div>
            </div>
          </div>
          
          <div v-else class="info-section">
            <div class="info-row">
              <span class="label">源路径</span>
              <span class="value">{{ itemPath }}</span>
            </div>
            <div class="info-row">
              <span class="label">大小</span>
              <span class="value">{{ formatBytes(itemSize) }}</span>
            </div>
            <div v-if="selectedDir" class="info-row">
              <span class="label">文件数</span>
              <span class="value">{{ formatNumber(selectedDir.file_count) }}</span>
            </div>
            <div v-if="selectedFile" class="info-row">
              <span class="label">类型</span>
              <span class="value">{{ selectedFile.extension || '无扩展名' }}</span>
            </div>
          </div>
          
          <div class="form-section">
            <label class="form-label">目标磁盘</label>
            <select class="form-select" v-model="targetDisk" :disabled="migrating">
              <option value="">选择目标磁盘...</option>
              <option 
                v-for="disk in availableDisks" 
                :key="disk.drive_letter"
                :value="disk.drive_letter + '\\'"
              >
                {{ disk.drive_letter }} - {{ disk.label }} (可用: {{ formatBytes(disk.free_space) }})
              </option>
            </select>
          </div>
          
          <!-- 安全性分析结果 -->
          <div v-if="!isBatchMode && safetyAnalysis" class="safety-analysis" :class="riskLevelClass">
            <div class="safety-header">
              <IconShield :size="20" />
              <div class="safety-title">
                <span class="safety-level">{{ riskLevelText }}</span>
                <span class="safety-score">安全评分: {{ safetyAnalysis.safety_score }}/100</span>
              </div>
            </div>
            
            <div class="safety-details">
              <div v-if="safetyAnalysis.app_type !== '未知类型'" class="safety-item">
                <span class="safety-label">应用类型</span>
                <span class="safety-value">{{ safetyAnalysis.app_type }}</span>
              </div>
              
              <div v-if="safetyAnalysis.reasons.length > 0" class="safety-reasons">
                <div class="safety-label">分析结果</div>
                <ul class="safety-list">
                  <li v-for="(reason, index) in safetyAnalysis.reasons" :key="index">{{ reason }}</li>
                </ul>
              </div>
              
              <div v-if="safetyAnalysis.recommendations.length > 0" class="safety-recommendations">
                <div class="safety-label">建议</div>
                <ul class="safety-list">
                  <li v-for="(rec, index) in safetyAnalysis.recommendations" :key="index">{{ rec }}</li>
                </ul>
              </div>
            </div>
          </div>
          
          <div v-if="analyzingSafety" class="analyzing">
            <svg class="spinner" width="16" height="16" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-dasharray="30 10"/>
            </svg>
            <div class="analyzing-content">
              <span class="analyzing-text">正在分析安全性（扫描3层子目录）...</span>
              <div class="analyzing-stats">
                <span class="stat-item">已扫描: {{ safetyScannedDirs }} 个目录</span>
                <span class="stat-divider">•</span>
                <span class="stat-item">速度: {{ Math.round(safetyDirsPerSecond) }} 目录/秒</span>
              </div>
            </div>
          </div>
          
          <div class="warning">
            <IconWarning :size="20" />
            <span>{{ willKeepSourceLink ? '迁移后会在原位置保留链接占位，程序仍可从旧路径访问。' : '迁移后会直接移除原路径，不再保留链接占位。' }}</span>
          </div>
          
          <div v-if="migrationError" class="error">
            <IconError :size="20" />
            <span>{{ migrationError }}</span>
          </div>
          
          <div v-if="migrating" class="progress">
            <div class="progress-header">
              <span class="progress-label" v-if="isBatchMode">
                正在迁移 {{ currentMigratingIndex + 1 }} / {{ selectedItems.length }}
              </span>
              <span class="progress-label" v-else>正在迁移...</span>
              <span class="progress-stats" v-if="isBatchMode && migrationSpeed > 0">
                {{ formatSpeed(migrationSpeed) }}
              </span>
            </div>
            
            <div v-if="isBatchMode" class="progress-bar-wrapper">
              <div class="progress-bar-track">
                <div class="progress-bar-fill" :style="{ width: `${((currentMigratingIndex + 1) / selectedItems.length) * 100}%` }"></div>
              </div>
            </div>
            <div v-else class="progress-bar-wrapper">
              <div class="progress-bar-track">
                <div class="progress-bar-fill" :style="{ width: `${migrationProgressPercent}%` }"></div>
              </div>
            </div>
            
            <div class="progress-info">
              <span class="progress-text" v-if="isBatchMode && selectedItems[currentMigratingIndex]">
                {{ selectedItems[currentMigratingIndex].name }}
              </span>
              <span class="progress-text" v-else>{{ migrationStatusText }}</span>
            </div>
            
            <div v-if="(isBatchMode && migrationSpeed > 0) || (!isBatchMode && (migrationSpeed > 0 || migrationProgressPercent > 0))" class="progress-details">
              <div class="progress-detail-item">
                <span class="detail-label">已传输</span>
                <span class="detail-value">{{ formatBytes(migratedSize) }} / {{ formatBytes(itemSize) }}</span>
              </div>
              <div v-if="!isBatchMode" class="progress-detail-item">
                <span class="detail-label">当前进度</span>
                <span class="detail-value">{{ migrationProgressPercent.toFixed(1) }}%</span>
              </div>
              <div v-if="!isBatchMode && currentMigratingFile" class="progress-detail-item">
                <span class="detail-label">当前文件</span>
                <span class="detail-value">{{ currentMigratingFile }}</span>
              </div>
              <div v-if="!isBatchMode && migrationSpeed > 0" class="progress-detail-item">
                <span class="detail-label">传输速度</span>
                <span class="detail-value">{{ formatSpeed(migrationSpeed) }}</span>
              </div>
              <div class="progress-detail-item" v-if="estimatedTimeRemaining > 0 && estimatedTimeRemaining < 86400">
                <span class="detail-label">剩余时间</span>
                <span class="detail-value">{{ formatTime(estimatedTimeRemaining) }}</span>
              </div>
            </div>
          </div>
        </div>
        
        <div class="footer">
          <button class="btn btn-secondary" @click="close" :disabled="migrating">取消</button>
          <button 
            class="btn btn-primary" 
            @click="startMigration" 
            :disabled="!targetDisk || migrating || !canMigrate || analyzingSafety"
            :title="!canMigrate ? '此项目不允许迁移' : ''"
          >
            <span v-if="migrating">
              <svg class="spinner" width="16" height="16" viewBox="0 0 16 16" fill="none">
                <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-dasharray="30 10"/>
              </svg>
              迁移中
            </span>
            <span v-else-if="analyzingSafety">分析中...</span>
            <span v-else-if="!canMigrate">不允许迁移</span>
            <span v-else>开始迁移</span>
          </button>
        </div>
      </div>
      
      <div v-else class="success">
        <div class="success-icon-wrapper">
          <svg class="success-icon" width="64" height="64" viewBox="0 0 64 64" fill="none">
            <circle cx="32" cy="32" r="30" fill="#10b981" fill-opacity="0.1"/>
            <circle cx="32" cy="32" r="24" stroke="#10b981" stroke-width="3"/>
            <path d="M20 32L28 40L44 24" stroke="#10b981" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </div>
        <h3 class="success-title">迁移完成</h3>
        
        <div v-if="isBatchMode" class="success-message">
          <p>成功迁移 {{ migrationResults.filter(r => r.success).length }} 项</p>
          <p v-if="migrationResults.filter(r => !r.success).length > 0" class="error-text">
            失败 {{ migrationResults.filter(r => !r.success).length }} 项
          </p>
        </div>
        <p v-else class="success-message">{{ successMessage }}</p>
        
        <div v-if="isBatchMode" class="batch-results">
          <div v-for="result in migrationResults" :key="result.path" class="batch-result-item" :class="{ 'result-error': !result.success }">
            <svg v-if="result.success" width="16" height="16" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="7" stroke="#10b981" stroke-width="2"/>
              <path d="M5 8L7 10L11 6" stroke="#10b981" stroke-width="2" stroke-linecap="round"/>
            </svg>
            <svg v-else width="16" height="16" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="7" stroke="#ef4444" stroke-width="2"/>
              <path d="M5 5L11 11M11 5L5 11" stroke="#ef4444" stroke-width="2" stroke-linecap="round"/>
            </svg>
            <span class="result-path">{{ result.path }}</span>
            <span v-if="result.error" class="result-error-msg">{{ result.error }}</span>
          </div>
        </div>
        
        <div v-else class="success-details">
          <div class="detail-row">
            <span class="label">源路径</span>
            <span class="value">{{ migrationResult?.source_path }}</span>
          </div>
          <div class="detail-row">
            <span class="label">目标路径</span>
            <span class="value">{{ migrationResult?.target_path }}</span>
          </div>
          <div class="detail-row">
            <span class="label">文件大小</span>
            <span class="value">{{ formatBytes(migrationResult?.file_size || 0) }}</span>
          </div>
          <div class="detail-row">
            <span class="label">保留方式</span>
            <span class="value">{{ linkTypeText }}</span>
          </div>
          <div class="detail-row">
            <span class="label">耗时</span>
            <span class="value">{{ ((migrationResult?.duration_ms ?? 0) / 1000).toFixed(2) }} 秒</span>
          </div>
        </div>
        
        <button class="btn btn-primary btn-full" @click="close">完成</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(45, 35, 25, 0.4);
  backdrop-filter: blur(24px) saturate(100%);
  -webkit-backdrop-filter: blur(24px) saturate(100%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  animation: fadeIn 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  pointer-events: auto;
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.dialog {
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: 24px;
  box-shadow: 
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  width: 90%;
  max-width: 540px;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  animation: slideIn 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans);
}

@keyframes slideIn {
  from {
    opacity: 0;
    transform: scale(0.92) translateY(20px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}

.content {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.header {
  padding: 1.75rem 2rem;
  border-bottom: 1px solid var(--color-border-light);
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.6) 0%, rgba(255, 252, 245, 0.3) 100%);
  flex-shrink: 0;
}

.header h3 {
  font-family: var(--font-serif);
  font-size: 1.375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0;
  letter-spacing: -0.02em;
}

.close-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(139, 92, 46, 0.06);
  border: none;
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-base);
}

.close-btn:hover {
  background: rgba(139, 92, 46, 0.12);
  color: var(--color-text-primary);
  transform: scale(1.05);
}

.body {
  padding: 2rem;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}

.body::-webkit-scrollbar {
  width: 6px;
}

.body::-webkit-scrollbar-track {
  background: transparent;
}

.body::-webkit-scrollbar-thumb {
  background: rgba(139, 92, 46, 0.15);
  border-radius: 3px;
}

.body::-webkit-scrollbar-thumb:hover {
  background: rgba(139, 92, 46, 0.25);
}

.info-section {
  background: var(--color-surface);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  margin-bottom: 1.5rem;
  border: 1px solid var(--color-border-light);
}

.info-row {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 0.875rem 0;
  font-size: 0.9375rem;
  gap: 1rem;
}

.info-row:not(:last-child) {
  border-bottom: 1px solid var(--color-border-light);
}

.label {
  color: var(--color-text-tertiary);
  font-weight: 500;
  flex-shrink: 0;
  min-width: 80px;
}

.value {
  color: var(--color-text-primary);
  font-weight: 500;
  text-align: right;
  word-break: break-all;
}

.form-section {
  margin-bottom: 1.5rem;
}

.form-label {
  display: block;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.5rem;
}

.form-select {
  width: 100%;
  padding: 0.75rem 1rem;
  font-size: 0.9375rem;
  background: rgba(255, 255, 255, 0.8);
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  color: var(--color-text-primary);
  cursor: pointer;
  transition: all var(--transition-base);
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg width='12' height='8' viewBox='0 0 12 8' fill='none' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M1 1.5L6 6.5L11 1.5' stroke='%235a5a5a' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 1rem center;
  padding-right: 2.75rem;
  box-shadow: var(--shadow-sm);
}

.form-select:hover {
  border-color: var(--color-border-strong);
  background: rgba(255, 255, 255, 0.95);
}

.form-select:focus {
  outline: none;
  border-color: var(--color-accent-primary);
  box-shadow: 0 0 0 3px rgba(139, 115, 85, 0.08);
}

.warning {
  display: flex;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
  background: rgba(245, 158, 11, 0.06);
  border: 1px solid rgba(245, 158, 11, 0.15);
  border-radius: var(--radius-md);
  font-size: 0.875rem;
  color: #92400e;
  align-items: flex-start;
}

.warning svg {
  flex-shrink: 0;
  color: var(--color-warning);
}

.error {
  display: flex;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
  background: rgba(239, 68, 68, 0.06);
  border: 1px solid rgba(239, 68, 68, 0.15);
  border-radius: var(--radius-md);
  font-size: 0.875rem;
  color: #991b1b;
  align-items: flex-start;
}

.error svg {
  flex-shrink: 0;
  color: var(--color-error);
}

.progress {
  margin-top: 1.5rem;
  padding: 1.5rem;
  background: linear-gradient(135deg, rgba(139, 115, 85, 0.04) 0%, rgba(139, 115, 85, 0.08) 100%);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-medium);
}

.progress-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.progress-label {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-primary);
}

.progress-stats {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--color-accent-primary);
  background: rgba(139, 115, 85, 0.1);
  padding: 0.25rem 0.75rem;
  border-radius: 6px;
}

.progress-bar-wrapper {
  margin-bottom: 0.75rem;
}

.progress-bar-track {
  height: 8px;
  background: rgba(139, 92, 46, 0.1);
  border-radius: 4px;
  overflow: hidden;
  position: relative;
}

.progress-bar-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.progress-bar-fill:not([style*="width"]) {
  width: 40%;
  animation: indeterminate 1.5s ease-in-out infinite;
}

@keyframes indeterminate {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(350%); }
}

.progress-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
}

.progress-text {
  opacity: 0.9;
}

.progress-details {
  display: flex;
  gap: 1.5rem;
  margin-top: 1rem;
  padding-top: 1rem;
  border-top: 1px solid var(--color-border-light);
}

.progress-detail-item {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.detail-label {
  font-size: 0.75rem;
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: 600;
}

.detail-value {
  font-size: 0.9375rem;
  color: var(--color-text-primary);
  font-weight: 600;
}

.footer {
  padding: 1.5rem 2rem;
  border-top: 1px solid var(--color-border-light);
  display: flex;
  gap: 0.875rem;
  justify-content: flex-end;
  flex-shrink: 0;
  background: linear-gradient(to top, var(--color-bg-tertiary) 0%, rgba(255, 255, 255, 0.4) 100%);
}

.btn {
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition-base);
  display: flex;
  align-items: center;
  gap: 0.5rem;
  letter-spacing: -0.01em;
}

.btn-secondary {
  background: rgba(139, 92, 46, 0.06);
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border-light);
}

.btn-secondary:hover {
  background: rgba(139, 92, 46, 0.12);
  border-color: var(--color-border-medium);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.btn-primary {
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  color: white;
  box-shadow: 
    0 0 0 1px rgba(139, 115, 85, 0.2),
    0 4px 16px rgba(139, 115, 85, 0.25);
}

.btn-primary:hover:not(:disabled) {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(139, 115, 85, 0.3),
    0 8px 24px rgba(139, 115, 85, 0.35);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.success {
  padding: 3rem 2.5rem;
  text-align: center;
  animation: fadeIn 0.4s ease-out;
}

.success-icon-wrapper {
  margin-bottom: 1.5rem;
  animation: iconPop 0.6s cubic-bezier(0.34, 1.56, 0.64, 1);
}

@keyframes iconPop {
  0% {
    opacity: 0;
    transform: scale(0);
  }
  50% {
    transform: scale(1.1);
  }
  100% {
    opacity: 1;
    transform: scale(1);
  }
}

.success-icon {
  display: inline-block;
}

.success-title {
  font-family: var(--font-serif);
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0 0 0.75rem 0;
  letter-spacing: -0.02em;
}

.success-message {
  font-size: 0.9375rem;
  color: var(--color-text-tertiary);
  line-height: 1.6;
  margin: 0 0 2rem 0;
}

.error-text {
  color: var(--color-error);
  margin-top: 0.5rem;
}

.success-details {
  background: var(--color-surface);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  margin-bottom: 2rem;
  text-align: left;
  border: 1px solid var(--color-border-light);
}

.detail-row {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 0.75rem 0;
  font-size: 0.875rem;
  gap: 1rem;
}

.detail-row:not(:last-child) {
  border-bottom: 1px solid var(--color-border-light);
}

.btn-full {
  width: 100%;
  padding: 1rem 1.5rem;
  font-size: 1rem;
  justify-content: center;
}

.batch-items-preview {
  margin-top: 1rem;
  padding: 1rem;
  background: rgba(139, 92, 46, 0.03);
  border-radius: var(--radius-sm);
  max-height: 200px;
  overflow-y: auto;
}

.batch-items-preview::-webkit-scrollbar {
  width: 6px;
}

.batch-items-preview::-webkit-scrollbar-track {
  background: rgba(139, 92, 46, 0.05);
  border-radius: 3px;
}

.batch-items-preview::-webkit-scrollbar-thumb {
  background: rgba(139, 92, 46, 0.2);
  border-radius: 3px;
}

.batch-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0.5rem 0;
  border-bottom: 1px solid var(--color-border-light);
}

.batch-item:last-child {
  border-bottom: none;
}

.batch-item-name {
  font-size: 0.875rem;
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  margin-right: 1rem;
}

.batch-item-size {
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
  font-weight: 500;
  flex-shrink: 0;
}

.batch-more {
  padding: 0.5rem 0;
  text-align: center;
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  font-style: italic;
}

.batch-results {
  max-height: 300px;
  overflow-y: auto;
  margin: 1.5rem 0;
  padding: 1rem;
  background: var(--color-surface);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
}

.batch-results::-webkit-scrollbar {
  width: 6px;
}

.batch-results::-webkit-scrollbar-track {
  background: rgba(139, 92, 46, 0.05);
  border-radius: 3px;
}

.batch-results::-webkit-scrollbar-thumb {
  background: rgba(139, 92, 46, 0.2);
  border-radius: 3px;
}

.batch-result-item {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem;
  margin-bottom: 0.5rem;
  background: rgba(255, 255, 255, 0.6);
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-light);
}

.batch-result-item:last-child {
  margin-bottom: 0;
}

.batch-result-item.result-error {
  background: rgba(239, 68, 68, 0.04);
  border-color: rgba(239, 68, 68, 0.15);
}

.result-path {
  flex: 1;
  font-size: 0.875rem;
  color: var(--color-text-primary);
  word-break: break-all;
}

.result-error-msg {
  font-size: 0.8125rem;
  color: var(--color-error);
  margin-top: 0.25rem;
}

.safety-analysis {
  background: rgba(139, 115, 85, 0.04);
  border: 1.5px solid var(--color-border-medium);
  border-radius: var(--radius-md);
  padding: 1.25rem;
  margin-bottom: 1.5rem;
  transition: all var(--transition-base);
  animation: slideDown 0.3s ease-out;
}

@keyframes slideDown {
  from {
    opacity: 0;
    max-height: 0;
    padding-top: 0;
    padding-bottom: 0;
    margin-bottom: 0;
  }
  to {
    opacity: 1;
    max-height: 500px;
    padding-top: 1.25rem;
    padding-bottom: 1.25rem;
    margin-bottom: 1.5rem;
  }
}

.safety-analysis.risk-safe {
  background: rgba(16, 185, 129, 0.04);
  border-color: rgba(16, 185, 129, 0.2);
}

.safety-analysis.risk-moderate {
  background: rgba(245, 158, 11, 0.04);
  border-color: rgba(245, 158, 11, 0.2);
}

.safety-analysis.risk-risky {
  background: rgba(249, 115, 22, 0.04);
  border-color: rgba(249, 115, 22, 0.2);
}

.safety-analysis.risk-dangerous {
  background: rgba(239, 68, 68, 0.04);
  border-color: rgba(239, 68, 68, 0.2);
}

.safety-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 1rem;
}

.safety-header svg {
  flex-shrink: 0;
  color: var(--color-accent-primary);
}

.risk-safe .safety-header svg {
  color: var(--color-success);
}

.risk-moderate .safety-header svg {
  color: var(--color-warning);
}

.risk-risky .safety-header svg {
  color: #f97316;
}

.risk-dangerous .safety-header svg {
  color: var(--color-error);
}

.safety-title {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.safety-level {
  font-size: 1rem;
  font-weight: 600;
  color: var(--color-text-primary);
}

.safety-score {
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
}

.safety-details {
  display: flex;
  flex-direction: column;
  gap: 0.875rem;
}

.safety-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 0.875rem;
}

.safety-label {
  font-weight: 600;
  color: var(--color-text-secondary);
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
}

.safety-value {
  color: var(--color-text-primary);
  font-weight: 500;
}

.safety-reasons,
.safety-recommendations {
  display: flex;
  flex-direction: column;
}

.safety-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.safety-list li {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  padding-left: 1.25rem;
  position: relative;
  line-height: 1.5;
}

.safety-list li::before {
  content: '•';
  position: absolute;
  left: 0.5rem;
  color: var(--color-accent-primary);
  opacity: 0.5;
}

.analyzing {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
  background: rgba(139, 115, 85, 0.04);
  border-radius: var(--radius-sm);
  margin-bottom: 1.5rem;
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  border: 1px solid var(--color-border-light);
}

.analyzing .spinner {
  animation: spin 1s linear infinite;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.analyzing-content {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  flex: 1;
}

.analyzing-text {
  font-weight: 600;
  color: var(--color-text-primary);
}

.analyzing-stats {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

.stat-item {
  display: flex;
  align-items: center;
}

.stat-divider {
  opacity: 0.3;
}

@media (max-width: 768px) {
  .dialog {
    width: 95%;
    max-height: 95vh;
  }

  .header {
    padding: 1.5rem;
  }

  .body {
    padding: 1.5rem;
  }

  .footer {
    padding: 1.25rem 1.5rem;
    flex-direction: column;
  }

  .btn {
    width: 100%;
    justify-content: center;
  }

  .progress-details {
    flex-direction: column;
    gap: 1rem;
  }
}
</style>
