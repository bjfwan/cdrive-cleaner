<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className, 'icon-spinning']"
    :aria-label="label || '加载中'"
    :aria-hidden="!label"
    role="img"
  >
    <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M22 12a10 10 0 0 1-10 10" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M12 22a10 10 0 0 1-10-10" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
  </svg>
</template>

<script setup lang="ts">
import type { IconProps } from '../types'

withDefaults(defineProps<IconProps>(), {
  size: 24,
  className: '',
  label: '',
  spinning: true
})
</script>

<style scoped>
.icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
}

.icon-spinning {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
