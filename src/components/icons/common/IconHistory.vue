<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '历史'"
    :aria-hidden="!label"
    role="img"
  >
    <path d="M1 4v6h6" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M3.51 15a9 9 0 1 0 .79-8.27L1 10" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M12 7v5l3 3" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
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
