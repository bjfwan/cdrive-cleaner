<script setup lang="ts">
import { ref, watch } from 'vue';
import { formatBytes } from '../utils/format';

interface Props {
  show: boolean;
  beforeUsed: number;
  afterUsed: number;
  diskTotal: number;
  freedBytes: number;
  itemCount: number;
  operationType: 'migrate' | 'delete' | 'reclaim' | 'redirect';
}

const props = defineProps<Props>();
const emit = defineEmits<{ 'close': [] }>();

const visible = ref(false);
let timer: ReturnType<typeof setTimeout> | null = null;

watch(() => props.show, (val) => {
  if (val) {
    visible.value = true;
    clearTimer();
    timer = setTimeout(() => {
      visible.value = false;
      emit('close');
    }, 3000);
  } else {
    visible.value = false;
    clearTimer();
  }
});

function clearTimer() {
  if (timer) {
    clearTimeout(timer);
    timer = null;
  }
}

function close() {
  visible.value = false;
  clearTimer();
  emit('close');
}

function beforePercent() {
  if (props.diskTotal === 0) return 0;
  return Math.min((props.beforeUsed / props.diskTotal) * 100, 100);
}

function afterPercent() {
  if (props.diskTotal === 0) return 0;
  return Math.min((props.afterUsed / props.diskTotal) * 100, 100);
}

function operationLabel() {
  switch (props.operationType) {
    case 'migrate': return '迁移完成';
    case 'delete': return '清理完成';
    case 'reclaim': return '回收完成';
    case 'redirect': return '重定向完成';
  }
}
</script>

<template>
  <Teleport to="body">
    <transition name="result-fade">
      <div v-if="visible" class="result-overlay" @click.self="close">
        <div class="result-card">
          <button class="result-close" @click="close">✕</button>
          <div class="result-header">
            <span class="result-icon">✓</span>
            <strong>{{ operationLabel() }}</strong>
          </div>

          <div class="result-bars">
            <div class="result-bar-group">
              <span class="result-bar-label">之前</span>
              <div class="result-bar-track">
                <div class="result-bar-fill before" :style="{ width: beforePercent() + '%' }"></div>
              </div>
              <span class="result-bar-value">{{ formatBytes(beforeUsed) }}</span>
            </div>
            <div class="result-bar-group">
              <span class="result-bar-label">现在</span>
              <div class="result-bar-track">
                <div class="result-bar-fill after" :style="{ width: afterPercent() + '%' }"></div>
              </div>
              <span class="result-bar-value">{{ formatBytes(afterUsed) }}</span>
            </div>
          </div>

          <div class="result-hero">
            <strong>释放了 {{ formatBytes(freedBytes) }}</strong>
            <small>{{ itemCount }} 个项目</small>
          </div>
        </div>
      </div>
    </transition>
  </Teleport>
</template>

<style scoped>
.result-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  pointer-events: none;
}

.result-card {
  position: relative;
  width: min(380px, 90vw);
  padding: 1.5rem;
  border-radius: var(--radius-lg);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xl);
  pointer-events: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
}

.result-close {
  position: absolute;
  top: 12px;
  right: 12px;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  font-size: 0.8rem;
  transition: color var(--transition-fast);
}

.result-close:hover {
  color: var(--color-text-primary);
}

.result-header {
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.result-icon {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: rgba(15, 159, 110, 0.12);
  color: var(--color-success);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 1rem;
  font-weight: 700;
}

.result-header strong {
  font-size: 1rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.result-bars {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.result-bar-group {
  display: grid;
  grid-template-columns: 36px minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.6rem;
}

.result-bar-label {
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
  font-weight: 600;
}

.result-bar-track {
  height: 14px;
  border-radius: 999px;
  background: var(--color-bg-tertiary);
  overflow: hidden;
}

.result-bar-fill {
  height: 100%;
  border-radius: 999px;
  transition: width 0.6s cubic-bezier(0.16, 1, 0.3, 1);
}

.result-bar-fill.before {
  background: rgba(217, 119, 6, 0.6);
}

.result-bar-fill.after {
  background: var(--color-success);
}

.result-bar-value {
  font-size: 0.76rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
  min-width: 60px;
  text-align: right;
}

.result-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.2rem;
}

.result-hero strong {
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--color-highlight);
  font-feature-settings: 'tnum';
}

.result-hero small {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}

.result-fade-enter-active {
  transition: opacity var(--transition-base);
}

.result-fade-enter-active .result-card {
  transition: transform var(--transition-base), opacity var(--transition-base);
}

.result-fade-leave-active {
  transition: opacity var(--transition-base);
}

.result-fade-leave-active .result-card {
  transition: transform var(--transition-base), opacity var(--transition-base);
}

.result-fade-enter-from {
  opacity: 0;
}

.result-fade-enter-from .result-card {
  opacity: 0;
  transform: translateY(12px) scale(0.95);
}

.result-fade-leave-to {
  opacity: 0;
}

.result-fade-leave-to .result-card {
  opacity: 0;
  transform: translateY(-8px) scale(0.98);
}
</style>
