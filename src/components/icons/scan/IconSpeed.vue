<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '速度'"
    :aria-hidden="!label"
    role="img"
  >
    <path d="M12 2a10 10 0 0 0-8.66 15" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M12 2a10 10 0 0 1 8.66 15" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M12 12l4-6" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
    <circle cx="12" cy="12" r="2" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
    <path d="M4.93 17h.01" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
    <path d="M19.07 17h.01" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
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
