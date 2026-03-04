<script setup lang="ts">
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
        <svg v-if="type === 'danger'" width="48" height="48" viewBox="0 0 48 48" fill="none">
          <circle cx="24" cy="24" r="22" stroke="#ef4444" stroke-width="2"/>
          <path d="M24 16v12m0 4h.01" stroke="#ef4444" stroke-width="3" stroke-linecap="round"/>
        </svg>
        <svg v-else-if="type === 'warning'" width="48" height="48" viewBox="0 0 48 48" fill="none">
          <path d="M24 4L4 40h40L24 4z" stroke="#f59e0b" stroke-width="2" stroke-linejoin="round"/>
          <path d="M24 18v12m0 4h.01" stroke="#f59e0b" stroke-width="3" stroke-linecap="round"/>
        </svg>
        <svg v-else width="48" height="48" viewBox="0 0 48 48" fill="none">
          <circle cx="24" cy="24" r="22" stroke="#3b82f6" stroke-width="2"/>
          <path d="M24 16v12m0 4h.01" stroke="#3b82f6" stroke-width="3" stroke-linecap="round"/>
        </svg>
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

<style scoped src="./ConfirmDialog.css"></style>
