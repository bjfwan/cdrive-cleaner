<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { IconFolder, IconSpeed } from './icons';
import { useThrottledShallowRef } from '../composables/useThrottledRef';

interface Props {
  scanning: boolean;
}

defineProps<Props>();

interface IncrementalState {
  phase: string;
  totalDirs: number;
  checkedDirs: number;
  changedDirs: number;
  scannedDirs: number;
}

const INITIAL: IncrementalState = {
  phase: 'detecting',
  totalDirs: 0,
  checkedDirs: 0,
  changedDirs: 0,
  scannedDirs: 0,
};

const incremental = useThrottledShallowRef<IncrementalState>({ ...INITIAL }, 80);
const state = computed(() => incremental.state.value);

const phaseText = computed(() => {
  switch (state.value.phase) {
    case 'detecting':
      return '检测变化中';
    case 'scanning':
      return '扫描变化的目录';
    case 'merging':
      return '合并数据中';
    case 'completed':
      return '增量扫描完成';
    default:
      return '处理中';
  }
});

const progressPercent = computed(() => {
  const s = state.value;
  if (s.phase === 'detecting') {
    return s.totalDirs > 0 ? (s.checkedDirs / s.totalDirs) * 100 : 0;
  }
  if (s.phase === 'scanning') {
    return s.changedDirs > 0 ? (s.scannedDirs / s.changedDirs) * 100 : 0;
  }
  if (s.phase === 'merging' || s.phase === 'completed') {
    return 100;
  }
  return 0;
});

const statusMessage = computed(() => {
  const s = state.value;
  if (s.phase === 'detecting') {
    return `已检查 ${s.checkedDirs} / ${s.totalDirs} 个目录`;
  }
  if (s.phase === 'scanning') {
    return `发现 ${s.changedDirs} 个变化，已扫描 ${s.scannedDirs} 个`;
  }
  if (s.phase === 'merging') {
    return '正在合并扫描结果...';
  }
  if (s.phase === 'completed') {
    return `完成！共处理 ${s.changedDirs} 个变化`;
  }
  return '';
});

let unlisten: (() => void) | null = null;

onMounted(async () => {
  unlisten = await listen<{
    phase: string;
    total_dirs: number;
    checked_dirs: number;
    changed_dirs: number;
    scanned_dirs: number;
  }>('incremental-scan-progress', (event) => {
    const p = event.payload;
    incremental.push({
      phase: p.phase,
      totalDirs: p.total_dirs,
      checkedDirs: p.checked_dirs,
      changedDirs: p.changed_dirs,
      scannedDirs: p.scanned_dirs,
    });
  });
});

onUnmounted(() => {
  unlisten?.();
  incremental.flush();
});
</script>

<template>
  <div v-if="scanning" class="incremental-scan-progress">
    <div class="progress-container">
      <div class="progress-header">
        <div class="progress-icon">
          <svg class="spinner" width="24" height="24" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-dasharray="60 20"/>
          </svg>
        </div>
        <div class="progress-title">
          <h3>增量扫描</h3>
          <p class="phase-text">{{ phaseText }}</p>
        </div>
      </div>

      <div class="progress-stats">
        <div class="stat-card">
          <div class="stat-icon">
            <IconFolder :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ state.changedDirs }}</div>
            <div class="stat-label">变化目录</div>
          </div>
        </div>

        <div class="stat-card">
          <div class="stat-icon">
            <IconSpeed :size="20" />
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ state.totalDirs }}</div>
            <div class="stat-label">总目录数</div>
          </div>
        </div>
      </div>

      <div class="progress-bar-section">
        <div class="progress-bar-track">
          <div class="progress-bar-fill" :style="{ width: `${Math.min(progressPercent, 100)}%` }"></div>
        </div>
        <div class="progress-info">
          <span class="progress-status">{{ statusMessage }}</span>
          <span class="progress-percent">{{ Math.round(progressPercent) }}%</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.incremental-scan-progress {
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
  background: linear-gradient(135deg, #10b981 0%, #059669 100%);
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
  letter-spacing: 0;
}

.phase-text {
  font-size: 0.875rem;
  color: #78716c;
  margin: 0;
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
  background: linear-gradient(135deg, #f0fdf4 0%, #dcfce7 100%);
  border-radius: 14px;
  border: 1px solid #bbf7d0;
}

.stat-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: white;
  border-radius: 10px;
  color: #10b981;
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
  letter-spacing: 0;
}

.stat-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.progress-bar-section {
  background: linear-gradient(135deg, #f0fdf4 0%, #dcfce7 100%);
  border-radius: 14px;
  padding: 1.5rem;
  border: 1px solid #bbf7d0;
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
  background: linear-gradient(90deg, #10b981 0%, #059669 100%);
  border-radius: 4px;
  transition: width 0.3s ease-out;
}

.progress-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.progress-status {
  font-size: 0.875rem;
  font-weight: 600;
  color: #065f46;
}

.progress-percent {
  font-size: 1rem;
  font-weight: 600;
  color: #065f46;
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
