<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '深度扫描'"
    :aria-hidden="!label"
    role="img"
  >
    <ellipse cx="12" cy="7" rx="9" ry="4" stroke="currentColor" stroke-width="1.75" />
    <path d="M3 7v5c0 2.21 4.03 4 9 4s9-1.79 9-4V7" stroke="currentColor" stroke-width="1.75" />
    <path d="M3 12v5c0 2.21 4.03 4 9 4s9-1.79 9-4v-5" stroke="currentColor" stroke-width="1.75" />
    <path d="M7 12h10" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" opacity="0.5" />
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
