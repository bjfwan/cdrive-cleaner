<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
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

watch(() => props.show, (newVal) => {
  if (newVal) {
    visible.value = true;
    startTimer();
  } else {
    visible.value = false;
  }
});

function startTimer() {
  if (timer) {
    clearTimeout(timer);
  }
  timer = window.setTimeout(() => {
    close();
  }, props.duration);
}

function close() {
  visible.value = false;
  setTimeout(() => {
    emit('close');
  }, 300); // 等待动画完成
}

onMounted(() => {
  if (props.show) {
    visible.value = true;
    startTimer();
  }
});
</script>

<template>
  <transition name="toast">
    <div v-if="visible" class="toast-container" @click="close">
      <div class="toast" :class="`toast-${type}`">
        <div class="toast-icon">
          <IconSuccess v-if="type === 'success'" :size="24" />
          <IconInfo v-else-if="type === 'info'" :size="24" />
          <IconWarning v-else-if="type === 'warning'" :size="24" />
          <IconError v-else :size="24" />
        </div>
        
        <div class="toast-content">
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
  top: 2rem;
  right: 2rem;
  z-index: 9999;
  cursor: pointer;
}

.toast {
  position: relative;
  min-width: 320px;
  max-width: 420px;
  padding: 1.25rem 1.5rem;
  background: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border-radius: 16px;
  box-shadow: 
    0 0 0 1px rgba(0, 0, 0, 0.04),
    0 8px 32px rgba(0, 0, 0, 0.08),
    0 24px 64px rgba(0, 0, 0, 0.12);
  display: flex;
  align-items: flex-start;
  gap: 1rem;
  overflow: hidden;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.toast:hover {
  transform: translateY(-2px);
  box-shadow: 
    0 0 0 1px rgba(0, 0, 0, 0.04),
    0 12px 40px rgba(0, 0, 0, 0.1),
    0 32px 80px rgba(0, 0, 0, 0.14);
}

.toast-success {
  border-left: 3px solid #10b981;
}

.toast-info {
  border-left: 3px solid #3b82f6;
}

.toast-warning {
  border-left: 3px solid #f59e0b;
}

.toast-error {
  border-left: 3px solid #ef4444;
}

.toast-icon {
  flex-shrink: 0;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.toast-success .toast-icon {
  color: #10b981;
  background: linear-gradient(135deg, #d1fae5 0%, #a7f3d0 100%);
}

.toast-info .toast-icon {
  color: #3b82f6;
  background: linear-gradient(135deg, #dbeafe 0%, #bfdbfe 100%);
}

.toast-warning .toast-icon {
  color: #f59e0b;
  background: linear-gradient(135deg, #fef3c7 0%, #fde68a 100%);
}

.toast-error .toast-icon {
  color: #ef4444;
  background: linear-gradient(135deg, #fee2e2 0%, #fecaca 100%);
}

.toast:hover .toast-icon {
  transform: scale(1.1) rotate(5deg);
}

.toast-content {
  flex: 1;
  min-width: 0;
}

.toast-message {
  font-size: 0.9375rem;
  font-weight: 600;
  color: #1f2937;
  line-height: 1.4;
  letter-spacing: -0.01em;
  margin-bottom: 0.25rem;
}

.toast-sub-message {
  font-size: 0.8125rem;
  font-weight: 500;
  color: #6b7280;
  line-height: 1.4;
  letter-spacing: -0.005em;
}

.toast-progress {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 3px;
  background: rgba(0, 0, 0, 0.05);
  overflow: hidden;
}

.toast-progress-bar {
  height: 100%;
  width: 100%;
  transform-origin: left;
  animation: progress linear forwards;
}

.toast-success .toast-progress-bar {
  background: linear-gradient(90deg, #10b981 0%, #059669 100%);
}

.toast-info .toast-progress-bar {
  background: linear-gradient(90deg, #3b82f6 0%, #2563eb 100%);
}

.toast-warning .toast-progress-bar {
  background: linear-gradient(90deg, #f59e0b 0%, #d97706 100%);
}

.toast-error .toast-progress-bar {
  background: linear-gradient(90deg, #ef4444 0%, #dc2626 100%);
}

@keyframes progress {
  from {
    transform: scaleX(1);
  }
  to {
    transform: scaleX(0);
  }
}

/* Toast 动画 */
.toast-enter-active {
  animation: toastIn 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.toast-leave-active {
  animation: toastOut 0.3s cubic-bezier(0.4, 0, 1, 1);
}

@keyframes toastIn {
  from {
    opacity: 0;
    transform: translateX(100%) scale(0.9);
  }
  to {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
}

@keyframes toastOut {
  from {
    opacity: 1;
    transform: translateX(0) scale(1);
  }
  to {
    opacity: 0;
    transform: translateX(100%) scale(0.95);
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
</style>
