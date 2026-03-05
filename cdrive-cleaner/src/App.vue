<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { TreemapChart } from 'echarts/charts';
import { TitleComponent, TooltipComponent } from 'echarts/components';
import DiskCard from './components/DiskCard.vue';
import ScanResults from './components/ScanResults.vue';
import ScanProgress from './components/ScanProgress.vue';
import DeepScanProgress from './components/DeepScanProgress.vue';
import Toast from './components/Toast.vue';
import Settings from './components/Settings.vue';
import History from './components/History.vue';
import Welcome from './components/Welcome.vue';
import appIcon from './assets/app-icon.svg';
import { IconHistory, IconSettings, IconScan, IconDeepScan } from './components/icons';

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

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
  is_symlink: boolean;
  link_target?: string;
}

interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
  modified_at: string;
  is_readonly: boolean;
}

interface ScanResult {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: DirectoryNode[];
  large_files: FileInfo[];
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
const hasDeepScanned = ref(false);

const showToast = ref(false);
const toastMessage = ref('');
const toastSubMessage = ref('');
const toastType = ref<'success' | 'info' | 'warning' | 'error'>('success');

const showSettings = ref(false);
const showHistory = ref(false);
const showWelcome = ref(false);

onMounted(async () => {
  await loadDisks();
  loadUserSettings();
  checkFirstLaunch();
});

function checkFirstLaunch() {
  const hasShown = localStorage.getItem('cdrive-cleaner-welcome-shown');
  if (!hasShown) {
    showWelcome.value = true;
  }
}

function closeWelcome() {
  showWelcome.value = false;
}

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
  hasDeepScanned.value = false;

  try {
    const result = await invoke<ScanResult>('scan_disk_incremental', { path: selectedDisk.value });
    scanResult.value = result;
    navigationStack.value = [selectedDisk.value];
    scanCache.value.set(selectedDisk.value, result);
    
    showToastNotification(
      '快速扫描完成',
      `发现 ${result.total_files.toLocaleString()} 个文件 · ${formatBytes(result.total_size)}`,
      'success'
    );
  } catch (err) {
    console.error('扫描失败:', err);
    error.value = '扫描失败';
    showToastNotification('扫描失败', '无法访问磁盘', 'error');
  } finally {
    scanning.value = false;
  }
}

async function startDeepScan() {
  if (!selectedDisk.value) return;
  
  // 如果正在扫描，直接返回
  if (deepScanning.value) {
    showToastNotification(
      '深度扫描进行中',
      '请等待当前扫描完成',
      'warning'
    );
    return;
  }
  
  // 如果已经有深度扫描结果，提示用户
  if (hasDeepScanned.value && deepScanResult.value) {
    showToastNotification(
      '已完成深度扫描',
      '当前磁盘已有深度扫描结果',
      'info'
    );
    return;
  }
  
  // 立即设置状态，提供即时反馈
  deepScanning.value = true;
  
  // 先检查是否有深度扫描缓存
  try {
    const cached = await invoke<ScanResult | null>('get_scan_cache', { 
      diskPath: selectedDisk.value,
      scanType: 'deep'
    });
    
    if (cached) {
      deepScanResult.value = cached;
      scanCache.value.set(selectedDisk.value, cached);
      hasDeepScanned.value = true;
      
      if (navigationStack.value.length === 0) {
        navigationStack.value = [selectedDisk.value];
        scanResult.value = cached;
      } else if (navigationStack.value[navigationStack.value.length - 1] === selectedDisk.value) {
        scanResult.value = cached;
      }
      
      showToastNotification(
        '已加载深度扫描缓存',
        `发现 ${cached.total_files.toLocaleString()} 个文件 · ${formatBytes(cached.total_size)}`,
        'info'
      );
      deepScanning.value = false;
      return;
    }
  } catch (err) {
    console.log('无深度扫描缓存，开始扫描');
  }

  try {
    const estimatedFiles = scanResult.value?.total_files ?? 800000;
    
    const result = await invoke<ScanResult>('scan_disk_deep', { 
      path: selectedDisk.value,
      estimatedFiles: estimatedFiles
    });
    deepScanResult.value = result;
    scanCache.value.set(selectedDisk.value, result);
    hasDeepScanned.value = true;
    
    if (navigationStack.value.length === 0) {
      navigationStack.value = [selectedDisk.value];
      scanResult.value = result;
    } else if (navigationStack.value[navigationStack.value.length - 1] === selectedDisk.value) {
      scanResult.value = result;
    }
    
    showToastNotification(
      '深度扫描完成',
      `发现 ${result.total_files.toLocaleString()} 个文件 · ${formatBytes(result.total_size)}`,
      'success'
    );
  } catch (err) {
    console.error('深度扫描失败:', err);
    showToastNotification('深度扫描失败', '无法完成深度分析', 'error');
  } finally {
    deepScanning.value = false;
  }
}

function showToastNotification(message: string, subMessage: string, type: 'success' | 'info' | 'warning' | 'error' = 'success') {
  toastMessage.value = message;
  toastSubMessage.value = subMessage;
  toastType.value = type;
  showToast.value = true;
}

function closeToast() {
  showToast.value = false;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
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

function openSettings() {
  showSettings.value = true;
}

function closeSettings() {
  showSettings.value = false;
}

function openHistory() {
  showHistory.value = true;
}

function closeHistory() {
  showHistory.value = false;
}

function saveSettings(newSettings: any) {
  if (newSettings.largeFileThreshold) {
    console.log('应用大文件阈值:', newSettings.largeFileThreshold);
  }
  
  if (newSettings.theme) {
    applyTheme(newSettings.theme);
  }
  
  showToastNotification('设置已保存', '您的偏好设置已成功保存', 'success');
}

function applyTheme(theme: 'light' | 'dark' | 'auto') {
  if (theme === 'auto') {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light');
  } else {
    document.documentElement.setAttribute('data-theme', theme);
  }
}

function loadUserSettings() {
  const saved = localStorage.getItem('cdrive-cleaner-settings');
  if (saved) {
    try {
      const settings = JSON.parse(saved);
      if (settings.theme) {
        applyTheme(settings.theme);
      }
      return settings;
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  }
  return null;
}
</script>

<template>
  <div class="app">
    <aside class="sidebar">
      <div class="brand">
        <h1>存储空间</h1>
        <div class="header-actions">
          <button class="settings-icon-btn" @click="openHistory" title="迁移历史">
            <IconHistory :size="20" />
          </button>
          <button class="settings-icon-btn" @click="openSettings" title="设置">
            <IconSettings :size="20" />
          </button>
        </div>
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
          :disabled="scanning || deepScanning || !selectedDisk"
          class="scan-btn primary"
        >
          <div class="btn-content">
            <IconScan :size="20" class="btn-icon" />
            <span>{{ scanning ? '扫描中...' : '快速扫描' }}</span>
          </div>
          <div class="btn-shimmer"></div>
        </button>
        
        <button 
          @click="startDeepScan" 
          :disabled="deepScanning || scanning || !selectedDisk"
          class="scan-btn deep"
        >
          <div class="btn-content">
            <IconDeepScan :size="20" class="btn-icon" />
            <div class="btn-text">
              <span class="btn-label">{{ deepScanning ? '深度扫描中...' : '深度扫描' }}</span>
              <span class="btn-hint">完整目录树 · 精确数据</span>
            </div>
          </div>
          <div class="btn-glow"></div>
        </button>
        
        <DeepScanProgress :scanning="deepScanning" />
      </div>
    </aside>

    <main class="main">
      <div v-if="!scanResult && !scanning" class="empty">
        <img :src="appIcon" alt="应用图标" class="empty-icon" width="120" height="120" />
        <h2>选择磁盘开始扫描</h2>
        <p>分析空间占用情况</p>
      </div>

      <ScanResults
        v-if="scanResult"
        :result="scanResult"
        :view-mode="viewMode"
        :can-go-back="navigationStack.length > 1"
        :current-path="navigationStack[navigationStack.length - 1]"
        :deep-scanning="deepScanning"
        :available-disks="disks"
        :has-deep-scanned="hasDeepScanned"
        @update:view-mode="viewMode = $event"
        @navigate="navigateToPath"
        @go-back="goBack"
        @start-deep-scan="startDeepScan"
      />

      <div v-if="error" class="error">{{ error }}</div>
    </main>

    <ScanProgress :scanning="scanning" />
    
    <Toast 
      :show="showToast"
      :message="toastMessage"
      :sub-message="toastSubMessage"
      :type="toastType"
      @close="closeToast"
    />

    <Settings
      :show="showSettings"
      :available-disks="disks"
      @close="closeSettings"
      @save="saveSettings"
    />

    <div v-if="showHistory" class="modal-overlay" @click="closeHistory">
      <div class="modal-content" @click.stop>
        <button class="modal-close" @click="closeHistory">✕</button>
        <History />
      </div>
    </div>

    <Welcome v-if="showWelcome" @close="closeWelcome" />
  </div>
</template>

<style scoped>
/* All styles are imported from external CSS files in main.ts */
</style>
