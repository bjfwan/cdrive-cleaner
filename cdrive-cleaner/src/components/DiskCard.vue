<script setup lang="ts">
interface DiskInfo {
  drive_letter: string;
  label: string;
  file_system: string;
  total_space: number;
  free_space: number;
  used_space: number;
  usage_percent: number;
}

interface Props {
  disk: DiskInfo;
  active: boolean;
}

defineProps<Props>();
defineEmits<{
  select: [];
}>();

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}
</script>

<template>
  <div class="disk-card" :class="{ active }" @click="$emit('select')">
    <div class="card-header">
      <div class="drive-info">
        <span class="drive-letter">{{ disk.drive_letter }}</span>
        <span class="drive-label">{{ disk.label }}</span>
      </div>
      <div class="capacity">{{ formatBytes(disk.total_space) }}</div>
    </div>

    <div class="usage-ring">
      <svg viewBox="0 0 36 36" class="circular-chart">
        <path class="circle-bg"
          d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
        />
        <path class="circle"
          :class="{
            warning: disk.usage_percent > 80,
            critical: disk.usage_percent > 90
          }"
          :stroke-dasharray="`${disk.usage_percent}, 100`"
          d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
        />
        <text x="18" y="21" class="percentage">{{ disk.usage_percent.toFixed(0) }}%</text>
      </svg>
    </div>

    <div class="usage-details">
      <div class="detail-row">
        <span class="label">已用</span>
        <span class="value">{{ formatBytes(disk.used_space) }}</span>
      </div>
      <div class="detail-row">
        <span class="label">可用</span>
        <span class="value">{{ formatBytes(disk.free_space) }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.disk-card {
  background: #fafaf9;
  border: 1.5px solid #e7e5e4;
  border-radius: 14px;
  padding: 1.5rem;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.disk-card:hover {
  border-color: #007aff;
  box-shadow: 0 6px 16px rgba(0, 122, 255, 0.12);
  transform: translateY(-3px);
}

.disk-card.active {
  border-color: #007aff;
  background: linear-gradient(135deg, #f0f7ff 0%, #ffffff 100%);
  box-shadow: 0 8px 24px rgba(0, 122, 255, 0.2);
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 1.5rem;
}

.drive-info {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.drive-letter {
  font-size: 1.5rem;
  font-weight: 600;
  color: #2c2c2c;
  letter-spacing: -0.02em;
}

.drive-label {
  font-size: 0.8125rem;
  color: #78716c;
}

.capacity {
  font-size: 0.8125rem;
  color: #a8a29e;
  font-weight: 500;
}

.usage-ring {
  width: min(100px, 100%);
  height: min(100px, 100%);
  max-width: 120px;
  margin: 0 auto 1.5rem;
  aspect-ratio: 1;
}

.circular-chart {
  display: block;
  width: 100%;
  height: 100%;
}

.circle-bg {
  fill: none;
  stroke: #f5f5f4;
  stroke-width: 3;
}

.circle {
  fill: none;
  stroke: #007aff;
  stroke-width: 3;
  stroke-linecap: round;
  transform: rotate(-90deg);
  transform-origin: 50% 50%;
  transition: stroke-dasharray 0.6s cubic-bezier(0.4, 0, 0.2, 1);
}

.circle.warning {
  stroke: #ff9500;
}

.circle.critical {
  stroke: #ff3b30;
}

.percentage {
  fill: #2c2c2c;
  font-size: 0.5rem;
  font-weight: 600;
  text-anchor: middle;
}

.usage-details {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.detail-row {
  display: flex;
  justify-content: space-between;
  font-size: 0.875rem;
}

.label {
  color: #78716c;
}

.value {
  color: #2c2c2c;
  font-weight: 500;
}
</style>


@media (max-width: 600px) {
  .disk-card {
    padding: 1.25rem;
  }

  .drive-letter {
    font-size: 1.25rem;
  }

  .usage-ring {
    width: 80px;
    height: 80px;
    margin-bottom: 1rem;
  }

  .detail-row {
    font-size: 0.8125rem;
  }
}
