<script setup lang="ts">
import { ref, computed, onBeforeUnmount } from 'vue';
import RiskBadge from './RiskBadge.vue';
import type { JunkRiskLevel } from '../types/junk';

interface Props {
  whySafe: string | null;
  ruleName: string;
  risk: JunkRiskLevel;
  variant?: 'inline' | 'subtle';
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'subtle',
});

const open = ref(false);
const triggerRef = ref<HTMLButtonElement | null>(null);
const bubbleRef = ref<HTMLElement | null>(null);
const bubbleStyle = ref<Record<string, string>>({});
const bubblePlacement = ref<'top' | 'bottom'>('top');

const triggerText = computed(() => {
  if (!props.whySafe) return '未补充';
  if (props.risk === 'risky') return '为什么不建议';
  if (props.risk === 'caution') return '为什么建议清';
  return '为什么可清';
});

const triggerLabel = computed(() => {
  if (!props.whySafe) return '没有补充说明';
  if (props.risk === 'risky') return '查看为什么不建议清理';
  if (props.risk === 'caution') return '查看为什么建议谨慎清理';
  return '查看为什么这是可清理项';
});

const bodyText = computed(() => {
  const text = props.whySafe?.trim();
  if (text) return text;
  if (props.risk === 'risky') return '此规则风险较高，默认不建议勾选。仍可使用「这条不对？」反馈补充说明。';
  return '此规则尚未补充安全说明。建议保持默认勾选状态，仍可使用「这条不对？」反馈。';
});

const badgeLevel = computed(() => {
  if (props.risk === 'safe') return 'safe';
  if (props.risk === 'caution') return 'caution';
  return 'risky';
});

function toggle() {
  if (open.value) {
    close();
    return;
  }
  updateBubblePosition();
  open.value = true;
  attach();
}

function close() {
  open.value = false;
  detach();
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') close();
}

function onDocClick(event: MouseEvent) {
  const trigger = triggerRef.value;
  const bubble = bubbleRef.value;
  const target = event.target as Node;
  if (!trigger) return;
  if (trigger === target || trigger.contains(target)) return;
  if (bubble && (bubble === target || bubble.contains(target))) return;
  close();
}

function updateBubblePosition() {
  const trigger = triggerRef.value;
  if (!trigger) return;

  const rect = trigger.getBoundingClientRect();
  const width = Math.min(320, window.innerWidth - 16);
  let left = rect.left + rect.width / 2 - width / 2;
  left = Math.max(8, Math.min(left, window.innerWidth - width - 8));

  const showAbove = rect.top > 220;
  bubblePlacement.value = showAbove ? 'top' : 'bottom';
  bubbleStyle.value = {
    width: `${width}px`,
    left: `${left}px`,
    top: `${showAbove ? rect.top - 8 : rect.bottom + 8}px`,
    transform: showAbove ? 'translateY(-100%)' : 'none',
  };
}

function attach() {
  document.addEventListener('keydown', onKey);
  document.addEventListener('mousedown', onDocClick);
  window.addEventListener('resize', updateBubblePosition);
  window.addEventListener('scroll', updateBubblePosition, true);
}

function detach() {
  document.removeEventListener('keydown', onKey);
  document.removeEventListener('mousedown', onDocClick);
  window.removeEventListener('resize', updateBubblePosition);
  window.removeEventListener('scroll', updateBubblePosition, true);
}

onBeforeUnmount(detach);
</script>

<template>
  <span class="why" :class="{ 'why--inline': variant === 'inline' }">
    <button
      ref="triggerRef"
      type="button"
      class="why__trigger"
      :class="[`why__trigger--${risk}`, { 'why__trigger--missing': !whySafe }]"
      :aria-expanded="open"
      :aria-label="triggerLabel"
      @click.stop="toggle"
    >
      <span class="why__trigger-icon" aria-hidden="true">?</span>
      <span class="why__trigger-label">{{ triggerText }}</span>
    </button>

    <Teleport to="body">
    <div
      v-if="open"
      ref="bubbleRef"
      class="why__bubble"
      :class="`why__bubble--${bubblePlacement}`"
      :style="bubbleStyle"
      role="dialog"
      aria-label="清理理由说明"
    >
      <header class="why__bubble-head">
        <strong class="why__bubble-name">{{ ruleName }}</strong>
        <RiskBadge :level="badgeLevel" />
      </header>
      <p class="why__bubble-text">{{ bodyText }}</p>
      <footer class="why__bubble-foot">
        <small>来源：内置规则库 · 可在反馈中纠正</small>
      </footer>
    </div>
    </Teleport>
  </span>
</template>

<style scoped>
.why {
  position: relative;
  display: inline-flex;
}

.why__trigger {
  display: inline-flex;
  align-items: center;
  gap: 0.28rem;
  padding: 0.16rem 0.5rem;
  border-radius: var(--radius-pill);
  border: 1px solid var(--color-border-medium);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 0.72rem;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
}

.why__trigger:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  border-color: var(--color-border-strong);
}

.why__trigger--missing {
  border-style: dashed;
  color: var(--color-text-tertiary);
}

.why__trigger--risky {
  color: var(--risk-risky-text);
  border-color: var(--risk-risky-ring);
}

.why__trigger-icon {
  display: inline-flex;
  width: 14px;
  height: 14px;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
  font-size: 0.66rem;
  font-weight: 800;
}

.why__trigger--risky .why__trigger-icon {
  background: var(--risk-risky-soft);
  color: var(--risk-risky-text);
}

.why__bubble {
  position: fixed;
  z-index: 9600;
  padding: 0.75rem 0.9rem;
  border-radius: var(--radius-md);
  background: var(--color-bg-primary);
  border: 1px solid var(--color-border-medium);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  max-height: min(320px, calc(100vh - 16px));
  overflow: auto;
  pointer-events: auto;
  animation: why-pop-in 0.14s ease;
}

@keyframes why-pop-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

.why__bubble-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
}

.why__bubble-name {
  font-size: 0.84rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.why__bubble-text {
  font-size: 0.8rem;
  color: var(--color-text-secondary);
  line-height: 1.5;
  word-break: break-word;
}

.why__bubble-foot small {
  color: var(--color-text-tertiary);
  font-size: 0.7rem;
}
</style>
