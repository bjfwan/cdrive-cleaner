<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue';

const emit = defineEmits<{ close: [] }>();

const step = ref(0);
const totalSteps = 3;
const spotlightStyle = ref<Record<string, string>>({});
const tooltipStyle = ref<Record<string, string>>({});
const tooltipPlacement = ref<'bottom' | 'top' | 'left' | 'right'>('bottom');

interface StepConfig {
  target: string;
  title: string;
  description: string;
  placement: 'bottom' | 'top' | 'left' | 'right';
}

const steps: StepConfig[] = [
  {
    target: '.sidebar, .topbar-cta--strong',
    title: '选择磁盘，点击扫描',
    description: '先看看 C 盘现状，3 秒出结果',
    placement: 'right',
  },
  {
    target: '.workspace-tabs',
    title: '三个标签，三种清理方式',
    description: '磁盘：找大目录搬走 · 游戏库：游戏搬盘 · 垃圾清理：安全删临时文件',
    placement: 'bottom',
  },
  {
    target: '.cart-fab',
    title: '搬运车 + 回滚',
    description: '选好要搬的，一键执行。所有操作可在历史里撤销。',
    placement: 'left',
  },
];

function finish() {
  localStorage.setItem('cdrive-cleaner-onboarding-completed', 'true');
  localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
  emit('close');
}

function nextStep() {
  if (step.value < totalSteps - 1) {
    step.value++;
    void positionSpotlight();
  } else {
    finish();
  }
}

function positionSpotlight() {
  const config = steps[step.value];
  // For step 0 with multiple targets, combine bounding rects
  const targets = config.target.split(',').map(s => s.trim());
  let rect: DOMRect | null = null;

  for (const selector of targets) {
    const el = document.querySelector(selector);
    if (!el) continue;
    const r = el.getBoundingClientRect();
    if (!rect) {
      rect = new DOMRect(r.x, r.y, r.width, r.height);
    } else {
      const minX = Math.min(rect.x, r.x);
      const minY = Math.min(rect.y, r.y);
      const maxX = Math.max(rect.x + rect.width, r.x + r.width);
      const maxY = Math.max(rect.y + rect.height, r.y + r.height);
      rect = new DOMRect(minX, minY, maxX - minX, maxY - minY);
    }
  }

  if (!rect) {
    // If target not found (e.g. cart-fab not visible), skip to next or finish
    if (step.value < totalSteps - 1) {
      step.value++;
      void positionSpotlight();
    } else {
      finish();
    }
    return;
  }

  const pad = 8;
  spotlightStyle.value = {
    top: `${rect.top - pad}px`,
    left: `${rect.left - pad}px`,
    width: `${rect.width + pad * 2}px`,
    height: `${rect.height + pad * 2}px`,
  };

  tooltipPlacement.value = config.placement;

  // Position tooltip relative to spotlight
  const gap = 12;
  let top = 0;
  let left = 0;
  const tooltipW = 280;
  const tooltipH = 140;

  switch (config.placement) {
    case 'bottom':
      top = rect.bottom + pad + gap;
      left = rect.left + rect.width / 2 - tooltipW / 2;
      break;
    case 'top':
      top = rect.top - pad - gap - tooltipH;
      left = rect.left + rect.width / 2 - tooltipW / 2;
      break;
    case 'right':
      top = rect.top + rect.height / 2 - tooltipH / 2;
      left = rect.right + pad + gap;
      break;
    case 'left':
      top = rect.top + rect.height / 2 - tooltipH / 2;
      left = rect.left - pad - gap - tooltipW;
      break;
  }

  // Clamp to viewport
  left = Math.max(12, Math.min(left, window.innerWidth - tooltipW - 12));
  top = Math.max(12, Math.min(top, window.innerHeight - tooltipH - 12));

  tooltipStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
  };
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    finish();
  } else if (e.key === 'Enter' || e.key === 'ArrowRight') {
    nextStep();
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown);
  nextTick(() => positionSpotlight());
  window.addEventListener('resize', positionSpotlight);
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
  window.removeEventListener('resize', positionSpotlight);
});
</script>

<template>
  <Teleport to="body">
    <div class="onboarding-mask" @click.self="finish">
      <!-- Spotlight cutout -->
      <div class="onboarding-spotlight" :style="spotlightStyle"></div>

      <!-- Tooltip -->
      <div class="onboarding-tooltip" :style="tooltipStyle" :data-placement="tooltipPlacement">
        <div class="onboarding-tooltip-header">
          <span class="onboarding-step-badge">{{ step + 1 }}/{{ totalSteps }}</span>
          <button class="onboarding-skip" @click="finish">跳过</button>
        </div>
        <h4 class="onboarding-tooltip-title">{{ steps[step].title }}</h4>
        <p class="onboarding-tooltip-desc">{{ steps[step].description }}</p>
        <button class="onboarding-next-btn" @click="nextStep">
          {{ step === totalSteps - 1 ? '开始使用' : '下一步' }}
        </button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.onboarding-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
  background: rgba(15, 23, 32, 0.5);
  animation: onb-fade-in var(--transition-base, 0.2s ease);
}

@keyframes onb-fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

.onboarding-spotlight {
  position: absolute;
  border-radius: var(--radius-md, 12px);
  box-shadow: 0 0 0 9999px rgba(15, 23, 32, 0.5);
  background: transparent;
  pointer-events: none;
  transition: top 0.35s cubic-bezier(0.16, 1, 0.3, 1),
              left 0.35s cubic-bezier(0.16, 1, 0.3, 1),
              width 0.35s cubic-bezier(0.16, 1, 0.3, 1),
              height 0.35s cubic-bezier(0.16, 1, 0.3, 1);
}

.onboarding-tooltip {
  position: absolute;
  width: 280px;
  padding: 1rem 1.2rem;
  border-radius: var(--radius-md, 12px);
  background: var(--color-surface-strong, #fff);
  border: 1px solid var(--color-border-light, rgba(0,0,0,0.08));
  box-shadow: 0 20px 40px rgba(15, 23, 32, 0.2);
  font-family: var(--font-sans, system-ui);
  pointer-events: auto;
  transition: top 0.35s cubic-bezier(0.16, 1, 0.3, 1),
              left 0.35s cubic-bezier(0.16, 1, 0.3, 1);
}

.onboarding-tooltip-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.onboarding-step-badge {
  font-size: 0.7rem;
  font-weight: 700;
  padding: 0.18rem 0.5rem;
  border-radius: 999px;
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.1));
  color: var(--color-highlight, #0f766e);
  letter-spacing: 0.04em;
}

.onboarding-skip {
  border: none;
  background: none;
  color: var(--color-text-tertiary, #778391);
  font-size: 0.76rem;
  font-weight: 600;
  cursor: pointer;
  padding: 0.2rem 0.4rem;
  border-radius: var(--radius-xs, 6px);
  transition: color var(--transition-fast, 0.1s);
}

.onboarding-skip:hover {
  color: var(--color-text-primary, #121923);
}

.onboarding-tooltip-title {
  font-size: 1rem;
  font-weight: 700;
  color: var(--color-text-primary, #121923);
  margin-bottom: 0.3rem;
  line-height: 1.2;
}

.onboarding-tooltip-desc {
  font-size: 0.82rem;
  color: var(--color-text-secondary, #43505c);
  line-height: 1.5;
  margin-bottom: 0.75rem;
}

.onboarding-next-btn {
  width: 100%;
  padding: 0.6rem 0;
  border: none;
  border-radius: var(--radius-sm, 8px);
  background: var(--color-highlight, #0f766e);
  color: var(--color-text-inverse, #f8fafc);
  font-size: 0.84rem;
  font-weight: 700;
  cursor: pointer;
  transition: transform var(--transition-fast, 0.1s), box-shadow var(--transition-fast, 0.1s);
  box-shadow: 0 8px 18px rgba(15, 118, 110, 0.2);
}

.onboarding-next-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 12px 24px rgba(15, 118, 110, 0.28);
}

.onboarding-next-btn:active {
  transform: scale(0.97);
}
</style>
