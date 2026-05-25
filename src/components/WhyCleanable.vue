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

const triggerLabel = computed(() => (props.whySafe ? '为什么这是可清的？' : '没有补充说明'));

const bodyText = computed(() => props.whySafe?.trim() || '此规则尚未补充安全说明。建议保持默认勾选状态，仍可使用「这条不对？」反馈。');

const badgeLevel = computed(() => {
  if (props.risk === 'safe') return 'safe';
  if (props.risk === 'caution') return 'caution';
  return 'risky';
});

function toggle() {
  open.value = !open.value;
}

function close() {
  open.value = false;
}

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') close();
}

function onDocClick(event: MouseEvent) {
  const trigger = triggerRef.value;
  if (!trigger) return;
  if (trigger === event.target) return;
  if (trigger.contains(event.target as Node)) return;
  close();
}

function attach() {
  document.addEventListener('keydown', onKey);
  document.addEventListener('mousedown', onDocClick);
}

function detach() {
  document.removeEventListener('keydown', onKey);
  document.removeEventListener('mousedown', onDocClick);
}

onBeforeUnmount(detach);
</script>

<template>
  <span class="why" :class="{ 'why--inline': variant === 'inline' }">
    <button
      ref="triggerRef"
      type="button"
      class="why__trigger"
      :class="{ 'why__trigger--missing': !whySafe }"
      :aria-expanded="open"
      :aria-label="triggerLabel"
      @click.stop="toggle"
      @focus="attach"
      @blur="detach"
    >
      <span class="why__trigger-icon" aria-hidden="true">?</span>
      <span class="why__trigger-label">{{ whySafe ? '为什么可清' : '未补充' }}</span>
    </button>

    <div v-if="open" class="why__bubble" role="dialog" aria-label="清理理由说明">
      <header class="why__bubble-head">
        <strong class="why__bubble-name">{{ ruleName }}</strong>
        <RiskBadge :level="badgeLevel" />
      </header>
      <p class="why__bubble-text">{{ bodyText }}</p>
      <footer class="why__bubble-foot">
        <small>来源：内置规则库 · 可在反馈中纠正</small>
      </footer>
    </div>
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

.why__bubble {
  position: absolute;
  top: calc(100% + 0.4rem);
  right: 0;
  z-index: 60;
  width: 280px;
  padding: 0.75rem 0.9rem;
  border-radius: var(--radius-md);
  background: var(--color-bg-primary);
  border: 1px solid var(--color-border-medium);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.why--inline .why__bubble {
  right: auto;
  left: 0;
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
}

.why__bubble-foot small {
  color: var(--color-text-tertiary);
  font-size: 0.7rem;
}
</style>
