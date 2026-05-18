<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { IconSpinner } from './icons';
import { formatBytes, formatTime } from '../utils/format';
import { useThrottledShallowRef } from '../composables/useThrottledRef';

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

interface DeepState {
  scannedFiles: number;
  scannedDirs: number;
  totalSize: number;
  progressPercent: number;
  filesPerSecond: number;
  elapsedMs: number;
  currentPath: string;
}

interface IncrementalState {
  phase: string;
  totalDirs: number;
  checkedDirs: number;
  changedDirs: number;
  scannedDirs: number;
}

type Mode = 'deep' | 'incremental';

const INITIAL_DEEP: DeepState = {
  scannedFiles: 0,
  scannedDirs: 0,
  totalSize: 0,
  progressPercent: 0,
  filesPerSecond: 0,
  elapsedMs: 0,
  currentPath: '',
};

const INITIAL_INCREMENTAL: IncrementalState = {
  phase: 'detecting',
  totalDirs: 0,
  checkedDirs: 0,
  changedDirs: 0,
  scannedDirs: 0,
};

const THROTTLE_MS = 80;

const deep = useThrottledShallowRef<DeepState>({ ...INITIAL_DEEP }, THROTTLE_MS);
const incremental = useThrottledShallowRef<IncrementalState>({ ...INITIAL_INCREMENTAL }, THROTTLE_MS);
// `mode` itself updates at most once per scan-mode switch, so a regular
// throttled ref isn't required; the latest value of either throttled state
// drives the readout once `mode` flips.
const modeRef = useThrottledShallowRef<Mode>('deep', THROTTLE_MS);

async function cancelScan() {
  try {
    await invoke('cancel_scan');
  } catch {}
}

const deepState = computed(() => deep.state.value);
const incrementalState = computed(() => incremental.state.value);
const mode = computed<Mode>(() => modeRef.state.value);

const incrementalProgressPercent = computed(() => {
  const s = incrementalState.value;
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

const progressValue = computed(() => {
  const raw = mode.value === 'incremental'
    ? incrementalProgressPercent.value
    : deepState.value.progressPercent;
  if (raw < 0) return 0;
  if (raw > 100) return 100;
  return raw;
});

const formattedSize = computed(() => formatBytes(deepState.value.totalSize));
const formattedTime = computed(() => formatTime(deepState.value.elapsedMs));
const formattedSpeed = computed(() => {
  const speed = deepState.value.filesPerSecond;
  if (speed === 0) return '计算中...';
  if (speed < 10) return `${speed.toFixed(1)} 文件/秒`;
  return `${Math.round(speed)} 文件/秒`;
});

const incrementalPhaseText = computed(() => {
  switch (incrementalState.value.phase) {
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
  const s = incrementalState.value;
  if (s.phase === 'detecting') {
    return `已检查 ${s.checkedDirs.toLocaleString()} / ${s.totalDirs.toLocaleString()} 个目录`;
  }
  if (s.phase === 'scanning') {
    return `发现 ${s.changedDirs.toLocaleString()} 个变化 · 已重扫 ${s.scannedDirs.toLocaleString()} 个`;
  }
  if (s.phase === 'merging') {
    return '正在合并扫描结果...';
  }
  if (s.phase === 'completed') {
    return `完成，共处理 ${s.changedDirs.toLocaleString()} 个变化`;
  }
  return '准备中...';
});

const hasBackendPhaseText = computed(() =>
  /(MFT|USN|初始化|水合|索引|切换到全量深度扫描)/.test(deepState.value.currentPath),
);
const hasRunningPhaseText = computed(() => /(扫描中|聚合|统计|\.\.\.)/.test(deepState.value.currentPath));

const phaseTitle = computed(() => {
  if (mode.value === 'incremental') {
    return `USN 增量更新 · ${incrementalPhaseText.value}`;
  }

  const path = deepState.value.currentPath;
  if (!path) {
    return '正在准备深度扫描';
  }
  if (hasBackendPhaseText.value) {
    return path;
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

  const path = deepState.value.currentPath;
  if (!path) {
    return '正在读取卷能力、准备目录索引，并建立第一批统计结果。';
  }
  if (path.includes('切换到全量深度扫描')) {
    return '增量变化集不足以安全快速合并，系统已自动切换为全量深度扫描，并继续持续上报进度。';
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

  const path = deepState.value.currentPath;
  if (!path || hasBackendPhaseText.value || hasRunningPhaseText.value) {
    return '';
  }
  return path;
});

function resetState() {
  modeRef.push('deep');
  deep.push({ ...INITIAL_DEEP });
  incremental.push({ ...INITIAL_INCREMENTAL });
  // Make sure resets are visible immediately.
  modeRef.flush();
  deep.flush();
  incremental.flush();
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
  if (!('__TAURI_INTERNALS__' in window)) {
    return;
  }

  try {
    const [deepListener, incrementalListener] = await Promise.all([
      listen<DeepScanProgressPayload>('deep-scan-progress', (event) => {
        const p = event.payload;
        modeRef.push('deep');
        deep.push({
          scannedFiles: p.scanned_files,
          scannedDirs: p.scanned_dirs,
          totalSize: p.total_size,
          progressPercent: p.progress_percent,
          filesPerSecond: p.files_per_second,
          elapsedMs: p.elapsed_ms,
          currentPath: p.current_path,
        });
      }),
      listen<IncrementalScanProgressPayload>('incremental-scan-progress', (event) => {
        const p = event.payload;
        modeRef.push('incremental');
        incremental.push({
          phase: p.phase,
          totalDirs: p.total_dirs,
          checkedDirs: p.checked_dirs,
          changedDirs: p.changed_dirs,
          scannedDirs: p.scanned_dirs,
        });
      }),
    ]);

    unlistenDeep = deepListener;
    unlistenIncremental = incrementalListener;
  } catch {}
});

onUnmounted(() => {
  unlistenDeep?.();
  unlistenIncremental?.();
  // Final flush so the last frame is observable for any consumers (e.g. tests).
  deep.flush();
  incremental.flush();
  modeRef.flush();
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
          <span>{{ incrementalState.checkedDirs.toLocaleString() }} / {{ incrementalState.totalDirs.toLocaleString() }} 已检查</span>
          <span>{{ incrementalState.scannedDirs.toLocaleString() }} / {{ incrementalState.changedDirs.toLocaleString() }} 已重扫</span>
        </template>
        <template v-else>
          <span>{{ deepState.scannedFiles.toLocaleString() }} 个文件</span>
          <span>{{ deepState.scannedDirs.toLocaleString() }} 个目录</span>
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
        <strong>{{ deepState.scannedFiles.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>扫描目录</span>
        <strong>{{ deepState.scannedDirs.toLocaleString() }}</strong>
      </div>
    </div>

    <div v-else class="progress-metrics">
      <div class="metric-tile">
        <span>增量阶段</span>
        <strong>{{ incrementalPhaseText }}</strong>
      </div>

      <div class="metric-tile">
        <span>已检查目录</span>
        <strong>{{ incrementalState.checkedDirs.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>变化目录</span>
        <strong>{{ incrementalState.changedDirs.toLocaleString() }}</strong>
      </div>

      <div class="metric-tile">
        <span>已重扫目录</span>
        <strong>{{ incrementalState.scannedDirs.toLocaleString() }}</strong>
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
