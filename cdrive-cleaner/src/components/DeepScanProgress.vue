<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { IconSpinner, IconArrowDown } from './icons';

interface Props {
  scanning: boolean;
}

defineProps<Props>();

const scannedFiles = ref(0);
const totalSize = ref(0);
const progressPercent = ref(0);
const filesPerSecond = ref(0);
const expanded = ref(false);

const formattedSize = computed(() => formatBytes(totalSize.value));
const formattedSpeed = computed(() => {
  const speed = filesPerSecond.value;
  if (speed === 0) return '计算中...';
  if (speed < 10) return `${speed.toFixed(1)} 文件/秒`;
  return `${Math.round(speed)} 文件/秒`;
});

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

let unlisten: (() => void) | null = null;

onMounted(async () => {
  unlisten = await listen('deep-scan-progress', (event: any) => {
    const progress = event.payload;
    scannedFiles.value = progress.scanned_files;
    totalSize.value = progress.total_size;
    progressPercent.value = progress.progress_percent;
    filesPerSecond.value = progress.files_per_second;
  });
});

onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
});
</script>

<template>
  <div v-if="scanning" class="deep-scan-progress">
    <div class="progress-header" @click="expanded = !expanded">
      <div class="header-left">
        <div class="spinner-icon">
          <IconSpinner :size="16" />
        </div>
        <div class="header-text">
          <div class="title">深度扫描中</div>
          <div class="subtitle">{{ Math.round(progressPercent) }}% · {{ scannedFiles.toLocaleString() }} 文件</div>
        </div>
      </div>
      <div class="expand-icon" :class="{ expanded }">
        <IconArrowDown :size="16" />
      </div>
    </div>
    
    <div class="progress-bar">
      <div class="progress-fill" :style="{ width: `${Math.min(progressPercent, 100)}%` }"></div>
    </div>
    
    <transition name="expand">
      <div v-if="expanded" class="progress-details">
        <div class="detail-item">
          <span class="detail-label">大小</span>
          <span class="detail-value">{{ formattedSize }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-label">速度</span>
          <span class="detail-value">{{ formattedSpeed }}</span>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.deep-scan-progress {
  background: linear-gradient(135deg, #f0f9ff 0%, #e0f2fe 100%);
  border: 1px solid #bae6fd;
  border-radius: 12px;
  padding: 1rem;
  margin: 1rem 0;
}

.progress-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  user-select: none;
  margin-bottom: 0.75rem;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex: 1;
  min-width: 0;
}

.spinner-icon {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  border-radius: 8px;
  color: #0ea5e9;
  flex-shrink: 0;
}

.spinner-icon svg {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.header-text {
  flex: 1;
  min-width: 0;
}

.title {
  font-size: 0.875rem;
  font-weight: 600;
  color: #0c4a6e;
  margin-bottom: 0.125rem;
}

.subtitle {
  font-size: 0.75rem;
  color: #0369a1;
}

.expand-icon {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #0369a1;
  transition: transform 0.2s ease;
  flex-shrink: 0;
}

.expand-icon.expanded {
  transform: rotate(180deg);
}

.progress-bar {
  height: 6px;
  background: rgba(255, 255, 255, 0.6);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, #0ea5e9 0%, #06b6d4 100%);
  border-radius: 3px;
  transition: width 0.3s ease-out;
}

.progress-details {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid #bae6fd;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.detail-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.detail-label {
  font-size: 0.75rem;
  color: #0369a1;
  font-weight: 500;
}

.detail-value {
  font-size: 0.75rem;
  color: #0c4a6e;
  font-weight: 600;
}

.expand-enter-active,
.expand-leave-active {
  transition: all 0.3s ease;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  opacity: 0;
  max-height: 0;
}

.expand-enter-to,
.expand-leave-from {
  opacity: 1;
  max-height: 100px;
}
</style>
