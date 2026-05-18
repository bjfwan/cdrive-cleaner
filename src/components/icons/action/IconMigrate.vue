<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '迁移'"
    :aria-hidden="!label"
    role="img"
  >
    <rect x="2" y="4" width="8" height="10" rx="2" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M14 12h7m0 0l-3-3m3 3l-3 3" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <rect x="14" y="16" width="8" height="6" rx="1.5" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
</template>

<script setup lang="ts">
import type { IconProps } from '../types'

withDefaults(defineProps<IconProps>(), {
  size: 16,
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
