<script setup lang="ts">
import { ref, computed } from 'vue';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
}

interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
}

interface DiskInfo {
  drive_letter: string;
  label: string;
  free_space: number;
}

interface Props {
  show: boolean;
  selectedDir: DirectoryNode | null;
  selectedFile: FileInfo | null;
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

const itemName = computed(() => props.selectedFile ? '迁移文件' : '迁移目录');
const itemPath = computed(() => props.selectedFile?.path || props.selectedDir?.path);
const itemSize = computed(() => props.selectedFile?.size || props.selectedDir?.size || 0);

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

function close() {
  targetDisk.value = '';
  migrating.value = false;
  migrationError.value = '';
  migrationSuccess.value = false;
  migrationResult.value = null;
  emit('close');
}

async function startMigration() {
  if (!targetDisk.value) {
    migrationError.value = '请选择目标磁盘';
    return;
  }

  if (!itemPath.value) {
    migrationError.value = '未选择要迁移的项目';
    return;
  }

  migrating.value = true;
  migrationError.value = '';

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const result = await invoke('migrate_file', {
      source: itemPath.value,
      targetDisk: targetDisk.value,
      linkType: null
    });
    
    migrationSuccess.value = true;
    migrationResult.value = result;
    migrating.value = false;
  } catch (err) {
    console.error('迁移失败:', err);
    migrationError.value = String(err);
    migrating.value = false;
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
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M15 5L5 15M5 5L15 15" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
          </button>
        </div>
        
        <div class="body">
          <div class="info-section">
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
          
          <div class="warning">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 2L2 17H18L10 2Z" stroke="#ff9500" stroke-width="2" stroke-linejoin="round"/>
              <path d="M10 8V12" stroke="#ff9500" stroke-width="2" stroke-linecap="round"/>
              <circle cx="10" cy="15" r="0.5" fill="#ff9500"/>
            </svg>
            <span>迁移后将在原位置创建符号链接，程序可正常访问</span>
          </div>
          
          <div v-if="migrationError" class="error">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <circle cx="10" cy="10" r="8" stroke="#ef4444" stroke-width="2"/>
              <path d="M10 6V10M10 14H10.01" stroke="#ef4444" stroke-width="2" stroke-linecap="round"/>
            </svg>
            <span>{{ migrationError }}</span>
          </div>
          
          <div v-if="migrating" class="progress">
            <div class="progress-header">
              <span class="progress-label">正在迁移...</span>
            </div>
            <div class="progress-bar-wrapper">
              <div class="progress-bar-track">
                <div class="progress-bar-fill"></div>
              </div>
            </div>
            <div class="progress-info">
              <span class="progress-text">正在复制文件到目标磁盘</span>
            </div>
          </div>
        </div>
        
        <div class="footer">
          <button class="btn btn-secondary" @click="close" :disabled="migrating">取消</button>
          <button class="btn btn-primary" @click="startMigration" :disabled="!targetDisk || migrating">
            <span v-if="migrating">
              <svg class="spinner" width="16" height="16" viewBox="0 0 16 16" fill="none">
                <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-dasharray="30 10"/>
              </svg>
              迁移中
            </span>
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
        <h3 class="success-title">迁移成功</h3>
        <p class="success-message">文件已成功迁移到目标磁盘，并在原位置创建了符号链接</p>
        <div class="success-details">
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
