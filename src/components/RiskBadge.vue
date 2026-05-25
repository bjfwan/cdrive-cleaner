<script setup lang="ts">
import { computed } from 'vue';
import IconRiskSafe from './icons/status/IconRiskSafe.vue';
import IconRiskMedium from './icons/status/IconRiskMedium.vue';
import IconRiskDanger from './icons/status/IconRiskDanger.vue';
import IconError from './icons/status/IconError.vue';

type RiskLevel = 'safe' | 'caution' | 'risky' | 'blocked';

interface Props {
  level: RiskLevel;
  size?: 'sm' | 'md';
  showIcon?: boolean;
  showLabel?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  size: 'sm',
  showIcon: true,
  showLabel: true,
});

const label = computed(() => {
  if (props.level === 'safe') return '安全可清';
  if (props.level === 'caution') return '谨慎确认';
  if (props.level === 'risky') return '不建议清';
  return '禁止清除';
});

const iconComponent = computed(() => {
  if (props.level === 'safe') return IconRiskSafe;
  if (props.level === 'caution') return IconRiskMedium;
  if (props.level === 'risky') return IconRiskDanger;
  return IconError;
});

const iconPx = computed(() => (props.size === 'md' ? 14 : 12));
</script>

<template>
  <span class="risk-badge" :class="[`risk-badge--${level}`, `risk-badge--${size}`]" role="img" :aria-label="label">
    <component v-if="showIcon" :is="iconComponent" :size="iconPx" />
    <span v-if="showLabel" class="risk-badge__text">{{ label }}</span>
  </span>
</template>

<style scoped>
.risk-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.28rem;
  padding: 0.18rem 0.55rem;
  border-radius: var(--radius-pill);
  font-weight: 700;
  line-height: 1;
  border: 1px solid transparent;
  font-feature-settings: 'tnum';
  white-space: nowrap;
}

.risk-badge--sm {
  font-size: 0.74rem;
}

.risk-badge--md {
  font-size: 0.82rem;
  padding: 0.24rem 0.7rem;
}

.risk-badge__text {
  line-height: 1;
}

.risk-badge--safe {
  background: var(--risk-safe-soft);
  color: var(--risk-safe-text);
  border-color: var(--risk-safe-ring);
}

.risk-badge--caution {
  background: var(--risk-caution-soft);
  color: var(--risk-caution-text);
  border-color: var(--risk-caution-ring);
}

.risk-badge--risky {
  background: var(--risk-risky-soft);
  color: var(--risk-risky-text);
  border-color: var(--risk-risky-ring);
}

.risk-badge--blocked {
  background: rgba(31, 41, 55, 0.08);
  color: var(--color-text-secondary);
  border-color: var(--color-border-strong);
}

[data-theme="dark"] .risk-badge--blocked {
  background: rgba(255, 255, 255, 0.06);
  color: var(--color-text-secondary);
  border-color: var(--color-border-medium);
}
</style>
