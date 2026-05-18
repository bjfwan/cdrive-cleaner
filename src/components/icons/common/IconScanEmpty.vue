<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    :class="['icon', className]"
    :aria-label="label || '未扫描'"
    :aria-hidden="!label"
    role="img"
  >
    <!-- Folder body -->
    <path 
      d="M2 7V19a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2H9.5L8 5H4a2 2 0 0 0-2 2z"
      fill="currentColor"
      opacity="0.15"
      stroke="currentColor" 
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <!-- Magnifier circle -->
    <circle cx="17.5" cy="11.5" r="4.5" stroke="currentColor" stroke-width="1.75" />
    <!-- Magnifier handle -->
    <path 
      d="M20.7 14.7L23 17"
      stroke="currentColor" 
      stroke-width="1.75"
      stroke-linecap="round"
    />
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
