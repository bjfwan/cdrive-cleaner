<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue';

const emit = defineEmits<{ close: [] }>();

interface CoachStep {
  target: string;
  placement: 'top' | 'bottom' | 'left' | 'right';
  title: string;
  description: string;
  offset?: number;
}

const steps: CoachStep[] = [
  {
    target: '.topbar-cta--strong',
    placement: 'bottom',
    title: '选择磁盘，点击扫描',
    description: '先看看 C 盘现状，几秒出结果',
  },
  {
    target: '.workspace-tabs',
    placement: 'bottom',
    title: '三种清理方式',
    description: '磁盘：找大目录搬走 · 游戏库：游戏搬盘 · 垃圾清理：删临时文件',
  },
  {
    target: '.topbar-tool:last-child',
    placement: 'bottom',
    title: '设置 + 历史',
    description: '所有操作可在历史里撤销，设置里切换主题和清理偏好',
  },
];

const currentStep = ref(0);
const spotlightStyle = ref<Record<string, string>>({});
const tooltipStyle = ref<Record<string, string>>({});

const isLast = computed(() => currentStep.value === steps.length - 1);
const btnLabel = computed(() => (isLast.value ? '开始使用' : '下一步'));

function close() {
  emit('close');
}

function nextStep() {
  if (isLast.value) {
    close();
  } else {
    currentStep.value++;
    positionAll();
  }
}

function positionAll() {
  const config = steps[currentStep.value];
  if (!config) {
    close();
    return;
  }

  const el = document.querySelector(config.target);
  if (!el) {
    if (currentStep.value < steps.length - 1) {
      currentStep.value++;
      positionAll();
    } else {
      close();
    }
    return;
  }

  const rect = el.getBoundingClientRect();
  const pad = 8;

  spotlightStyle.value = {
    top: `${rect.top - pad}px`,
    left: `${rect.left - pad}px`,
    width: `${rect.width + pad * 2}px`,
    height: `${rect.height + pad * 2}px`,
  };

  const gap = config.offset ?? 12;
  const tooltipW = 280;
  const tooltipH = 130;
  let top = 0;
  let left = 0;

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

  left = Math.max(12, Math.min(left, window.innerWidth - tooltipW - 12));
  top = Math.max(12, Math.min(top, window.innerHeight - tooltipH - 12));

  tooltipStyle.value = {
    top: `${top}px`,
    left: `${left}px`,
  };
}

function onResize() {
  positionAll();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    close();
  } else if (e.key === 'Enter' || e.key === 'ArrowRight') {
    nextStep();
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown);
  window.addEventListener('resize', onResize);
  nextTick(() => positionAll());
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
  window.removeEventListener('resize', onResize);
});
</script>

<template>
  <Teleport to="body">
    <div class="coach-overlay" @click.self="close">
      <div class="coach-spotlight" :style="spotlightStyle"></div>

      <div class="coach-tooltip" :style="tooltipStyle">
        <div class="coach-tooltip-head">
          <span class="coach-badge">{{ currentStep + 1 }}/{{ steps.length }}</span>
          <button class="coach-skip" @click="close">跳过引导</button>
        </div>
        <h4 class="coach-title">{{ steps[currentStep].title }}</h4>
        <p class="coach-desc">{{ steps[currentStep].description }}</p>
        <div class="coach-foot">
          <div class="coach-dots">
            <span
              v-for="i in steps.length"
              :key="i"
              class="coach-dot"
              :class="{ active: currentStep === i - 1 }"
            ></span>
          </div>
          <button class="coach-next" @click="nextStep">{{ btnLabel }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.coach-overlay {
  position: fixed;
  inset: 0;
  z-index: 9000;
  pointer-events: auto;
}

.coach-spotlight {
  position: fixed;
  border-radius: 12px;
  box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.55);
  pointer-events: none;
  transition:
    top 0.4s cubic-bezier(0.16, 1, 0.3, 1),
    left 0.4s cubic-bezier(0.16, 1, 0.3, 1),
    width 0.4s cubic-bezier(0.16, 1, 0.3, 1),
    height 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

.coach-tooltip {
  position: fixed;
  width: 280px;
  padding: 1rem 1.2rem;
  border-radius: var(--radius-md, 12px);
  background: var(--color-surface-strong, #fff);
  border: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  box-shadow: var(--shadow-lg, 0 20px 40px rgba(15, 23, 32, 0.2));
  font-family: var(--font-sans, system-ui);
  pointer-events: auto;
  transition:
    top 0.4s cubic-bezier(0.16, 1, 0.3, 1),
    left 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  animation: coach-in 0.25s ease;
}

@keyframes coach-in {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}

.coach-tooltip-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.coach-badge {
  font-size: 0.7rem;
  font-weight: 700;
  padding: 0.18rem 0.5rem;
  border-radius: 999px;
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.1));
  color: var(--color-highlight, #0f766e);
  letter-spacing: 0.04em;
}

.coach-skip {
  border: none;
  background: none;
  color: var(--color-text-tertiary, #778391);
  font-size: 0.74rem;
  font-weight: 600;
  cursor: pointer;
  padding: 0.2rem 0.4rem;
  border-radius: 6px;
  transition: color 0.15s;
}

.coach-skip:hover {
  color: var(--color-text-primary, #121923);
}

.coach-title {
  font-size: 1rem;
  font-weight: 700;
  color: var(--color-text-primary, #121923);
  margin-bottom: 0.3rem;
  line-height: 1.2;
}

.coach-desc {
  font-size: 0.82rem;
  color: var(--color-text-secondary, #43505c);
  line-height: 1.5;
  margin-bottom: 0.85rem;
}

.coach-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.coach-dots {
  display: flex;
  gap: 5px;
}

.coach-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-border-medium, #d1d5db);
  transition: background 0.2s, transform 0.2s;
}

.coach-dot.active {
  background: var(--color-highlight, #0f766e);
  transform: scale(1.3);
}

.coach-next {
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 8px;
  background: var(--color-highlight, #0f766e);
  color: var(--color-text-inverse, #fff);
  font-size: 0.82rem;
  font-weight: 700;
  cursor: pointer;
  transition: transform 0.15s, box-shadow 0.15s;
  box-shadow: 0 6px 14px rgba(15, 118, 110, 0.2);
}

.coach-next:hover {
  transform: translateY(-1px);
  box-shadow: 0 10px 20px rgba(15, 118, 110, 0.28);
}

.coach-next:active {
  transform: scale(0.96);
}
</style>
