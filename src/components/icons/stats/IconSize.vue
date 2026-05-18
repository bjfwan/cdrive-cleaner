<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '大小'"
    :aria-hidden="!label"
    role="img"
  >
    <path d="M12 2l9 5v10l-9 5-9-5V7l9-5z" fill="currentColor" opacity="0.15" />
    <path d="M12 2l9 5v10l-9 5-9-5V7l9-5z" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" />
    <path d="M12 22V12" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M21 7l-9 5" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
    <path d="M3 7l9 5" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" />
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
