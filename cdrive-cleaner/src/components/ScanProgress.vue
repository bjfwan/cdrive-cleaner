<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { IconFolder, IconFile, IconClock, IconSpeed } from './icons';
import { formatBytes, formatTime } from '../utils/format';

interface Props {
  scanning: boolean;
}

defineProps<Props>();

async function cancelScan() {
  try { await invoke('cancel_scan'); } catch {}
}

const scannedFiles = ref(0);
const scannedDirs = ref(0);
const totalSize = ref(0);
const currentPath = ref('');
const elapsedMs = ref(0);
const filesPerSecond = ref(0);
const progressPercent = ref(0);

const targetFiles = ref(0);
const targetDirs = ref(0);
const targetSize = ref(0);
const targetPercent = ref(0);

const formattedSize = computed(() => formatBytes(totalSize.value));
const formattedSpeed = computed(() => {
  const speed = filesPerSecond.value;
  if (speed === 0) return '计算中...';
  if (speed < 10) return `${speed.toFixed(1)} 文件/秒`;
  return `${Math.round(speed)} 文件/秒`;
});
const formattedTime = computed(() => formatTime(elapsedMs.value));

// 平滑动画
function smoothUpdate() {
  const fileDiff = Math.abs(targetFiles.value - scannedFiles.value);
  const dirDiff = Math.abs(targetDirs.value - scannedDirs.value);
  const sizeDiff = Math.abs(targetSize.value - totalSize.value);
  const percentDiff = Math.abs(targetPercent.value - progressPercent.value);
  
  // 根据差距大小动态调整平滑系数
  const fileSmoothFactor = fileDiff > 10000 ? 0.3 : fileDiff > 1000 ? 0.2 : 0.1;
  const dirSmoothFactor = dirDiff > 100 ? 0.3 : 0.15;
  const sizeSmoothFactor = sizeDiff > 1024 * 1024 * 1024 ? 0.3 : 0.15; // 1GB
  const percentSmoothFactor = 0.2;
  
  scannedFiles.value += (targetFiles.value - scannedFiles.value) * fileSmoothFactor;
  scannedDirs.value += (targetDirs.value - scannedDirs.value) * dirSmoothFactor;
  totalSize.value += (targetSize.value - totalSize.value) * sizeSmoothFactor;
  progressPercent.value += (targetPercent.value - progressPercent.value) * percentSmoothFactor;
  
  // 如果接近目标值，直接设置为目标值
  if (fileDiff < 50) {
    scannedFiles.value = targetFiles.value;
  }
  if (dirDiff < 2) {
    scannedDirs.value = targetDirs.value;
  }
  if (sizeDiff < 10 * 1024 * 1024) { // 10MB
    totalSize.value = targetSize.value;
  }
  if (percentDiff < 1) {
    progressPercent.value = targetPercent.value;
  }
}

let unlisten: (() => void) | null = null;
let animationFrame: number | null = null;

function startAnimation() {
  const animate = () => {
    smoothUpdate();
    animationFrame = requestAnimationFrame(animate);
  };
  animationFrame = requestAnimationFrame(animate);
}

function stopAnimation() {
  if (animationFrame !== null) {
    cancelAnimationFrame(animationFrame);
    animationFrame = null;
  }
}

onMounted(async () => {
  startAnimation();
  
  // 只监听快速扫描的进度事件
  unlisten = await listen<{ scanned_files: number; scanned_dirs: number; total_size: number; current_path: string; elapsed_ms: number; files_per_second: number; progress_percent: number }>('quick-scan-progress', (event) => {
    const progress = event.payload;
    
    targetFiles.value = progress.scanned_files;
    targetDirs.value = progress.scanned_dirs;
    targetSize.value = progress.total_size;
    currentPath.value = progress.current_path;
    elapsedMs.value = progress.elapsed_ms;
    filesPerSecond.value = progress.files_per_second;
    targetPercent.value = progress.progress_percent || 0;
  });
});

onUnmounted(() => {
  stopAnimation();
  if (unlisten) {
    unlisten();
  }
});
</script>

<template>
  <div v-if="scanning" class="scan-progress">
    <div class="progress-container">
      <div class="progress-header">
        <div class="progress-icon">
          <svg class="spinner" width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-dasharray="60 20"/>
          </svg>
        </div>
        <div class="progress-title">
          <h3>快速扫描</h3>
          <p class="current-path">{{ currentPath || '准备中...' }}</p>
        </div>
        <button class="cancel-btn" @click="cancelScan">取消</button>
      </div>
      
      <div class="progress-stats">
        <div class="stat-card">
          <div class="stat-icon">
            <IconFolder :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ Math.round(scannedDirs).toLocaleString() }}</div>
            <div class="stat-label">目录</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <IconFile :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ Math.round(scannedFiles).toLocaleString() }}</div>
            <div class="stat-label">文件</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <IconClock :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ formattedTime }}</div>
            <div class="stat-label">已用时间</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <IconSpeed :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ formattedSpeed }}</div>
            <div class="stat-label">扫描速度</div>
          </div>
        </div>
      </div>
      
      <div class="progress-bar-section">
        <div class="progress-bar-track">
          <div class="progress-bar-fill" :style="{ width: `${Math.min(progressPercent, 100)}%` }"></div>
        </div>
        <div class="progress-info">
          <span class="progress-size">{{ formattedSize }}</span>
          <span class="progress-percent">{{ Math.round(progressPercent) }}%</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scan-progress {
  position: fixed;
  inset: 0;
  padding: 1rem;
  background: rgba(18, 18, 18, 0.38);
  backdrop-filter: blur(24px) saturate(120%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  animation: fadeIn 0.35s ease-out;
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.progress-container {
  width: min(100%, 40rem);
  padding: 2rem;
  border-radius: var(--radius-xl);
  background:
    radial-gradient(circle at top right, rgba(215, 230, 242, 0.82), transparent 30%),
    linear-gradient(180deg, rgba(255, 255, 255, 0.9), rgba(247, 241, 232, 0.94));
  border: 1px solid rgba(255, 255, 255, 0.28);
  box-shadow: var(--shadow-xl);
  animation: slideUp 0.42s cubic-bezier(0.16, 1, 0.3, 1);
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(40px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.cancel-btn {
  margin-left: auto;
  padding: 0.62rem 1rem;
  border: 1px solid rgba(220, 38, 38, 0.18);
  border-radius: 1rem;
  background: rgba(220, 38, 38, 0.06);
  color: var(--color-error);
  font-size: 0.82rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base), box-shadow var(--transition-base);
  flex-shrink: 0;
}

.cancel-btn:hover {
  transform: translateY(-1px);
  background: rgba(220, 38, 38, 0.12);
  box-shadow: var(--shadow-xs);
}

.progress-header {
  display: flex;
  align-items: center;
  gap: 1.25rem;
  margin-bottom: 2rem;
}

.progress-icon {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  border-radius: 1rem;
  color: var(--color-text-inverse);
  flex-shrink: 0;
  box-shadow: var(--shadow-sm);
}

.spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.progress-title h3 {
  font-size: 1.55rem;
  color: var(--color-text-primary);
  margin: 0 0 0.375rem 0;
}

.current-path {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.progress-stats {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 1rem;
  margin-bottom: 2rem;
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1.25rem;
  background: rgba(255, 255, 255, 0.68);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.stat-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(23, 23, 23, 0.06);
  border-radius: 0.9rem;
  color: var(--color-highlight);
  flex-shrink: 0;
}

.stat-content {
  flex: 1;
  min-width: 0;
}

.stat-value {
  font-size: 1.2rem;
  font-weight: 800;
  color: var(--color-text-primary);
  margin-bottom: 0.125rem;
}

.stat-label {
  font-size: 0.75rem;
  font-weight: 800;
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.12em;
}

.progress-bar-section {
  background: linear-gradient(135deg, rgba(15, 118, 110, 0.08) 0%, rgba(37, 99, 235, 0.08) 100%);
  border-radius: var(--radius-md);
  padding: 1.5rem;
  border: 1px solid rgba(15, 118, 110, 0.12);
}

.progress-bar-track {
  height: 8px;
  background: rgba(23, 23, 23, 0.08);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 1rem;
}

.progress-bar-fill {
  height: 100%;
  background: linear-gradient(90deg, rgba(15, 118, 110, 0.9) 0%, rgba(37, 99, 235, 0.78) 100%);
  border-radius: 4px;
  transition: width 0.3s ease-out;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.progress-size {
  font-size: 1rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.progress-percent {
  font-size: 1rem;
  font-weight: 800;
  color: var(--color-highlight);
}

@media (max-width: 600px) {
  .progress-container {
    padding: 1.4rem;
  }

  .progress-stats {
    grid-template-columns: 1fr;
  }

  .progress-title h3 {
    font-size: 1.25rem;
  }

  .stat-value {
    font-size: 1.125rem;
  }
}
</style>
