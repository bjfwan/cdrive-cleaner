<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from 'vue';
import { IconError, IconWarning, IconInfo } from './icons';

interface Props {
  show: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
  highRisk?: boolean;
  confirmWord?: string;
  requireHoldMs?: number;
}

const props = withDefaults(defineProps<Props>(), {
  confirmText: '确定',
  cancelText: '取消',
  type: 'danger',
  highRisk: false,
  confirmWord: '删除',
  requireHoldMs: 0,
});

const emit = defineEmits<{
  'confirm': [];
  'cancel': [];
}>();

const wordInput = ref('');
const holdProgress = ref(0);
let holdRaf = 0;
let holdStart = 0;

const usesHold = computed(() => props.highRisk && props.requireHoldMs > 0);
const usesWord = computed(() => props.highRisk && !usesHold.value);

const wordMatches = computed(() => {
  if (!usesWord.value) return true;
  return wordInput.value.trim() === props.confirmWord.trim();
});

const canConfirm = computed(() => {
  if (!props.highRisk) return true;
  if (usesHold.value) return holdProgress.value >= 1;
  return wordMatches.value;
});

function cancelHold() {
  if (holdRaf) {
    cancelAnimationFrame(holdRaf);
    holdRaf = 0;
  }
  holdStart = 0;
  holdProgress.value = 0;
}

function tickHold(ts: number) {
  if (!holdStart) holdStart = ts;
  const elapsed = ts - holdStart;
  const ratio = Math.min(elapsed / props.requireHoldMs, 1);
  holdProgress.value = ratio;
  if (ratio < 1) {
    holdRaf = requestAnimationFrame(tickHold);
  } else {
    holdRaf = 0;
    emit('confirm');
  }
}

function startHold(event: PointerEvent | KeyboardEvent) {
  if (!usesHold.value) return;
  event.preventDefault();
  cancelHold();
  holdRaf = requestAnimationFrame(tickHold);
}

function stopHold() {
  if (!usesHold.value) return;
  if (holdProgress.value < 1) {
    cancelHold();
  }
}

function handleConfirm() {
  if (!canConfirm.value) return;
  if (usesHold.value) return;
  emit('confirm');
}

function handleCancel() {
  cancelHold();
  emit('cancel');
}

watch(
  () => props.show,
  (next) => {
    if (!next) {
      wordInput.value = '';
      cancelHold();
    }
  },
);

onUnmounted(() => cancelHold());
</script>

<template>
  <div v-if="show" class="confirm-overlay" @click.self="handleCancel">
    <div class="confirm-dialog" :class="{ 'confirm-dialog--high-risk': highRisk }" @click.stop>
      <div v-if="highRisk" class="confirm-risk-ribbon" aria-hidden="true"></div>
      <div class="confirm-icon" :class="`icon-${type}`">
        <IconError v-if="type === 'danger'" :size="48" />
        <IconWarning v-else-if="type === 'warning'" :size="48" />
        <IconInfo v-else :size="48" />
      </div>
      <h3>{{ title }}</h3>
      <p>{{ message }}</p>

      <div v-if="usesWord" class="confirm-gate">
        <label class="confirm-gate__label">
          <span class="confirm-gate__hint">{{ confirmWord }}</span>
          <input
            v-model="wordInput"
            type="text"
            class="confirm-gate__input"
            :placeholder="confirmWord"
            autocomplete="off"
            spellcheck="false"
          />
        </label>
      </div>

      <div class="confirm-actions" :class="{ 'confirm-actions--high-risk': highRisk }">
        <button
          v-if="cancelText"
          class="btn-base btn-secondary confirm-cancel"
          :class="{ 'confirm-cancel--dominant': highRisk }"
          @click="handleCancel"
        >{{ cancelText }}</button>
        <button
          v-if="!usesHold"
          class="btn-base"
          :class="[`btn-${type === 'danger' ? 'danger' : type === 'warning' ? 'caution' : 'primary'}`]"
          :disabled="!canConfirm"
          @click="handleConfirm"
        >{{ confirmText }}</button>
        <button
          v-else
          class="btn-base btn-danger confirm-hold"
          :style="{ '--hold-progress': holdProgress }"
          @pointerdown="startHold"
          @pointerup="stopHold"
          @pointerleave="stopHold"
          @pointercancel="stopHold"
          @keydown.space.prevent="startHold"
          @keyup.space.prevent="stopHold"
        >
          <span class="confirm-hold__fill" :style="{ transform: `scaleX(${holdProgress})` }"></span>
          <span class="confirm-hold__label">{{ confirmText }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.confirm-overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2200;
  animation: fadeIn var(--motion-fade-duration) var(--motion-fade-easing);
  font-family: var(--font-sans);
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.confirm-dialog {
  position: relative;
  width: 90%;
  max-width: 420px;
  padding: 2.5rem 2rem 2rem;
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: var(--radius-lg);
  box-shadow:
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  text-align: center;
  animation: slideUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

.confirm-dialog--high-risk {
  box-shadow:
    0 0 0 1px var(--risk-risky-ring),
    var(--shadow-lg),
    var(--shadow-xl);
}

.confirm-risk-ribbon {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 0.35rem;
  background: linear-gradient(90deg, var(--risk-risky-base), var(--risk-risky-strong));
  border-radius: var(--radius-lg) var(--radius-lg) 0 0;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(20px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.confirm-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 1.5rem;
  animation: iconPulse 2s ease-in-out infinite;
}

@keyframes iconPulse {
  0%, 100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(1.05);
    opacity: 0.9;
  }
}

.icon-danger {
  color: var(--risk-risky-base);
}

.icon-warning {
  color: var(--risk-caution-base);
}

.icon-info {
  color: var(--color-info);
}

.confirm-dialog h3 {
  font-family: var(--font-serif);
  font-size: 1.375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.75rem;
  letter-spacing: 0;
}

.confirm-dialog p {
  font-size: 0.9375rem;
  color: var(--color-text-secondary);
  line-height: 1.6;
  margin-bottom: 1.4rem;
}

.confirm-gate {
  margin: 0 0 1.4rem;
  display: flex;
  justify-content: center;
}

.confirm-gate__label {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 0.4rem;
}

.confirm-gate__hint {
  font-size: 0.78rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--risk-risky-text);
  align-self: flex-start;
  padding: 0.15rem 0.55rem;
  background: var(--risk-risky-soft);
  border: 1px solid var(--risk-risky-ring);
  border-radius: var(--radius-pill);
}

.confirm-gate__input {
  width: 100%;
  padding: 0.7rem 0.9rem;
  font-size: 0.95rem;
  font-family: inherit;
  color: var(--color-text-primary);
  background: var(--color-surface-strong);
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  transition: border-color var(--motion-fade-duration) var(--motion-fade-easing),
              box-shadow var(--motion-fade-duration) var(--motion-fade-easing);
}

.confirm-gate__input:focus-visible {
  outline: none;
  border-color: var(--risk-risky-base);
  box-shadow: 0 0 0 3px var(--risk-risky-ring);
}

.confirm-actions {
  display: flex;
  gap: 0.875rem;
  justify-content: center;
}

.confirm-actions--high-risk {
  flex-direction: row-reverse;
}

.confirm-cancel {
  min-width: 120px;
}

.confirm-cancel--dominant {
  flex: 1.4;
  font-size: 1rem;
  min-height: 2.85rem;
  background: var(--color-highlight-soft);
  border-color: var(--color-highlight);
  color: var(--color-text-primary);
  font-weight: 700;
}

.confirm-cancel--dominant:hover:not(:disabled) {
  background: var(--color-highlight-soft);
  border-color: var(--color-highlight);
  transform: translateY(-1px);
  box-shadow: var(--shadow-sm);
}

.confirm-hold {
  position: relative;
  overflow: hidden;
  min-width: 120px;
}

.confirm-hold__fill {
  position: absolute;
  inset: 0;
  background: var(--risk-risky-strong);
  transform-origin: left;
  transform: scaleX(0);
  transition: transform 40ms linear;
  z-index: 0;
}

.confirm-hold__label {
  position: relative;
  z-index: 1;
}

@media (max-width: 480px) {
  .confirm-dialog {
    width: 95%;
    padding: 2rem 1.5rem 1.5rem;
  }

  .confirm-actions {
    flex-direction: column;
  }

  .confirm-actions--high-risk {
    flex-direction: column-reverse;
  }

  .confirm-cancel,
  .confirm-hold {
    width: 100%;
  }
}
</style>
