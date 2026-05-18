<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '搬运车'"
    :aria-hidden="!label"
    role="img"
  >
    <path 
      d="M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6"
      stroke="currentColor" 
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <circle cx="9" cy="21" r="1.5" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
    <circle cx="20" cy="21" r="1.5" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
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
