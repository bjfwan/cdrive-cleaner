<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '中风险'"
    :aria-hidden="!label"
    role="img"
  >
    <circle 
      cx="12" 
      cy="12" 
      r="10"
      :stroke="color || '#f97316'" 
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      :fill="color || '#f97316'"
    />
    <path 
      d="M12 8v4M12 16h.01"
      stroke="white" 
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
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
