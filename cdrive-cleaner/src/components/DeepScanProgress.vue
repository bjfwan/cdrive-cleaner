<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { IconSpinner, IconArrowDown } from './icons';
import { formatBytes } from '../utils/format';

interface Props {
  scanning: boolean;
}

defineProps<Props>();

interface DeepScanProgressPayload {
  scanned_files: number;
  scanned_dirs: number;
  total_size: number;
  current_path: string;
  elapsed_ms: number;
  files_per_second: number;
  progress_percent: number;
}

async function cancelScan() {
  try { await invoke('cancel_scan'); } catch {}
}

const scannedFiles = ref(0);
const totalSize = ref(0);
const progressPercent = ref(0);
const filesPerSecond = ref(0);
const currentPath = ref('');
const expanded = ref(false);

const formattedSize = computed(() => formatBytes(totalSize.value));
const formattedSpeed = computed(() => {
  const speed = filesPerSecond.value;
  if (speed === 0) return '计算中...';
  if (speed < 10) return `${speed.toFixed(1)} 文件/秒`;
  return `${Math.round(speed)} 文件/秒`;
});
const phaseText = computed(() => {
  if (!currentPath.value) {
    return '正在准备深度扫描...';
  }

  if (
    currentPath.value.includes('MFT') ||
    currentPath.value.endsWith('...') ||
    currentPath.value.includes('扫描中')
  ) {
    return currentPath.value;
  }

  return '目录树统计与聚合中';
});
const displayPath = computed(() => {
  if (!currentPath.value || currentPath.value === phaseText.value) {
    return '';
  }

  return currentPath.value;
});

let unlisten: (() => void) | null = null;

onMounted(async () => {
  unlisten = await listen<DeepScanProgressPayload>('deep-scan-progress', (event) => {
    const progress = event.payload;
    scannedFiles.value = progress.scanned_files;
    totalSize.value = progress.total_size;
    progressPercent.value = progress.progress_percent;
    filesPerSecond.value = progress.files_per_second;
    currentPath.value = progress.current_path;
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
      <button class="cancel-btn" @click.stop="cancelScan">取消</button>
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
          <span class="detail-label">阶段</span>
          <span class="detail-value detail-value--path">{{ phaseText }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-label">大小</span>
          <span class="detail-value">{{ formattedSize }}</span>
        </div>
        <div class="detail-item">
          <span class="detail-label">速度</span>
          <span class="detail-value">{{ formattedSpeed }}</span>
        </div>
        <div v-if="displayPath" class="detail-item">
          <span class="detail-label">路径</span>
          <span class="detail-value detail-value--path">{{ displayPath }}</span>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.deep-scan-progress {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.12) 0%, rgba(93, 201, 194, 0.14) 100%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 1rem;
  padding: 1rem;
  margin-top: 0.2rem;
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
  background: rgba(255, 255, 255, 0.12);
  border-radius: 0.8rem;
  color: rgba(248, 246, 242, 0.94);
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
  font-weight: 800;
  color: var(--color-text-inverse);
  margin-bottom: 0.125rem;
}

.subtitle {
  font-size: 0.75rem;
  color: rgba(248, 246, 242, 0.66);
}

.cancel-btn {
  padding: 0.25rem 0.75rem;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 0.8rem;
  background: rgba(255, 255, 255, 0.08);
  color: rgba(248, 246, 242, 0.84);
  font-size: 0.75rem;
  font-weight: 700;
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base);
  flex-shrink: 0;
}

.cancel-btn:hover {
  transform: translateY(-1px);
  background: rgba(220, 38, 38, 0.12);
}

.expand-icon {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: rgba(248, 246, 242, 0.72);
  transition: transform var(--transition-base);
  flex-shrink: 0;
}

.expand-icon.expanded {
  transform: rotate(180deg);
}

.progress-bar {
  height: 6px;
  background: rgba(255, 255, 255, 0.12);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, rgba(255, 255, 255, 0.95) 0%, rgba(93, 201, 194, 0.86) 100%);
  border-radius: 3px;
  transition: width 0.3s ease-out;
}

.progress-details {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
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
  color: rgba(248, 246, 242, 0.62);
  font-weight: 700;
}

.detail-value {
  font-size: 0.75rem;
  color: var(--color-text-inverse);
  font-weight: 800;
}

.detail-value--path {
  max-width: 16rem;
  text-align: right;
  line-height: 1.5;
  word-break: break-all;
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
