<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '错误'"
    :aria-hidden="!label"
    role="img"
  >
    <circle cx="12" cy="12" r="10" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
    <path d="M15 9l-6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M9 9l6 6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
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
