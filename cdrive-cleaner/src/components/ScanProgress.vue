<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen } from '@tauri-apps/api/event';

interface Props {
  scanning: boolean;
}

defineProps<Props>();

const scannedFiles = ref(0);
const scannedDirs = ref(0);
const totalSize = ref(0);
const currentPath = ref('');
const elapsedMs = ref(0);
const filesPerSecond = ref(0);
const progressPercent = ref(0);

// 平滑处理的目标值
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

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatTime(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  
  if (hours > 0) {
    return `${hours}:${String(minutes % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
  }
  return `${minutes}:${String(seconds % 60).padStart(2, '0')}`;
}

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
  unlisten = await listen('quick-scan-progress', (event: any) => {
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
      </div>
      
      <div class="progress-stats">
        <div class="stat-card">
          <div class="stat-icon">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M4 4H10L12 6H16C17.1046 6 18 6.89543 18 8V14C18 15.1046 17.1046 16 16 16H4C2.89543 16 2 15.1046 2 14V6C2 4.89543 2.89543 4 4 4Z" stroke="currentColor" stroke-width="1.5"/>
            </svg>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ Math.round(scannedDirs).toLocaleString() }}</div>
            <div class="stat-label">目录</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M6 3H11L14 6V15C14 16.1046 13.1046 17 12 17H6C4.89543 17 4 16.1046 4 15V5C4 3.89543 4.89543 3 6 3Z" stroke="currentColor" stroke-width="1.5"/>
              <path d="M11 3V6H14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ Math.round(scannedFiles).toLocaleString() }}</div>
            <div class="stat-label">文件</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 2C14.4183 2 18 5.58172 18 10C18 14.4183 14.4183 18 10 18C5.58172 18 2 14.4183 2 10C2 5.58172 5.58172 2 10 2Z" stroke="currentColor" stroke-width="1.5"/>
              <path d="M10 6V10L13 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ formattedTime }}</div>
            <div class="stat-label">已用时间</div>
          </div>
        </div>
        
        <div class="stat-card">
          <div class="stat-icon">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M3 10H17M17 10L13 6M17 10L13 14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
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
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  animation: fadeIn 0.3s ease-out;
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
  background: white;
  border-radius: 24px;
  padding: 2.5rem;
  width: 90%;
  max-width: 600px;
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.3);
  animation: slideUp 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
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
  background: linear-gradient(135deg, #007aff 0%, #5856d6 100%);
  border-radius: 14px;
  color: white;
  flex-shrink: 0;
}

.spinner {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.progress-title h3 {
  font-size: 1.5rem;
  font-weight: 600;
  color: #2c2c2c;
  margin: 0 0 0.375rem 0;
  letter-spacing: -0.02em;
}

.current-path {
  font-size: 0.875rem;
  color: #78716c;
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
  background: linear-gradient(135deg, #fafaf9 0%, #f5f5f4 100%);
  border-radius: 14px;
  border: 1px solid #e7e5e4;
}

.stat-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  border-radius: 10px;
  color: #007aff;
  flex-shrink: 0;
}

.stat-content {
  flex: 1;
  min-width: 0;
}

.stat-value {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.125rem;
  letter-spacing: -0.01em;
}

.stat-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.progress-bar-section {
  background: linear-gradient(135deg, #f0f9ff 0%, #e0f2fe 100%);
  border-radius: 14px;
  padding: 1.5rem;
  border: 1px solid #bae6fd;
}

.progress-bar-track {
  height: 8px;
  background: rgba(255, 255, 255, 0.6);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 1rem;
}

.progress-bar-fill {
  height: 100%;
  background: linear-gradient(90deg, #0ea5e9 0%, #06b6d4 100%);
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
  font-weight: 600;
  color: #0c4a6e;
}

.progress-percent {
  font-size: 1rem;
  font-weight: 600;
  color: #0c4a6e;
}

@media (max-width: 600px) {
  .progress-container {
    padding: 2rem;
    max-width: 95%;
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
