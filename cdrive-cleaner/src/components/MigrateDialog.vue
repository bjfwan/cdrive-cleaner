<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { IconClose, IconWarning, IconError, IconShield } from './icons';

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
  free_space: number;
}

interface MigrationSafety {
  risk_level: 'safe' | 'moderate' | 'risky' | 'dangerous';
  safety_score: number;
  can_migrate: boolean;
  reasons: string[];
  recommendations: string[];
  app_type: string;
}

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
  'migrate': [targetDisk: string];
}>();

const targetDisk = ref<string>('');
const migrating = ref(false);
const migrationError = ref<string>('');
const migrationSuccess = ref(false);
const migrationResult = ref<any>(null);
const currentMigratingIndex = ref(0);
const migrationResults = ref<Array<{ path: string; success: boolean; error?: string }>>([]);

const migrationStartTime = ref(0);
const migratedSize = ref(0);
const migrationSpeed = ref(0);
const estimatedTimeRemaining = ref(0);
const elapsedTime = ref(0);
const updateTimer = ref<number | null>(null);

// 安全性分析
const safetyAnalysis = ref<MigrationSafety | null>(null);
const analyzingSafety = ref(false);

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
  const saved = localStorage.getItem('cdrive-cleaner-settings');
  if (saved) {
    try {
      const settings = JSON.parse(saved);
      if (settings.defaultTargetDisk) {
        targetDisk.value = settings.defaultTargetDisk;
      }
    } catch (e) {
      console.error('Failed to load default target disk:', e);
    }
  }
}

async function analyzeSafety() {
  if (!itemPath.value) return;
  
  analyzingSafety.value = true;
  safetyAnalysis.value = null;
  
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const result = await invoke<MigrationSafety>('analyze_migration_safety', {
      path: itemPath.value,
      size: itemSize.value
    });
    safetyAnalysis.value = result;
  } catch (err) {
    console.error('安全性分析失败:', err);
  } finally {
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

function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond === 0) return '0 B/s';
  const k = 1024;
  const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  const i = Math.floor(Math.log(bytesPerSecond) / Math.log(k));
  return `${(bytesPerSecond / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatTime(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)} 秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分 ${Math.round(seconds % 60)} 秒`;
  return `${Math.floor(seconds / 3600)} 小时 ${Math.floor((seconds % 3600) / 60)} 分`;
}

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
  startProgressTimer();

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    
    // 从设置中读取是否创建符号链接
    const saved = localStorage.getItem('cdrive-cleaner-settings');
    let createSymlink = true; // 默认创建
    if (saved) {
      try {
        const settings = JSON.parse(saved);
        createSymlink = settings.createSymlink !== false;
      } catch (e) {
        console.error('Failed to load settings:', e);
      }
    }
    
    const result = await invoke('migrate_file', {
      source: itemPath.value,
      targetDisk: targetDisk.value,
      linkType: createSymlink ? null : 'none'
    });
    
    stopProgressTimer();
    migrationSuccess.value = true;
    migrationResult.value = result;
    migrating.value = false;
  } catch (err) {
    stopProgressTimer();
    console.error('迁移失败:', err);
    migrationError.value = String(err);
    migrating.value = false;
  }
}

async function startBatchMigration() {
  migrating.value = true;
  migrationError.value = '';
  migrationResults.value = [];
  currentMigratingIndex.value = 0;
  migrationStartTime.value = Date.now();
  migratedSize.value = 0;

  const { invoke } = await import('@tauri-apps/api/core');
  const totalSize = itemSize.value;

  // 从设置中读取是否创建符号链接
  const saved = localStorage.getItem('cdrive-cleaner-settings');
  let createSymlink = true; // 默认创建
  if (saved) {
    try {
      const settings = JSON.parse(saved);
      createSymlink = settings.createSymlink !== false;
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  }

  for (let i = 0; i < props.selectedItems.length; i++) {
    currentMigratingIndex.value = i;
    const item = props.selectedItems[i];
    
    try {
      await invoke('migrate_file', {
        source: item.path,
        targetDisk: targetDisk.value,
        linkType: createSymlink ? null : 'none'
      });
      
      migratedSize.value += item.size || 0;
      
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
    } catch (err) {
      console.error(`迁移失败 ${item.path}:`, err);
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
            <span>正在分析安全性...</span>
          </div>
          
          <div class="warning">
            <IconWarning :size="20" />
            <span>迁移后将在原位置创建符号链接，程序可正常访问</span>
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
                <div class="progress-bar-fill"></div>
              </div>
            </div>
            
            <div class="progress-info">
              <span class="progress-text" v-if="isBatchMode && selectedItems[currentMigratingIndex]">
                {{ selectedItems[currentMigratingIndex].name }}
              </span>
              <span class="progress-text" v-else>正在复制文件到目标磁盘</span>
            </div>
            
            <div v-if="isBatchMode && migrationSpeed > 0" class="progress-details">
              <div class="progress-detail-item">
                <span class="detail-label">已传输</span>
                <span class="detail-value">{{ formatBytes(migratedSize) }} / {{ formatBytes(itemSize) }}</span>
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
        <p v-else class="success-message">文件已成功迁移到目标磁盘，并在原位置创建了符号链接</p>
        
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
            <span class="label">耗时</span>
            <span class="value">{{ (migrationResult?.duration_ms / 1000).toFixed(2) }} 秒</span>
          </div>
        </div>
        
        <button class="btn btn-primary btn-full" @click="close">完成</button>
      </div>
    </div>
  </div>
</template>

<style scoped src="./MigrateDialog.css"></style>
