<script setup lang="ts">
import type { IconProps } from '../types';

withDefaults(defineProps<IconProps>(), {
  size: 24
});
</script>

<template>
  <svg 
    :width="size" 
    :height="size" 
    viewBox="0 0 24 24" 
    fill="none" 
    xmlns="http://www.w3.org/2000/svg"
  >
    <path 
      d="M12 4L12 20" 
      stroke="currentColor" 
      stroke-width="2" 
      stroke-linecap="round"
    />
    <path 
      d="M7 8L12 4L17 8" 
      stroke="currentColor" 
      stroke-width="2" 
      stroke-linecap="round" 
      stroke-linejoin="round"
    />
    <path 
      d="M7 16L12 20L17 16" 
      stroke="currentColor" 
      stroke-width="2" 
      stroke-linecap="round" 
      stroke-linejoin="round"
    />
    <circle 
      cx="12" 
      cy="12" 
      r="2.5" 
      fill="currentColor" 
      opacity="0.5"
    />
  </svg>
</template>
