<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';

interface DiskInfo {
  drive_letter: string;
  label: string;
  file_system: string;
  total_space: number;
  free_space: number;
  used_space: number;
  usage_percent: number;
}

interface ScanResult {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: any[];
  inaccessible_count: number;
}

const disks = ref<DiskInfo[]>([]);
const selectedDisk = ref<string>('');
const scanning = ref(false);
const scanResult = ref<ScanResult | null>(null);
const error = ref<string>('');

onMounted(async () => {
  await loadDisks();
});

async function loadDisks() {
  try {
    disks.value = await invoke<DiskInfo[]>('get_disk_info');
    if (disks.value.length > 0) {
      const cDrive = disks.value.find(d => d.drive_letter === 'C:');
      selectedDisk.value = cDrive ? 'C:\\' : disks.value[0].drive_letter + '\\';
    }
  } catch (err) {
    error.value = `Failed to load disks: ${err}`;
  }
}

async function startScan() {
  if (!selectedDisk.value) {
    error.value = 'Please select a disk';
    return;
  }

  scanning.value = true;
  error.value = '';
  scanResult.value = null;

  try {
    const result = await invoke<ScanResult>('scan_disk', { path: selectedDisk.value });
    scanResult.value = result;
  } catch (err) {
    error.value = `Scan failed: ${err}`;
  } finally {
    scanning.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(2)} ${sizes[i]}`;
}

function formatPercent(percent: number): string {
  return `${percent.toFixed(1)}%`;
}
</script>

<template>
  <div class="app">
    <header>
      <h1>CDrive Cleaner</h1>
      <p>智能 C 盘清理工具</p>
    </header>

    <main>
      <section class="disk-section">
        <h2>磁盘信息</h2>
        <div class="disk-grid">
          <div 
            v-for="disk in disks" 
            :key="disk.drive_letter"
            class="disk-card"
            :class="{ selected: selectedDisk === disk.drive_letter + '\\' }"
            @click="selectedDisk = disk.drive_letter + '\\'"
          >
            <div class="disk-header">
              <span class="disk-letter">{{ disk.drive_letter }}</span>
              <span class="disk-label">{{ disk.label }}</span>
            </div>
            <div class="disk-usage">
              <div class="usage-bar">
                <div 
                  class="usage-fill" 
                  :style="{ width: disk.usage_percent + '%' }"
                  :class="{ 
                    warning: disk.usage_percent > 80,
                    danger: disk.usage_percent > 90 
                  }"
                ></div>
              </div>
              <div class="usage-text">
                {{ formatBytes(disk.used_space) }} / {{ formatBytes(disk.total_space) }}
                ({{ formatPercent(disk.usage_percent) }})
              </div>
            </div>
            <div class="disk-fs">{{ disk.file_system }}</div>
          </div>
        </div>
      </section>

      <section class="scan-section">
        <button 
          @click="startScan" 
          :disabled="scanning || !selectedDisk"
          class="scan-button"
        >
          {{ scanning ? '扫描中...' : '开始扫描' }}
        </button>
      </section>

      <div v-if="error" class="error-message">
        {{ error }}
      </div>

      <section v-if="scanResult" class="result-section">
        <h2>扫描结果</h2>
        <div class="stats-grid">
          <div class="stat-card">
            <div class="stat-label">总大小</div>
            <div class="stat-value">{{ formatBytes(scanResult.total_size) }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">文件数</div>
            <div class="stat-value">{{ scanResult.total_files.toLocaleString() }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">目录数</div>
            <div class="stat-value">{{ scanResult.total_dirs.toLocaleString() }}</div>
          </div>
          <div class="stat-card">
            <div class="stat-label">扫描耗时</div>
            <div class="stat-value">{{ (scanResult.scan_duration_ms / 1000).toFixed(2) }}s</div>
          </div>
        </div>
        <div v-if="scanResult.inaccessible_count > 0" class="warning-message">
          无法访问 {{ scanResult.inaccessible_count }} 个文件/目录
        </div>
      </section>
    </main>
  </div>
</template>

<style scoped>
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

.app {
  min-height: 100vh;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  padding: 2rem;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
}

header {
  text-align: center;
  color: white;
  margin-bottom: 3rem;
}

header h1 {
  font-size: 3rem;
  font-weight: 700;
  margin-bottom: 0.5rem;
}

header p {
  font-size: 1.2rem;
  opacity: 0.9;
}

main {
  max-width: 1200px;
  margin: 0 auto;
}

section {
  background: white;
  border-radius: 12px;
  padding: 2rem;
  margin-bottom: 2rem;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
}

h2 {
  font-size: 1.5rem;
  margin-bottom: 1.5rem;
  color: #333;
}

.disk-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 1.5rem;
}

.disk-card {
  padding: 1.5rem;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.3s ease;
}

.disk-card:hover {
  border-color: #667eea;
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.2);
}

.disk-card.selected {
  border-color: #667eea;
  background: #f8f9ff;
}

.disk-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
}

.disk-letter {
  font-size: 2rem;
  font-weight: 700;
  color: #667eea;
}

.disk-label {
  font-size: 0.9rem;
  color: #666;
}

.disk-usage {
  margin-bottom: 0.5rem;
}

.usage-bar {
  height: 8px;
  background: #e0e0e0;
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 0.5rem;
}

.usage-fill {
  height: 100%;
  background: #667eea;
  transition: width 0.3s ease;
}

.usage-fill.warning {
  background: #f59e0b;
}

.usage-fill.danger {
  background: #ef4444;
}

.usage-text {
  font-size: 0.85rem;
  color: #666;
}

.disk-fs {
  font-size: 0.8rem;
  color: #999;
  margin-top: 0.5rem;
}

.scan-section {
  text-align: center;
}

.scan-button {
  padding: 1rem 3rem;
  font-size: 1.2rem;
  font-weight: 600;
  color: white;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.3s ease;
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}

.scan-button:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 0 6px 16px rgba(102, 126, 234, 0.4);
}

.scan-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
  transform: none;
}

.error-message {
  background: #fee;
  color: #c33;
  padding: 1rem;
  border-radius: 8px;
  margin-bottom: 2rem;
  border-left: 4px solid #c33;
}

.warning-message {
  background: #fff3cd;
  color: #856404;
  padding: 1rem;
  border-radius: 8px;
  margin-top: 1rem;
  border-left: 4px solid #ffc107;
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1.5rem;
}

.stat-card {
  padding: 1.5rem;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border-radius: 8px;
  color: white;
  text-align: center;
}

.stat-label {
  font-size: 0.9rem;
  opacity: 0.9;
  margin-bottom: 0.5rem;
}

.stat-value {
  font-size: 2rem;
  font-weight: 700;
}
</style>
