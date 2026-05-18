<script setup lang="ts">
import { computed } from 'vue';
import type { DiskInfo } from '../types';
import { formatBytes } from '../utils/format';

interface Props {
  disk: DiskInfo;
  active: boolean;
}

const props = defineProps<Props>();

defineEmits<{
  select: [];
}>();

const usageState = computed(() => {
  if (props.disk.usage_percent >= 90) {
    return 'critical';
  }
  if (props.disk.usage_percent >= 80) {
    return 'warning';
  }
  return 'healthy';
});

const usageStyle = computed(() => {
  const usage = Math.min(Math.max(props.disk.usage_percent, 0), 100);
  const degrees = usage * 3.6;
  const ringColor =
    usage >= 90 ? 'rgba(220, 38, 38, 0.88)' : usage >= 80 ? 'rgba(217, 119, 6, 0.88)' : 'rgba(15, 118, 110, 0.88)';

  return {
    background: `
      radial-gradient(circle at center, rgba(255, 255, 255, 0.94) 0 58%, transparent 59%),
      conic-gradient(
        from -90deg,
        ${ringColor} 0deg,
        ${ringColor} ${degrees}deg,
        rgba(23, 23, 23, 0.08) ${degrees}deg,
        rgba(23, 23, 23, 0.04) 360deg
      )
    `,
  };
});

const usageHeadline = computed(() => {
  if (props.disk.usage_percent >= 90) {
    return '空间压力很高';
  }
  if (props.disk.usage_percent >= 80) {
    return '建议尽快处理';
  }
  return '容量状态健康';
});

const usageDescription = computed(() => {
  if (props.disk.usage_percent >= 90) {
    return '优先清理大目录或迁移数据，避免系统盘继续逼近满载。';
  }
  if (props.disk.usage_percent >= 80) {
    return '已经接近高压区，适合先扫描热点目录，再决定迁移策略。';
  }
  return '当前仍有充足余量，可以先观察目录结构再做迁移。';
});
</script>

<template>
  <div class="disk-card" :class="[usageState, { active }]" @click="$emit('select')">
    <div class="card-glow"></div>

    <div class="card-header">
      <div class="drive-meta">
        <div class="drive-symbol">{{ disk.drive_letter.replace(':', '') }}</div>
        <div>
          <div class="drive-line">
            <span class="drive-letter">{{ disk.drive_letter }}</span>
            <span class="fs-pill">{{ disk.file_system }}</span>
          </div>
          <div class="drive-label">{{ disk.label || 'Local Disk' }}</div>
        </div>
      </div>

      <div class="capacity">
        <span>总容量</span>
        <strong>{{ formatBytes(disk.total_space) }}</strong>
      </div>
    </div>

    <div class="usage-section">
      <div class="usage-ring" :style="usageStyle">
        <div class="usage-ring-inner">
          <strong>{{ disk.usage_percent.toFixed(0) }}%</strong>
          <span>已占用</span>
        </div>
      </div>

      <div class="usage-copy">
        <h3>{{ usageHeadline }}</h3>
        <p>{{ usageDescription }}</p>
      </div>
    </div>

    <div class="usage-meter">
      <div class="usage-meter-track">
        <div class="usage-meter-fill" :style="{ width: `${Math.min(disk.usage_percent, 100)}%` }"></div>
      </div>

      <div class="usage-meta">
        <span>已用 {{ formatBytes(disk.used_space) }}</span>
        <span>可用 {{ formatBytes(disk.free_space) }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.disk-card {
  position: relative;
  padding: 1.15rem;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.74), rgba(247, 241, 232, 0.9));
  box-shadow: var(--shadow-xs);
  cursor: pointer;
  overflow: hidden;
  transition: transform var(--transition-base), border-color var(--transition-base), background var(--transition-base);
}

.card-glow {
  position: absolute;
  inset: auto -15% -35% auto;
  width: 9rem;
  height: 9rem;
  background: radial-gradient(circle, rgba(15, 118, 110, 0.12), transparent 68%);
  pointer-events: none;
  transition: transform var(--transition-slow), opacity var(--transition-slow);
}

.disk-card:hover {
  transform: translateY(-2px);
  border-color: var(--color-border-medium);
  box-shadow: var(--shadow-sm);
}

.disk-card:hover .card-glow,
.disk-card.active .card-glow {
  transform: scale(1.08);
  opacity: 1;
}

.disk-card.active {
  border-color: rgba(15, 118, 110, 0.22);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.88), rgba(239, 248, 246, 0.88));
  box-shadow: 0 20px 40px rgba(15, 118, 110, 0.12);
}

.disk-card.warning {
  border-color: rgba(217, 119, 6, 0.14);
}

.disk-card.critical {
  border-color: rgba(220, 38, 38, 0.14);
}

.card-header {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
}

.drive-meta {
  display: flex;
  align-items: center;
  gap: 0.85rem;
  min-width: 0;
}

.drive-symbol {
  width: 2.85rem;
  height: 2.85rem;
  border-radius: 1rem;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(23, 23, 23, 0.06);
  color: var(--color-text-primary);
  font-size: 1.05rem;
  font-weight: 800;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.7);
  flex-shrink: 0;
}

.drive-line {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  flex-wrap: wrap;
}

.drive-letter {
  font-size: 1.2rem;
  font-weight: 800;
  letter-spacing: 0;
}

.drive-label {
  margin-top: 0.2rem;
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
}

.fs-pill {
  padding: 0.28rem 0.55rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.06);
  color: var(--color-text-secondary);
  font-size: 0.68rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.capacity {
  display: flex;
  flex-direction: column;
  align-items: end;
  gap: 0.15rem;
  text-align: right;
}

.capacity span {
  font-size: 0.68rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
  font-weight: 700;
}

.capacity strong {
  font-size: 0.9rem;
  font-weight: 800;
}

.usage-section {
  display: grid;
  grid-template-columns: 5.4rem minmax(0, 1fr);
  gap: 1rem;
  align-items: center;
  margin-bottom: 1rem;
}

.usage-ring {
  position: relative;
  width: 5.4rem;
  height: 5.4rem;
  border-radius: 50%;
  padding: 0.18rem;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6), var(--shadow-xs);
}

.usage-ring-inner {
  position: absolute;
  inset: 16%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
}

.usage-ring-inner strong {
  font-size: 1.02rem;
  font-weight: 800;
  line-height: 1;
}

.usage-ring-inner span {
  margin-top: 0.18rem;
  font-size: 0.62rem;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
  font-weight: 700;
}

.usage-copy h3 {
  font-size: 1rem;
  margin-bottom: 0.28rem;
}

.usage-copy p {
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--color-text-tertiary);
}

.usage-meter-track {
  width: 100%;
  height: 0.46rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.08);
  overflow: hidden;
}

.usage-meter-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, rgba(15, 118, 110, 0.92), rgba(93, 201, 194, 0.72));
}

.warning .usage-meter-fill {
  background: linear-gradient(90deg, rgba(217, 119, 6, 0.9), rgba(245, 158, 11, 0.7));
}

.critical .usage-meter-fill {
  background: linear-gradient(90deg, rgba(220, 38, 38, 0.92), rgba(248, 113, 113, 0.72));
}

.usage-meta {
  display: flex;
  justify-content: space-between;
  gap: 0.6rem;
  margin-top: 0.6rem;
  font-size: 0.78rem;
  color: var(--color-text-secondary);
}

@media (max-width: 560px) {
  .card-header {
    flex-direction: column;
  }

  .capacity {
    align-items: start;
    text-align: left;
  }
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .disk-card {
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .disk-card.active {
  background: linear-gradient(180deg, rgba(20, 184, 166, 0.12), rgba(20, 184, 166, 0.04));
  border-color: rgba(20, 184, 166, 0.32);
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.45);
}

[data-theme="dark"] .drive-symbol {
  background: rgba(255, 255, 255, 0.06);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
}

[data-theme="dark"] .fs-pill {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .usage-meter-track {
  background: rgba(255, 255, 255, 0.08);
}
</style>
