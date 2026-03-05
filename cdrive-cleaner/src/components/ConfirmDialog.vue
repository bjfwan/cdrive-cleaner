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

<style scoped src="./ConfirmDialog.css"></style>
