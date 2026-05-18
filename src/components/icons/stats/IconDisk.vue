<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '磁盘'"
    :aria-hidden="!label"
    role="img"
  >
    <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="1.75" />
    <circle cx="12" cy="12" r="4" stroke="currentColor" stroke-width="1.75" />
    <circle cx="12" cy="12" r="1.5" fill="currentColor" />
    <path d="M14 10l5-5" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
  </svg>
</template>

<script setup lang="ts">
import type { IconProps } from '../types'

withDefaults(defineProps<IconProps>(), {
  size: 24,
  className: '',
  label: '',
  spinning: false
})
</script>

<style scoped>
.icon {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
}
</style>
