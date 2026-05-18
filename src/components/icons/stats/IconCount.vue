<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '计数'"
    :aria-hidden="!label"
    role="img"
  >
    <rect x="3" y="3" width="8" height="8" rx="2" stroke="currentColor" stroke-width="1.75" />
    <rect x="13" y="3" width="8" height="8" rx="2" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
    <rect x="3" y="13" width="8" height="8" rx="2" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="1.75" />
    <rect x="13" y="13" width="8" height="8" rx="2" stroke="currentColor" stroke-width="1.75" />
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
