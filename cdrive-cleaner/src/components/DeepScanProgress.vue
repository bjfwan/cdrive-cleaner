<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { IconSpinner } from './icons';
import { formatBytes, formatTime } from '../utils/format';

interface Props {
  scanning: boolean;
}

const props = defineProps<Props>();

interface DeepScanProgressPayload {
  scanned_files: number;
  scanned_dirs: number;
  total_size: number;
  current_path: string;
  elapsed_ms: number;
  files_per_second: number;
  progress_percent: number;
}

interface IncrementalScanProgressPayload {
  phase: string;
  total_dirs: number;
  checked_dirs: number;
  changed_dirs: number;
  scanned_dirs: number;
}

async function cancelScan() {
  try {
    await invoke('cancel_scan');
  } catch {}
}

const mode = ref<'deep' | 'incremental'>('deep');
const scannedFiles = ref(0);
const scannedDirs = ref(0);
const totalSize = ref(0);
const progressPercent = ref(0);
const filesPerSecond = ref(0);
const elapsedMs = ref(0);
const currentPath = ref('');

const incrementalPhase = ref('detecting');
const totalIncrementalDirs = ref(0);
const checkedDirs = ref(0);
const changedDirs = ref(0);
const rescannedDirs = ref(0);

const incrementalProgressPercent = computed(() => {
  const phase = incrementalPhase.value;
  if (phase === 'detecting') {
    return totalIncrementalDirs.value > 0
      ? (checkedDirs.value / totalIncrementalDirs.value) * 100
      : 0;
  }
  if (phase === 'scanning') {
    return changedDirs.value > 0 ? (rescannedDirs.value / changedDirs.value) * 100 : 0;
  }
  if (phase === 'merging' || phase === 'completed') {
    return 100;
  }
  return 0;
});

const progressValue = computed(() => {
  const raw = mode.value === 'incremental' ? incrementalProgressPercent.value : progressPercent.value;
  return Math.min(Math.max(raw, 0), 100);
});
const formattedSize = computed(() => formatBytes(totalSize.value));
const formattedTime = computed(() => formatTime(elapsedMs.value));
const formattedSpeed = computed(() => {
  const speed = filesPerSecond.value;
  if (speed === 0) return '计算中...';
  if (speed < 10) return `${speed.toFixed(1)} 文件/秒`;
  return `${Math.round(speed)} 文件/秒`;
});

const incrementalPhaseText = computed(() => {
  switch (incrementalPhase.value) {
    case 'detecting':
      return '检测变化中';
    case 'scanning':
      return '重扫变化目录中';
    case 'merging':
      return '合并结果中';
    case 'completed':
      return '增量更新完成';
    default:
      return '处理中';
  }
});
const incrementalStatusLine = computed(() => {
  if (incrementalPhase.value === 'detecting') {
    return `已检查 ${checkedDirs.value.toLocaleString()} / ${totalIncrementalDirs.value.toLocaleString()} 个目录`;
  }
  if (incrementalPhase.value === 'scanning') {
    return `发现 ${changedDirs.value.toLocaleString()} 个变化 · 已重扫 ${rescannedDirs.value.toLocaleString()} 个`;
  }
  if (incrementalPhase.value === 'merging') {
    return '正在合并扫描结果...';
  }
  if (incrementalPhase.value === 'completed') {
    return `完成，共处理 ${changedDirs.value.toLocaleString()} 个变化`;
  }
  return '准备中...';
});

const hasBackendPhaseText = computed(() =>
  /(MFT|USN|初始化|水合|索引)/.test(currentPath.value),
);
const hasRunningPhaseText = computed(() =>
  /(扫描中|聚合|统计|\.\.\.)/.test(currentPath.value),
);
const phaseTitle = computed(() => {
  if (mode.value === 'incremental') {
    return `USN 增量更新 · ${incrementalPhaseText.value}`;
  }

  if (!currentPath.value) {
    return '正在准备深度扫描';
  }

  if (hasBackendPhaseText.value) {
    return currentPath.value;
  }

  if (hasRunningPhaseText.value) {
    return '目录树统计与聚合中';
  }

  return '正在构建完整目录树';
});
const phaseDescription = computed(() => {
  if (mode.value === 'incremental') {
    return '检测到缓存后，将优先按 USN 变化集更新目录统计，并合并为新的快照。';
  }

  if (!currentPath.value) {
    return '正在读取卷能力、准备目录索引，并建立第一批统计结果。';
  }

  if (hasBackendPhaseText.value) {
    return '底层索引建立完成后，会继续刷新目录体积、层级结构和可导航快照。';
  }

  return '扫描结果会持续更新到分析页，完成后即可直接按目录层级继续下钻。';
});
const displayPath = computed(() => {
  if (mode.value === 'incremental') {
    return incrementalStatusLine.value;
  }

  if (!currentPath.value || hasBackendPhaseText.value || hasRunningPhaseText.value) {
    return '';
  }

  return currentPath.value;
});

function resetState() {
  mode.value = 'deep';
  scannedFiles.value = 0;
  scannedDirs.value = 0;
  totalSize.value = 0;
  progressPercent.value = 0;
  filesPerSecond.value = 0;
  elapsedMs.value = 0;
  currentPath.value = '';

  incrementalPhase.value = 'detecting';
  totalIncrementalDirs.value = 0;
  checkedDirs.value = 0;
  changedDirs.value = 0;
  rescannedDirs.value = 0;
}

watch(
  () => props.scanning,
  (scanning) => {
    if (scanning) {
      resetState();
    }
  },
  { immediate: true },
);

let unlistenDeep: (() => void) | null = null;
let unlistenIncremental: (() => void) | null = null;

onMounted(async () => {
  const [deepListener, incrementalListener] = await Promise.all([
    listen<DeepScanProgressPayload>('deep-scan-progress', (event) => {
      const progress = event.payload;
      mode.value = 'deep';
      scannedFiles.value = progress.scanned_files;
      scannedDirs.value = progress.scanned_dirs;
      totalSize.value = progress.total_size;
      progressPercent.value = progress.progress_percent;
      filesPerSecond.value = progress.files_per_second;
      elapsedMs.value = progress.elapsed_ms;
      currentPath.value = progress.current_path;
    }),
    listen<IncrementalScanProgressPayload>('incremental-scan-progress', (event) => {
      const progress = event.payload;
      mode.value = 'incremental';
      incrementalPhase.value = progress.phase;
      totalIncrementalDirs.value = progress.total_dirs;
      checkedDirs.value = progress.checked_dirs;
      changedDirs.value = progress.changed_dirs;
      rescannedDirs.value = progress.scanned_dirs;
    }),
  ]);

  unlistenDeep = deepListener;
  unlistenIncremental = incrementalListener;
});

onUnmounted(() => {
  unlistenDeep?.();
  unlistenIncremental?.();
});
</script>

<template>
  <div v-if="scanning" class="deep-scan-progress">
    <div class="progress-top">
      <div class="progress-status">
        <div class="spinner-shell">
          <IconSpinner :size="18" />
        </div>

        <div class="progress-copy">
          <span class="status-badge">
            <span class="status-dot"></span>
            {{ mode === 'incremental' ? '增量更新中' : '深度扫描进行中' }}
          </span>
          <div class="progress-title">{{ phaseTitle }}</div>
          <p class="progress-description">{{ phaseDescription }}</p>
        </div>
      </div>

      <div class="progress-actions">
        <div class="progress-percent">
          <span>当前进度</span>
          <strong>{{ Math.round(progressValue) }}%</strong>
        </div>
        <button class="cancel-btn" @click="cancelScan">取消</button>
      </div>
    </div>

    <div class="progress-bar-section">
      <div class="progress-bar-track">
        <div class="progress-bar-fill" :style="{ width: `${progressValue}%` }"></div>
      </div>

      <div class="progress-counts">
        <template v-if="mode === 'incremental'">
          <span>{{ checkedDirs.toLocaleString() }} / {{ totalIncrementalDirs.toLocaleString() }} 已检查</span>
          <span>{{ rescannedDirs.toLocaleString() }} / {{ changedDirs.toLocaleString() }} 已重扫</span>
        </template>
        <template v-else>
          <span>{{ scannedFiles.toLocaleString() }} 个文件</span>
          <span>{{ scannedDirs.toLocaleString() }} 个目录</span>
        </template>
      </div>
    </div>

    <div class="progress-context">
      <span class="context-label">{{ mode === 'incremental' ? '当前状态' : '当前路径' }}</span>
      <strong>{{ displayPath || (mode === 'incremental' ? '正在准备增量更新' : '正在准备目录索引与分层统计') }}</strong>
    </div>

    <div v-if="mode === 'deep'" class="progress-metrics">
      <div class="metric-tile">
        <span>扫描速度</span>
        <strong>{{ formattedSpeed }}</strong>
      </div>

      <div class="metric-tile">
        <span>累计体积</span>
        <strong>{{ formattedSize }}</strong>
      </div>

      <div class="metric-tile">
        <span>已用时间</span>
        <strong>{{ formattedTime }}</strong>
      </div>

      <div class="metric-tile">
        <span>扫描文件</span>
        <strong>{{ scannedFiles.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>扫描目录</span>
        <strong>{{ scannedDirs.toLocaleString() }}</strong>
      </div>
    </div>

    <div v-else class="progress-metrics">
      <div class="metric-tile">
        <span>增量阶段</span>
        <strong>{{ incrementalPhaseText }}</strong>
      </div>

      <div class="metric-tile">
        <span>已检查目录</span>
        <strong>{{ checkedDirs.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>变化目录</span>
        <strong>{{ changedDirs.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>已重扫目录</span>
        <strong>{{ rescannedDirs.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>当前进度</span>
        <strong>{{ Math.round(progressValue) }}%</strong>
      </div>
    </div>
  </div>
</template>

<style scoped>
.deep-scan-progress {
  padding: 1.05rem 1.1rem 1.1rem;
  border-radius: var(--radius-lg);
  background:
    radial-gradient(circle at top right, rgba(93, 201, 194, 0.18), transparent 34%),
    linear-gradient(135deg, rgba(255, 255, 255, 0.94), rgba(240, 247, 244, 0.97));
  border: 1px solid rgba(15, 118, 110, 0.12);
  box-shadow: 0 18px 34px rgba(26, 20, 14, 0.08);
  backdrop-filter: blur(18px);
}

.progress-top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
}

.progress-status {
  display: flex;
  align-items: flex-start;
  gap: 0.9rem;
  flex: 1;
  min-width: 0;
}

.spinner-shell {
  width: 2.9rem;
  height: 2.9rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 1rem;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.78);
  flex-shrink: 0;
}

.spinner-shell :deep(svg) {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.progress-copy {
  min-width: 0;
}

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  margin-bottom: 0.42rem;
  padding: 0.36rem 0.68rem;
  border-radius: var(--radius-pill);
  background: rgba(15, 118, 110, 0.08);
  color: var(--color-highlight);
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

.status-dot {
  width: 0.48rem;
  height: 0.48rem;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 0 0 rgba(15, 118, 110, 0.28);
  animation: pulse 1.8s ease-out infinite;
}

@keyframes pulse {
  0% {
    box-shadow: 0 0 0 0 rgba(15, 118, 110, 0.28);
  }

  70% {
    box-shadow: 0 0 0 0.45rem rgba(15, 118, 110, 0);
  }

  100% {
    box-shadow: 0 0 0 0 rgba(15, 118, 110, 0);
  }
}

.progress-title {
  font-size: 1.08rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.progress-description {
  margin-top: 0.28rem;
  font-size: 0.84rem;
  line-height: 1.6;
  color: var(--color-text-secondary);
}

.progress-actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-shrink: 0;
}

.progress-percent {
  text-align: right;
}

.progress-percent span {
  display: block;
  margin-bottom: 0.12rem;
  font-size: 0.7rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.progress-percent strong {
  font-size: 1.7rem;
  line-height: 0.92;
  color: var(--color-text-primary);
}

.cancel-btn {
  padding: 0.68rem 0.95rem;
  border: 1px solid rgba(220, 38, 38, 0.12);
  border-radius: 0.9rem;
  background: rgba(255, 255, 255, 0.74);
  color: var(--color-error);
  font-size: 0.8rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base), box-shadow var(--transition-base);
}

.cancel-btn:hover {
  transform: translateY(-1px);
  background: rgba(220, 38, 38, 0.08);
  box-shadow: var(--shadow-xs);
}

.progress-bar-section {
  margin-top: 1rem;
}

.progress-bar-track {
  height: 0.72rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.08);
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #1d5e62 0%, #2f8c84 56%, #70c9c1 100%);
  box-shadow: 0 0 18px rgba(47, 140, 132, 0.24);
  transition: width 0.3s ease-out;
}

.progress-counts {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 0.55rem 1rem;
  margin-top: 0.6rem;
  font-size: 0.8rem;
  font-weight: 700;
  color: var(--color-text-secondary);
}

.progress-context {
  margin-top: 0.95rem;
  padding: 0.82rem 0.92rem;
  border-radius: 1rem;
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(46, 33, 18, 0.06);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.76);
}

.context-label,
.metric-tile span {
  display: block;
  margin-bottom: 0.24rem;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.progress-context strong {
  display: block;
  font-size: 0.84rem;
  line-height: 1.6;
  color: var(--color-text-primary);
  word-break: break-all;
}

.progress-metrics {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 0.75rem;
  margin-top: 0.85rem;
}

.metric-tile {
  padding: 0.78rem 0.85rem;
  border-radius: 1rem;
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(46, 33, 18, 0.06);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.76);
}

.metric-tile strong {
  display: block;
  font-size: 0.9rem;
  line-height: 1.4;
  color: var(--color-text-primary);
}

@media (max-width: 1100px) {
  .progress-metrics {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 760px) {
  .progress-top {
    flex-direction: column;
  }

  .progress-actions {
    width: 100%;
    justify-content: space-between;
  }

  .progress-metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 540px) {
  .deep-scan-progress {
    padding: 0.9rem;
  }

  .progress-status {
    flex-direction: column;
  }

  .progress-actions {
    flex-direction: column;
    align-items: stretch;
  }

  .progress-percent {
    text-align: left;
  }

  .cancel-btn {
    width: 100%;
  }

  .progress-metrics {
    grid-template-columns: 1fr;
  }
}
</style>
