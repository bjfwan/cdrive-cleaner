<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const scanning = ref(false);
const scanResult = ref<any>(null);

async function startScan() {
  scanning.value = true;
  try {
    const result = await invoke('scan_disk', { path: 'C:\\' });
    scanResult.value = result;
  } catch (error) {
    console.error('Scan failed:', error);
  } finally {
    scanning.value = false;
  }
}
</script>

<template>
  <div class="app">
    <h1>CDrive Cleaner</h1>
    <button @click="startScan" :disabled="scanning">
      {{ scanning ? 'Scanning...' : 'Scan C Drive' }}
    </button>
    <div v-if="scanResult">
      <p>Total Size: {{ scanResult.total_size }} bytes</p>
      <p>Total Files: {{ scanResult.total_files }}</p>
      <p>Duration: {{ scanResult.scan_duration_ms }}ms</p>
    </div>
  </div>
</template>

<style scoped>
.app {
  padding: 2rem;
  text-align: center;
}

button {
  padding: 0.8rem 1.5rem;
  font-size: 1rem;
  cursor: pointer;
  background: #409eff;
  color: white;
  border: none;
  border-radius: 4px;
}

button:hover:not(:disabled) {
  background: #66b1ff;
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
