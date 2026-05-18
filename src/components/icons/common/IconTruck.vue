<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '搬运'"
    :aria-hidden="!label"
    role="img"
  >
    <rect x="1" y="3" width="15" height="13" rx="2" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <path 
      d="M16 8h4l3 3v5h-7V8z"
      fill="currentColor"
      opacity="0.15"
      stroke="currentColor" 
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <circle cx="5.5" cy="18.5" r="2.5" stroke="currentColor" stroke-width="1.75" />
    <circle cx="18.5" cy="18.5" r="2.5" stroke="currentColor" stroke-width="1.75" />
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
