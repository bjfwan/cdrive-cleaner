<script setup lang="ts">
import { IconError, IconWarning, IconInfo } from './icons';
interface Props {
  show: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
}

const props = withDefaults(defineProps<Props>(), {
  confirmText: '确定',
  cancelText: '取消',
  type: 'danger'
});

const emit = defineEmits<{
  'confirm': [];
  'cancel': [];
}>();
</script>

<template>
  <div v-if="show" class="confirm-overlay" @click.self="emit('cancel')">
    <div class="confirm-dialog" @click.stop>
      <div class="confirm-icon" :class="`icon-${type}`">
        <IconError v-if="type === 'danger'" :size="48" />
        <IconWarning v-else-if="type === 'warning'" :size="48" />
        <IconInfo v-else :size="48" />
      </div>
      <h3>{{ title }}</h3>
      <p>{{ message }}</p>
      <div class="confirm-actions">
        <button class="btn btn-secondary" @click="emit('cancel')">{{ cancelText }}</button>
        <button class="btn" :class="`btn-${type}`" @click="emit('confirm')">{{ confirmText }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(45, 35, 25, 0.4);
  backdrop-filter: blur(24px) saturate(100%);
  -webkit-backdrop-filter: blur(24px) saturate(100%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
  animation: fadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans);
}

@keyframes fadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.confirm-dialog {
  width: 90%;
  max-width: 420px;
  padding: 2.5rem 2rem 2rem;
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: 24px;
  box-shadow: 
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  text-align: center;
  animation: slideUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
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
  color: var(--color-error);
}

.icon-warning {
  color: var(--color-warning);
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
  letter-spacing: -0.02em;
}

.confirm-dialog p {
  font-size: 0.9375rem;
  color: var(--color-text-tertiary);
  line-height: 1.6;
  margin-bottom: 2rem;
}

.confirm-actions {
  display: flex;
  gap: 0.875rem;
  justify-content: center;
}

.btn {
  min-width: 120px;
  padding: 0.875rem 2rem;
  font-size: 1rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: -0.01em;
}

.btn-secondary {
  color: var(--color-text-primary);
  background: rgba(139, 92, 46, 0.12);
  border: 1px solid var(--color-border-strong);
  font-weight: 600;
}

.btn-secondary:hover {
  background: rgba(139, 92, 46, 0.18);
  border-color: var(--color-border-strong);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.btn-danger {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-error) 0%, #dc2626 100%);
  box-shadow: 
    0 0 0 1px rgba(239, 68, 68, 0.2),
    0 4px 16px rgba(239, 68, 68, 0.3);
}

.btn-danger:hover {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(239, 68, 68, 0.3),
    0 8px 24px rgba(239, 68, 68, 0.4);
}

.btn-warning {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-warning) 0%, #d97706 100%);
  box-shadow: 
    0 0 0 1px rgba(245, 158, 11, 0.2),
    0 4px 16px rgba(245, 158, 11, 0.3);
}

.btn-warning:hover {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(245, 158, 11, 0.3),
    0 8px 24px rgba(245, 158, 11, 0.4);
}

.btn-info {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-info) 0%, #2563eb 100%);
  box-shadow: 
    0 0 0 1px rgba(59, 130, 246, 0.2),
    0 4px 16px rgba(59, 130, 246, 0.3);
}

.btn-info:hover {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(59, 130, 246, 0.3),
    0 8px 24px rgba(59, 130, 246, 0.4);
}

@media (max-width: 480px) {
  .confirm-dialog {
    width: 95%;
    padding: 2rem 1.5rem 1.5rem;
  }

  .confirm-actions {
    flex-direction: column;
  }

  .btn {
    width: 100%;
  }
}
</style>
