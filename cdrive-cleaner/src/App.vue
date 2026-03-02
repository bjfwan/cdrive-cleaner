<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { TreemapChart } from 'echarts/charts';
import { TitleComponent, TooltipComponent } from 'echarts/components';
import DiskCard from './components/DiskCard.vue';
import ScanResults from './components/ScanResults.vue';

use([CanvasRenderer, TreemapChart, TitleComponent, TooltipComponent]);

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
const deepScanning = ref(false);
const scanResult = ref<ScanResult | null>(null);
const deepScanResult = ref<ScanResult | null>(null);
const error = ref<string>('');
const viewMode = ref<'treemap' | 'list' | 'large-files'>('treemap');
const navigationStack = ref<string[]>([]);
const scanCache = ref<Map<string, ScanResult>>(new Map());

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
    error.value = '无法加载磁盘信息';
  }
}

async function startScan() {
  if (!selectedDisk.value) return;
  scanning.value = true;
  deepScanning.value = false;
  error.value = '';
  scanResult.value = null;
  deepScanResult.value = null;
  navigationStack.value = [];
  scanCache.value.clear();

  try {
    const result = await invoke<ScanResult>('scan_disk', { path: selectedDisk.value });
    console.log('快速扫描结果:', result);
    console.log('directories 数量:', result.directories?.length || 0);
    scanResult.value = result;
    navigationStack.value = [selectedDisk.value];
    scanCache.value.set(selectedDisk.value, result);
    scanning.value = false;
    
    startDeepScan();
  } catch (err) {
    console.error('扫描失败:', err);
    error.value = '扫描失败';
    scanning.value = false;
  }
}

async function startDeepScan() {
  if (!selectedDisk.value) return;
  deepScanning.value = true;

  try {
    const result = await invoke<ScanResult>('scan_disk_deep', { path: selectedDisk.value });
    deepScanResult.value = result;
    scanCache.value.set(selectedDisk.value, result);
    if (navigationStack.value[navigationStack.value.length - 1] === selectedDisk.value) {
      scanResult.value = result;
    }
  } catch (err) {
    console.error('深度扫描失败:', err);
  } finally {
    deepScanning.value = false;
  }
}

async function navigateToPath(path: string) {
  const cachedResult = scanCache.value.get(path);
  
  if (cachedResult && cachedResult.directories.length > 0 && cachedResult.directories[0].children.length > 0) {
    scanResult.value = cachedResult;
    navigationStack.value.push(path);
    return;
  }

  if (deepScanResult.value) {
    const findInTree = (nodes: any[], targetPath: string): any => {
      for (const node of nodes) {
        if (node.path === targetPath) {
          return {
            root_path: node.path,
            total_size: node.size,
            total_files: node.file_count,
            total_dirs: node.children.length,
            scan_duration_ms: 0,
            directories: node.children,
            large_files: [],
            inaccessible_count: 0
          };
        }
        if (node.children && node.children.length > 0) {
          const found = findInTree(node.children, targetPath);
          if (found) return found;
        }
      }
      return null;
    };

    const found = findInTree(deepScanResult.value.directories, path);
    if (found) {
      scanResult.value = found;
      navigationStack.value.push(path);
      scanCache.value.set(path, found);
      console.log('[调试] 从深度扫描树中找到目录:', path, '子目录数:', found.directories.length);
      return;
    }
  }

  scanning.value = true;
  error.value = '';

  try {
    const result = await invoke<ScanResult>('scan_disk', { path });
    scanResult.value = result;
    navigationStack.value.push(path);
    scanCache.value.set(path, result);
  } catch (err) {
    error.value = '无法访问该目录';
  } finally {
    scanning.value = false;
  }
}

function goBack() {
  if (navigationStack.value.length <= 1) return;
  navigationStack.value.pop();
  const previousPath = navigationStack.value[navigationStack.value.length - 1];
  const cached = scanCache.value.get(previousPath);
  if (cached) {
    scanResult.value = cached;
  }
}
</script>

<template>
  <div class="app">
    <aside class="sidebar">
      <div class="brand">
        <h1>存储空间</h1>
      </div>

      <div class="disks-container">
        <DiskCard
          v-for="disk in disks"
          :key="disk.drive_letter"
          :disk="disk"
          :active="selectedDisk === disk.drive_letter + '\\'"
          @select="selectedDisk = disk.drive_letter + '\\'"
        />
      </div>

      <div class="action">
        <button 
          @click="startScan" 
          :disabled="scanning || !selectedDisk"
          class="scan-btn"
        >
          {{ scanning ? '扫描中' : '扫描磁盘' }}
        </button>
        <div v-if="deepScanning" class="deep-scan-status">
          <div class="spinner-small"></div>
          <span>后台深度扫描中...</span>
        </div>
      </div>
    </aside>

    <main class="main">
      <div v-if="!scanResult && !scanning" class="empty">
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none" class="empty-icon">
          <circle cx="32" cy="32" r="30" stroke="currentColor" stroke-width="2"/>
          <path d="M32 16v16l12 0" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
        <h2>选择磁盘开始扫描</h2>
        <p>分析空间占用情况</p>
      </div>

      <div v-if="scanning" class="empty">
        <div class="spinner"></div>
        <h2>正在扫描</h2>
        <p>分析文件系统</p>
      </div>

      <ScanResults
        v-if="scanResult"
        :result="scanResult"
        :view-mode="viewMode"
        :can-go-back="navigationStack.length > 1"
        :current-path="navigationStack[navigationStack.length - 1]"
        :deep-scanning="deepScanning"
        :available-disks="disks"
        @update:view-mode="viewMode = $event"
        @navigate="navigateToPath"
        @go-back="goBack"
      />

      <div v-if="error" class="error">{{ error }}</div>
    </main>
  </div>
</template>

<style>
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600&display=swap');

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  -webkit-font-smoothing: antialiased;
}
</style>

<style scoped>
.app {
  display: flex;
  height: 100vh;
  background: #ffffff;
  color: #2c2c2c;
}

.sidebar {
  width: 340px;
  background: white;
  border-right: 1px solid #e7e5e4;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.brand {
  padding: 2.5rem 2rem 2rem;
  border-bottom: 1px solid #e7e5e4;
}

.brand h1 {
  font-size: 1.5rem;
  font-weight: 600;
  color: #2c2c2c;
  letter-spacing: -0.02em;
}

.disks-container {
  flex: 1;
  overflow-y: auto;
  padding: 2rem 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.action {
  padding: 1.5rem;
  border-top: 1px solid #e7e5e4;
  background: linear-gradient(to top, #fafaf9 0%, #ffffff 100%);
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.scan-btn {
  width: 100%;
  padding: 0.875rem;
  font-size: 0.9375rem;
  font-weight: 500;
  background: linear-gradient(135deg, #007aff 0%, #0051d5 100%);
  color: white;
  border: none;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  box-shadow: 0 4px 12px rgba(0, 122, 255, 0.25);
}

.scan-btn:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 0 8px 20px rgba(0, 122, 255, 0.35);
}

.scan-btn:active:not(:disabled) {
  transform: translateY(0);
}

.scan-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

.deep-scan-status {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem;
  background: #f5f5f4;
  border-radius: 8px;
  font-size: 0.875rem;
  color: #78716c;
}

.spinner-small {
  width: 16px;
  height: 16px;
  border: 2px solid #e7e5e4;
  border-top-color: #007aff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: #ffffff;
}

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 4rem 2rem;
}

.empty-icon {
  color: #d6d3d1;
  margin-bottom: 2rem;
}

.empty h2 {
  font-size: 1.5rem;
  font-weight: 600;
  margin-bottom: 0.5rem;
  letter-spacing: -0.02em;
}

.empty p {
  font-size: 0.9375rem;
  color: #78716c;
}

.spinner {
  width: 48px;
  height: 48px;
  border: 3px solid #e7e5e4;
  border-top-color: #2c2c2c;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin-bottom: 2rem;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.error {
  margin: 2rem;
  padding: 1rem 1.25rem;
  background: #fef2f2;
  color: #991b1b;
  border: 1px solid #fecaca;
  border-radius: 8px;
  font-size: 0.875rem;
}
</style>


@media (max-width: 900px) {
  .app {
    flex-direction: column;
  }

  .sidebar {
    width: 100%;
    border-right: none;
    border-bottom: 1px solid #e7e5e4;
    max-height: 50vh;
  }

  .disks-container {
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  }
}

@media (max-width: 600px) {
  .brand h1 {
    font-size: 1.25rem;
  }

  .disks-container {
    grid-template-columns: 1fr;
    padding: 1.5rem 1rem;
  }
}
