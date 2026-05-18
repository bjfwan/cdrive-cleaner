<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { formatBytes } from '../utils/format';
import { IconClose } from './icons';

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

const SPARK_W = 240;
const SPARK_H = 64;
const DETAIL_W = 540 - 64;
const DETAIL_H = 360 - 160;

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

function parseSqlTime(value: string): number {
  const normalized = value.includes('T') ? value : value.replace(' ', 'T') + 'Z';
  const parsed = Date.parse(normalized);
  return Number.isNaN(parsed) ? 0 : parsed;
}

/**
 * Walk the snapshot list once and produce everything the template needs.
 * This replaces the previous design where each of `sparklinePath`,
 * `sparklineFill`, `detailPath`, `detailFill`, `compare7`, `compare30`
 * triggered an independent O(n) traversal (and `Math.min(...values)` /
 * `Math.max(...values)` allocated and spread the entire array on every
 * recompute).
 */
const trendModel = computed(() => {
  const points = snapshots.value;
  const len = points.length;

  const empty = {
    mini: '',
    miniFill: '',
    large: '',
    largeFill: '',
    direction: 'flat' as 'up' | 'down' | 'flat',
    latest: null as DiskSnapshot | null,
    compare7: null as DiskSnapshot | null,
    compare30: null as DiskSnapshot | null,
  };

  if (len === 0) {
    return empty;
  }

  // Single sweep: min/max for the y-axis + nearest snapshot to (now - 7d) and
  // (now - 30d).
  const target7 = Date.now() - 7 * 24 * 60 * 60 * 1000;
  const target30 = Date.now() - 30 * 24 * 60 * 60 * 1000;
  let min = points[0].used_size;
  let max = min;
  let best7: DiskSnapshot | null = null;
  let best30: DiskSnapshot | null = null;
  let best7Diff = Number.POSITIVE_INFINITY;
  let best30Diff = Number.POSITIVE_INFINITY;

  for (let i = 0; i < len; i += 1) {
    const snap = points[i];
    const v = snap.used_size;
    if (v < min) min = v;
    if (v > max) max = v;

    const ts = parseSqlTime(snap.captured_at);
    const d7 = Math.abs(ts - target7);
    if (d7 < best7Diff) {
      best7Diff = d7;
      best7 = snap;
    }
    const d30 = Math.abs(ts - target30);
    if (d30 < best30Diff) {
      best30Diff = d30;
      best30 = snap;
    }
  }

  const latest = points[len - 1];
  const compare30 = best30;
  let direction: 'up' | 'down' | 'flat' = 'flat';
  if (compare30) {
    const delta = latest.used_size - compare30.used_size;
    if (delta > 0) direction = 'up';
    else if (delta < 0) direction = 'down';
  }

  const span = Math.max(max - min, 1);
  const padding = 4;

  // Build both viewports' line + fill paths in one pass.
  const buildPaths = (width: number, height: number) => {
    if (len < 2) {
      const y = height / 2;
      return { line: `M 0 ${y} L ${width} ${y}`, fill: '' };
    }
    const innerH = height - padding * 2;
    const stepX = width / (len - 1);
    let line = '';
    for (let i = 0; i < len; i += 1) {
      const x = i * stepX;
      const y = padding + innerH - ((points[i].used_size - min) / span) * innerH;
      line += `${i === 0 ? 'M' : ' L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    }
    const fill = `${line} L ${width.toFixed(2)} ${height} L 0 ${height} Z`;
    return { line, fill };
  };

  const mini = buildPaths(SPARK_W, SPARK_H);
  const large = buildPaths(DETAIL_W, DETAIL_H);

  return {
    mini: mini.line,
    miniFill: mini.fill,
    large: large.line,
    largeFill: large.fill,
    direction,
    latest,
    compare7: best7,
    compare30,
  };
});

const trendCopy = computed(() => {
  const { latest, compare30 } = trendModel.value;
  if (!latest || !compare30) return '记录中…';
  const delta = latest.used_size - compare30.used_size;
  if (delta === 0) return '过去 30 天没有变化';
  const verb = delta > 0 ? '增加了' : '减少了';
  return `过去 30 天${verb} ${formatBytes(Math.abs(delta))}`;
});

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

    <template v-else>
      <div
        class="trend-row"
        @click="openDetail"
        role="button"
        tabindex="0"
        @keydown.enter="openDetail"
      >
        <div class="trend-copy">
          <span class="trend-kicker">空间趋势</span>
          <strong>{{ trendModel.latest ? formatBytes(trendModel.latest.used_size) : '--' }}</strong>
          <small>{{ trendCopy }}</small>
        </div>

        <svg
          class="trend-chart"
          :viewBox="`0 0 ${SPARK_W} ${SPARK_H}`"
          preserveAspectRatio="none"
          :data-direction="trendModel.direction"
        >
          <defs>
            <linearGradient id="trend-fill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="currentColor" stop-opacity="0.32" />
              <stop offset="100%" stop-color="currentColor" stop-opacity="0" />
            </linearGradient>
          </defs>
          <path :d="trendModel.miniFill" fill="url(#trend-fill)" />
          <path
            :d="trendModel.mini"
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
            <strong>{{ trendModel.compare7 ? formatBytes(trendModel.compare7.used_size) : '--' }}</strong>
          </div>
          <div class="trend-stat">
            <span>30 天前</span>
            <strong>{{ trendModel.compare30 ? formatBytes(trendModel.compare30.used_size) : '--' }}</strong>
          </div>
        </div>
      </div>
    </template>

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
              <button class="trend-close" @click="closeDetail" aria-label="关闭"><IconClose :size="16" /></button>
            </header>

            <div class="trend-modal-body">
              <svg
                class="trend-chart-large"
                :viewBox="`0 0 ${DETAIL_W} ${DETAIL_H}`"
                preserveAspectRatio="none"
                :data-direction="trendModel.direction"
              >
                <defs>
                  <linearGradient id="trend-fill-large" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="currentColor" stop-opacity="0.32" />
                    <stop offset="100%" stop-color="currentColor" stop-opacity="0" />
                  </linearGradient>
                </defs>
                <path :d="trendModel.largeFill" fill="url(#trend-fill-large)" />
                <path
                  :d="trendModel.large"
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
                  <strong>{{ trendModel.latest ? formatBytes(trendModel.latest.used_size) : '--' }}</strong>
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
