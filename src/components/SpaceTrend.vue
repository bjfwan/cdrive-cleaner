<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { formatBytes } from '../utils/format';

interface DiskSnapshot {
  id: number;
  drive_letter: string;
  total_size: number;
  used_size: number;
  scanned_size: number;
  captured_at: string;
}

interface Props {
  drive: string;
  refreshKey?: number;
}

const props = defineProps<Props>();
const range = ref<7 | 30 | 0>(30);
const snapshots = ref<DiskSnapshot[]>([]);
const loading = ref(false);
const detailOpen = ref(false);

const sparklineWidth = 240;
const sparklineHeight = 64;

watch(
  () => [props.drive, props.refreshKey, range.value] as const,
  () => {
    void load();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onEsc);
});

async function load() {
  if (!props.drive) {
    snapshots.value = [];
    return;
  }
  const driveLetter = normalizeDrive(props.drive);
  if (!driveLetter) {
    snapshots.value = [];
    return;
  }
  loading.value = true;
  try {
    const days = range.value === 0 ? 365 : range.value;
    const data = await invoke<DiskSnapshot[]>('get_space_history', {
      drive: driveLetter,
      days,
    });
    snapshots.value = data;
  } catch {
    snapshots.value = [];
  } finally {
    loading.value = false;
  }
}

function normalizeDrive(input: string): string | null {
  const trimmed = input.trim();
  if (trimmed.length === 0) return null;
  const letter = trimmed[0]?.toUpperCase();
  if (!letter || !/[A-Z]/.test(letter)) return null;
  return `${letter}:\\`;
}

const latest = computed(() => snapshots.value.at(-1) ?? null);

const compareIndex = (days: number) => {
  if (snapshots.value.length === 0) return null;
  const target = Date.now() - days * 24 * 60 * 60 * 1000;
  let best: DiskSnapshot | null = null;
  let bestDiff = Number.POSITIVE_INFINITY;
  for (const snap of snapshots.value) {
    const ts = parseSqlTime(snap.captured_at);
    const diff = Math.abs(ts - target);
    if (diff < bestDiff) {
      bestDiff = diff;
      best = snap;
    }
  }
  return best;
};

const compare7 = computed(() => compareIndex(7));
const compare30 = computed(() => compareIndex(30));

function parseSqlTime(value: string): number {
  const normalized = value.includes('T') ? value : value.replace(' ', 'T') + 'Z';
  const parsed = Date.parse(normalized);
  return Number.isNaN(parsed) ? 0 : parsed;
}

const trendCopy = computed(() => {
  if (!latest.value || !compare30.value) return '记录中…';
  const delta = latest.value.used_size - compare30.value.used_size;
  if (delta === 0) return '过去 30 天没有变化';
  const verb = delta > 0 ? '增加了' : '减少了';
  return `过去 30 天${verb} ${formatBytes(Math.abs(delta))}`;
});

const trendDirection = computed<'up' | 'down' | 'flat'>(() => {
  if (!latest.value || !compare30.value) return 'flat';
  const delta = latest.value.used_size - compare30.value.used_size;
  if (delta > 0) return 'up';
  if (delta < 0) return 'down';
  return 'flat';
});

const sparklinePath = computed(() => buildPath(sparklineWidth, sparklineHeight, snapshots.value));
const sparklineFill = computed(() => buildFillPath(sparklineWidth, sparklineHeight, snapshots.value));

function buildPath(width: number, height: number, points: DiskSnapshot[]): string {
  if (points.length < 2) {
    if (points.length === 1) {
      const y = height / 2;
      return `M 0 ${y} L ${width} ${y}`;
    }
    return '';
  }
  const values = points.map((p) => p.used_size);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = Math.max(max - min, 1);
  const padding = 4;
  const innerH = height - padding * 2;
  const stepX = points.length === 1 ? width : width / (points.length - 1);
  return points
    .map((p, i) => {
      const x = i * stepX;
      const y = padding + innerH - ((p.used_size - min) / span) * innerH;
      return `${i === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(' ');
}

function buildFillPath(width: number, height: number, points: DiskSnapshot[]): string {
  if (points.length < 2) return '';
  const main = buildPath(width, height, points);
  return `${main} L ${width.toFixed(2)} ${height} L 0 ${height} Z`;
}

const detailPath = computed(() => buildPath(540 - 64, 360 - 160, snapshots.value));
const detailFill = computed(() => buildFillPath(540 - 64, 360 - 160, snapshots.value));

function openDetail() {
  if (snapshots.value.length === 0) return;
  detailOpen.value = true;
  document.addEventListener('keydown', onEsc);
}

function closeDetail() {
  detailOpen.value = false;
  document.removeEventListener('keydown', onEsc);
}

function onEsc(e: KeyboardEvent) {
  if (e.key === 'Escape') closeDetail();
}

function formatTimestamp(value: string): string {
  const ts = parseSqlTime(value);
  if (!ts) return value;
  const d = new Date(ts);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  const hh = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd} ${hh}:${mi}`;
}
</script>

<template>
  <section class="trend" :class="{ empty: snapshots.length === 0 }">
    <div v-if="snapshots.length === 0" class="trend-empty">
      <div class="trend-empty-mark"></div>
      <div class="trend-empty-copy">
        <strong>空间趋势</strong>
        <span>扫描完成后这里会出现趋势</span>
      </div>
    </div>

    <div v-else class="trend-row" @click="openDetail" role="button" tabindex="0" @keydown.enter="openDetail">
      <div class="trend-copy">
        <span class="trend-kicker">空间趋势</span>
        <strong>{{ latest ? formatBytes(latest.used_size) : '--' }}</strong>
        <small>{{ trendCopy }}</small>
      </div>

      <svg
        class="trend-chart"
        :viewBox="`0 0 ${sparklineWidth} ${sparklineHeight}`"
        preserveAspectRatio="none"
        :data-direction="trendDirection"
      >
        <defs>
          <linearGradient id="trend-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stop-color="currentColor" stop-opacity="0.32" />
            <stop offset="100%" stop-color="currentColor" stop-opacity="0" />
          </linearGradient>
        </defs>
        <path :d="sparklineFill" fill="url(#trend-fill)" />
        <path
          :d="sparklinePath"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>

      <div class="trend-stats">
        <div class="trend-stat">
          <span>7 天前</span>
          <strong>{{ compare7 ? formatBytes(compare7.used_size) : '--' }}</strong>
        </div>
        <div class="trend-stat">
          <span>30 天前</span>
          <strong>{{ compare30 ? formatBytes(compare30.used_size) : '--' }}</strong>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <transition name="modal">
        <div v-if="detailOpen" class="trend-modal-overlay" @click.self="closeDetail">
          <div class="trend-modal" role="dialog" aria-label="空间趋势详情">
            <header class="trend-modal-head">
              <div>
                <h3>空间趋势</h3>
                <p>{{ trendCopy }}</p>
              </div>
              <div class="trend-tabs">
                <button :class="{ active: range === 7 }" @click="range = 7">7 天</button>
                <button :class="{ active: range === 30 }" @click="range = 30">30 天</button>
                <button :class="{ active: range === 0 }" @click="range = 0">全部</button>
              </div>
              <button class="trend-close" @click="closeDetail" aria-label="关闭">✕</button>
            </header>

            <div class="trend-modal-body">
              <svg
                class="trend-chart-large"
                :viewBox="`0 0 ${540 - 64} ${360 - 160}`"
                preserveAspectRatio="none"
                :data-direction="trendDirection"
              >
                <defs>
                  <linearGradient id="trend-fill-large" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="currentColor" stop-opacity="0.32" />
                    <stop offset="100%" stop-color="currentColor" stop-opacity="0" />
                  </linearGradient>
                </defs>
                <path :d="detailFill" fill="url(#trend-fill-large)" />
                <path
                  :d="detailPath"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>

              <div class="trend-modal-stats">
                <div class="trend-modal-stat">
                  <span>当前已用</span>
                  <strong>{{ latest ? formatBytes(latest.used_size) : '--' }}</strong>
                </div>
                <div class="trend-modal-stat">
                  <span>采样点</span>
                  <strong>{{ snapshots.length }}</strong>
                </div>
                <div class="trend-modal-stat">
                  <span>最早一次</span>
                  <strong>{{ snapshots[0] ? formatTimestamp(snapshots[0].captured_at) : '--' }}</strong>
                </div>
              </div>
            </div>
          </div>
        </div>
      </transition>
    </Teleport>
  </section>
</template>

<style scoped>
.trend {
  display: flex;
  align-items: center;
  padding: 0.75rem 1rem;
  border-radius: var(--radius-md);
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.trend.empty {
  border-style: dashed;
  background: var(--color-surface);
}

.trend-empty {
  display: flex;
  align-items: center;
  gap: 0.85rem;
  width: 100%;
}

.trend-empty-mark {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-sm);
  background: var(--color-highlight-soft);
  position: relative;
  flex-shrink: 0;
}

.trend-empty-mark::after {
  content: '';
  position: absolute;
  inset: 30% 18% 30% 18%;
  border-bottom: 2px dashed var(--color-highlight);
  border-radius: 2px;
}

.trend-empty-copy {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.trend-empty-copy strong {
  font-size: 0.9rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.trend-empty-copy span {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}

.trend-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(180px, 1.25fr) auto;
  gap: 1.1rem;
  align-items: center;
  width: 100%;
  cursor: pointer;
  transition: transform var(--transition-fast);
}

.trend-row:hover {
  transform: translateY(-1px);
}

.trend-row:focus-visible {
  outline: 2px solid var(--color-highlight);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}

.trend-copy {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
  min-width: 0;
}

.trend-kicker {
  font-size: 0.66rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.trend-copy strong {
  font-family: var(--font-display);
  font-size: 1.4rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  line-height: 1;
}

.trend-copy small {
  font-size: 0.78rem;
  color: var(--color-text-secondary);
}

.trend-chart {
  width: 100%;
  height: 56px;
  color: var(--color-highlight);
}

.trend-chart[data-direction='up'] {
  color: var(--color-warning);
}

.trend-chart[data-direction='down'] {
  color: var(--color-success);
}

.trend-chart-large {
  width: 100%;
  height: 200px;
  color: var(--color-highlight);
}

.trend-chart-large[data-direction='up'] {
  color: var(--color-warning);
}

.trend-chart-large[data-direction='down'] {
  color: var(--color-success);
}

.trend-stats {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  text-align: right;
  flex-shrink: 0;
}

.trend-stat {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  justify-content: flex-end;
}

.trend-stat span {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
}

.trend-stat strong {
  font-size: 0.86rem;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  font-weight: 600;
}

.trend-modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 32, 0.42);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2200;
  padding: 1.5rem;
}

.trend-modal {
  width: min(540px, 95vw);
  max-height: min(640px, 88vh);
  background: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.trend-modal-head {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 1rem;
  padding: 1rem 1.2rem;
  border-bottom: 1px solid var(--color-border-light);
}

.trend-modal-head h3 {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.trend-modal-head p {
  margin-top: 0.18rem;
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
}

.trend-tabs {
  display: flex;
  gap: 0.3rem;
  background: var(--color-surface);
  padding: 0.25rem;
  border-radius: var(--radius-pill);
}

.trend-tabs button {
  padding: 0.4rem 0.8rem;
  border: none;
  background: transparent;
  color: var(--color-text-tertiary);
  font-size: 0.8rem;
  font-weight: 600;
  border-radius: var(--radius-pill);
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.trend-tabs button.active {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
}

.trend-close {
  position: absolute;
  top: 16px;
  right: 16px;
  width: 36px;
  height: 36px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-light);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  cursor: pointer;
  font-size: 0.95rem;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.trend-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.trend-modal-body {
  padding: 1.4rem 1.6rem 1.6rem;
  display: flex;
  flex-direction: column;
  gap: 1.2rem;
  flex: 1;
  min-height: 0;
}

.trend-modal-stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 0.8rem;
}

.trend-modal-stat {
  padding: 0.85rem;
  border-radius: var(--radius-sm);
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.trend-modal-stat span {
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.trend-modal-stat strong {
  font-size: 0.95rem;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.modal-enter-active,
.modal-leave-active {
  transition: opacity var(--transition-base);
}

.modal-enter-active .trend-modal,
.modal-leave-active .trend-modal {
  transition: transform var(--transition-base), opacity var(--transition-base);
}

.modal-enter-from .trend-modal,
.modal-leave-to .trend-modal {
  opacity: 0;
  transform: translateY(8px) scale(0.96);
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
</style>
