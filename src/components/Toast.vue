<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue';
import { IconSuccess, IconInfo, IconWarning, IconError } from './icons';

interface Props {
  message: string;
  subMessage?: string;
  type?: 'success' | 'info' | 'warning' | 'error';
  duration?: number;
  show: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  type: 'success',
  duration: 3000,
});

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const visible = ref(false);
let timer: number | null = null;

function clearTimer() {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}

function startTimer() {
  clearTimer();
  timer = window.setTimeout(() => {
    close();
  }, props.duration);
}

function showToast() {
  visible.value = true;
  startTimer();
}

function close() {
  clearTimer();
  visible.value = false;
  window.setTimeout(() => {
    emit('close');
  }, 260);
}

watch(
  () => [props.show, props.message, props.subMessage, props.type, props.duration] as const,
  ([show]) => {
    if (show) {
      showToast();
    } else {
      clearTimer();
      visible.value = false;
    }
  },
);

onMounted(() => {
  if (props.show) {
    showToast();
  }
});

onUnmounted(() => {
  clearTimer();
});
</script>

<template>
  <transition name="toast">
    <div v-if="visible" class="toast-container" @click="close">
      <div class="toast" :class="`toast-${type}`">
        <div class="toast-accent"></div>

        <div class="toast-icon">
          <IconSuccess v-if="type === 'success'" :size="22" />
          <IconInfo v-else-if="type === 'info'" :size="22" />
          <IconWarning v-else-if="type === 'warning'" :size="22" />
          <IconError v-else :size="22" />
        </div>

        <div class="toast-content">
          <div class="toast-label">系统提示</div>
          <div class="toast-message">{{ message }}</div>
          <div v-if="subMessage" class="toast-sub-message">{{ subMessage }}</div>
        </div>

        <div class="toast-progress">
          <div class="toast-progress-bar" :style="{ animationDuration: `${duration}ms` }"></div>
        </div>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.toast-container {
  position: fixed;
  top: 1.4rem;
  right: 1.4rem;
  z-index: 2500;
  cursor: pointer;
}

.toast {
  position: relative;
  min-width: 22rem;
  max-width: 28rem;
  display: flex;
  align-items: flex-start;
  gap: 0.95rem;
  padding: 1rem 1.15rem 1rem 1rem;
  border-radius: var(--radius-md);
  background: rgba(255, 255, 255, 0.94);
  border: 1px solid rgba(255, 255, 255, 0.58);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

.toast-accent {
  position: absolute;
  inset: 0 auto 0 0;
  width: 0.26rem;
  background: currentColor;
  opacity: 0.85;
}

.toast-icon {
  width: 2.65rem;
  height: 2.65rem;
  border-radius: 0.95rem;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(23, 23, 23, 0.05);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.66);
}

.toast-content {
  min-width: 0;
  flex: 1;
}

.toast-label {
  margin-bottom: 0.18rem;
  font-size: 0.68rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.toast-message {
  font-size: 0.95rem;
  font-weight: 800;
  color: var(--color-text-primary);
  line-height: 1.35;
}

.toast-sub-message {
  margin-top: 0.22rem;
  font-size: 0.8rem;
  line-height: 1.5;
  color: var(--color-text-secondary);
}

.toast-success {
  color: var(--color-success);
}

.toast-success .toast-icon {
  background: var(--risk-safe-soft);
}

.toast-info {
  color: var(--color-info);
}

.toast-info .toast-icon {
  background: var(--color-accent-wash);
}

.toast-warning {
  color: var(--color-warning);
}

.toast-warning .toast-icon {
  background: var(--risk-caution-soft);
}

.toast-error {
  color: var(--color-error);
}

.toast-error .toast-icon {
  background: var(--risk-risky-soft);
}

.toast-progress {
  position: absolute;
  inset: auto 0 0 0;
  height: 0.18rem;
  background: rgba(23, 23, 23, 0.05);
}

.toast-progress-bar {
  width: 100%;
  height: 100%;
  transform-origin: left;
  background: linear-gradient(90deg, currentColor 0%, rgba(255, 255, 255, 0.56) 100%);
  animation: progress linear forwards;
}

@keyframes progress {
  from {
    transform: scaleX(1);
  }

  to {
    transform: scaleX(0);
  }
}

.toast-enter-active {
  animation: toastIn 0.34s cubic-bezier(0.16, 1, 0.3, 1);
}

.toast-leave-active {
  animation: toastOut 0.24s ease-in forwards;
}

@keyframes toastIn {
  from {
    opacity: 0;
    transform: translateY(-0.6rem) translateX(1rem) scale(0.96);
  }

  to {
    opacity: 1;
    transform: translateY(0) translateX(0) scale(1);
  }
}

@keyframes toastOut {
  from {
    opacity: 1;
    transform: translateY(0) translateX(0) scale(1);
  }

  to {
    opacity: 0;
    transform: translateY(-0.35rem) translateX(1rem) scale(0.98);
  }
}

@media (max-width: 640px) {
  .toast-container {
    top: 1rem;
    right: 1rem;
    left: 1rem;
  }

  .toast {
    min-width: auto;
    max-width: none;
  }
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .toast {
  background: var(--color-surface-strong);
  border-color: var(--color-border-medium);
  box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
}

[data-theme="dark"] .toast-icon {
  background: rgba(255, 255, 255, 0.06);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
}

[data-theme="dark"] .toast-progress {
  background: rgba(255, 255, 255, 0.08);
}
</style>
