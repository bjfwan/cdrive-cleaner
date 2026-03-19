<script setup lang="ts">
import { defineAsyncComponent, onMounted, provide, ref, shallowRef } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import appIcon from './assets/app-icon.svg';
import DeepScanProgress from './components/DeepScanProgress.vue';
import DiskCard from './components/DiskCard.vue';
import { IconDeepScan, IconHistory, IconScan, IconSettings } from './components/icons';
import ScanProgress from './components/ScanProgress.vue';
import ScanResults from './components/ScanResults.vue';
import Toast from './components/Toast.vue';
import { TOAST_KEY } from './composables/useToast';
import type { AppSettings, DiskInfo, ScanResult, ToastType } from './types';
import { formatBytes } from './utils/format';
import { getSettings } from './utils/settings';

const Settings = defineAsyncComponent(() => import('./components/Settings.vue'));
const History = defineAsyncComponent(() => import('./components/History.vue'));
const Welcome = defineAsyncComponent(() => import('./components/Welcome.vue'));

const disks = ref<DiskInfo[]>([]);
const selectedDisk = ref<string>('');
const scanning = ref(false);
const deepScanning = ref(false);
const scanResult = shallowRef<ScanResult | null>(null);
const error = ref<string>('');
const viewMode = ref<'treemap' | 'list' | 'large-files'>('treemap');
const navigationStack = ref<string[]>([]);
const hasDeepScanned = ref(false);

const showToast = ref(false);
const toastMessage = ref('');
const toastSubMessage = ref('');
const toastType = ref<ToastType>('success');

const showSettings = ref(false);
const showHistory = ref(false);
const showWelcome = ref(false);

let deepScanResult: ScanResult | null = null;
let deepScanCache = new Map<string, ScanResult>();

provide(TOAST_KEY, showToastNotification);

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
      const cDrive = disks.value.find((disk) => disk.drive_letter === 'C:');
      selectedDisk.value = cDrive ? 'C:\\' : `${disks.value[0].drive_letter}\\`;
    }
  } catch {
    error.value = '无法加载磁盘信息';
  }
}

function resetDeepState() {
  deepScanResult = null;
  deepScanCache = new Map();
  hasDeepScanned.value = false;
}

function setRootSnapshot(result: ScanResult) {
  deepScanResult = result;
  deepScanCache = new Map([[selectedDisk.value, result]]);
  hasDeepScanned.value = true;
}

async function loadDeepSnapshot(path: string): Promise<ScanResult> {
  const cached = deepScanCache.get(path);
  if (cached) {
    return cached;
  }

  const snapshot = await invoke<ScanResult>('get_directory_snapshot', {
    rootPath: selectedDisk.value,
    path,
  });
  deepScanCache.set(path, snapshot);
  return snapshot;
}

async function startScan() {
  if (!selectedDisk.value) {
    return;
  }

  scanning.value = true;
  deepScanning.value = false;
  error.value = '';
  scanResult.value = null;
  navigationStack.value = [];
  resetDeepState();

  try {
    const result = await invoke<ScanResult>('scan_disk_incremental', { path: selectedDisk.value });
    scanResult.value = result;
    navigationStack.value = [selectedDisk.value];

    showToastNotification(
      '快速扫描完成',
      `发现 ${result.total_files.toLocaleString()} 个文件 · ${formatBytes(result.total_size)}`,
      'success',
    );
  } catch (err) {
    const msg = String(err);
    if (msg.includes('取消')) {
      showToastNotification('扫描已取消', '', 'info');
    } else {
      error.value = '扫描失败';
      showToastNotification('扫描失败', '无法访问磁盘', 'error');
    }
  } finally {
    scanning.value = false;
  }
}

async function startDeepScan() {
  if (!selectedDisk.value) {
    return;
  }

  if (deepScanning.value) {
    showToastNotification('深度扫描进行中', '请等待当前扫描完成', 'warning');
    return;
  }

  deepScanning.value = true;
  error.value = '';

  const currentPath = navigationStack.value[navigationStack.value.length - 1] || selectedDisk.value;

  try {
    const estimatedFiles = deepScanResult?.total_files ?? scanResult.value?.total_files ?? 800000;
    const rootSnapshot = await invoke<ScanResult>('scan_disk_deep', {
      path: selectedDisk.value,
      estimatedFiles,
    });

    setRootSnapshot(rootSnapshot);

    if (navigationStack.value.length === 0 || currentPath === selectedDisk.value) {
      navigationStack.value = [selectedDisk.value];
      scanResult.value = rootSnapshot;
    } else {
      try {
        scanResult.value = await loadDeepSnapshot(currentPath);
      } catch {
        navigationStack.value = [selectedDisk.value];
        scanResult.value = rootSnapshot;
      }
    }

    showToastNotification(
      '深度扫描完成',
      `发现 ${rootSnapshot.total_files.toLocaleString()} 个文件 · ${formatBytes(rootSnapshot.total_size)}`,
      'success',
    );
  } catch (err) {
    const msg = String(err);
    if (msg.includes('取消')) {
      showToastNotification('深度扫描已取消', '', 'info');
    } else {
      showToastNotification('深度扫描失败', '无法完成深度分析', 'error');
    }
  } finally {
    deepScanning.value = false;
  }
}

function showToastNotification(message: string, subMessage: string, type: ToastType = 'success') {
  toastMessage.value = message;
  toastSubMessage.value = subMessage;
  toastType.value = type;
  showToast.value = true;
}

function closeToast() {
  showToast.value = false;
}

async function navigateToPath(path: string) {
  const currentPath = navigationStack.value[navigationStack.value.length - 1];
  if (!path || path === currentPath) {
    return;
  }

  error.value = '';

  if (hasDeepScanned.value) {
    try {
      scanResult.value = await loadDeepSnapshot(path);
      navigationStack.value.push(path);
      return;
    } catch {
      showToastNotification('目录快照失效', '已回退到即时扫描，请考虑重新深度扫描', 'warning');
    }
  }

  scanning.value = true;

  try {
    const result = await invoke<ScanResult>('scan_disk', { path });
    scanResult.value = result;
    navigationStack.value.push(path);
  } catch {
    error.value = '无法访问该目录';
  } finally {
    scanning.value = false;
  }
}

async function refreshAfterMigration(paths: string[]) {
  if (!selectedDisk.value) {
    return;
  }

  const currentPath = navigationStack.value[navigationStack.value.length - 1] || selectedDisk.value;

  try {
    if (hasDeepScanned.value) {
      const estimatedFiles = deepScanResult?.total_files ?? scanResult.value?.total_files ?? 800000;
      const rootSnapshot = await invoke<ScanResult>('scan_disk_deep', {
        path: selectedDisk.value,
        estimatedFiles,
      });

      setRootSnapshot(rootSnapshot);

      if (currentPath === selectedDisk.value) {
        navigationStack.value = [selectedDisk.value];
        scanResult.value = rootSnapshot;
      } else {
        try {
          scanResult.value = await loadDeepSnapshot(currentPath);
        } catch {
          navigationStack.value = [selectedDisk.value];
          scanResult.value = rootSnapshot;
        }
      }
    } else {
      const result = await invoke<ScanResult>('scan_disk_incremental', { path: selectedDisk.value });
      navigationStack.value = [selectedDisk.value];
      scanResult.value = result;
    }

    showToastNotification('扫描结果已更新', `已同步 ${paths.length} 项迁移后的空间变化`, 'info');
  } catch {
    showToastNotification('迁移已完成', '结果刷新失败，请手动重新扫描一次', 'warning');
  }
}

async function goBack() {
  if (navigationStack.value.length <= 1) {
    return;
  }

  navigationStack.value.pop();
  const previousPath = navigationStack.value[navigationStack.value.length - 1];

  if (hasDeepScanned.value) {
    try {
      scanResult.value = await loadDeepSnapshot(previousPath);
      return;
    } catch {
      navigationStack.value = [selectedDisk.value];
      scanResult.value = deepScanResult;
      return;
    }
  }

  if (previousPath === selectedDisk.value && scanResult.value) {
    return;
  }

  try {
    scanResult.value = await invoke<ScanResult>('scan_disk', { path: previousPath });
  } catch {
    error.value = '无法返回上一层目录';
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

function onSettingsSaved(newSettings: AppSettings) {
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
  const settings = getSettings();
  if (settings.theme) {
    applyTheme(settings.theme);
  }
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
        @migrated="refreshAfterMigration"
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
      @save="onSettingsSaved"
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
