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

// Toast 通知
const showToast = ref(false);
const toastMessage = ref('');
const toastSubMessage = ref('');
const toastType = ref<'success' | 'info' | 'warning' | 'error'>('success');

// 设置
const showSettings = ref(false);
const showHistory = ref(false);

onMounted(async () => {
  await loadDisks();
  loadUserSettings();
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
  
  try {
    const cached = await invoke<ScanResult | null>('get_scan_cache', { 
      diskPath: selectedDisk.value,
      scanType: 'quick'
    });
    
    if (cached) {
      scanResult.value = cached;
      navigationStack.value = [selectedDisk.value];
      scanCache.value.set(selectedDisk.value, cached);
      hasDeepScanned.value = false;
      
      showToastNotification(
        '已加载缓存',
        `发现 ${cached.total_files.toLocaleString()} 个文件 · ${formatBytes(cached.total_size)}`,
        'info'
      );
      return;
    }
  } catch (err) {
    console.log('无缓存，开始扫描');
  }
  
  scanning.value = true;
  deepScanning.value = false;
  error.value = '';
  scanResult.value = null;
  deepScanResult.value = null;
  navigationStack.value = [];
  scanCache.value.clear();
  hasDeepScanned.value = false;

  try {
    const result = await invoke<ScanResult>('scan_disk', { path: selectedDisk.value });
    scanResult.value = result;
    navigationStack.value = [selectedDisk.value];
    scanCache.value.set(selectedDisk.value, result);
    
    await invoke('save_scan_cache', {
      diskPath: selectedDisk.value,
      scanType: 'quick',
      result: result
    });
    
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
  deepScanning.value = true;

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
    
    // 显示完成通知
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
  // 应用大文件阈值设置
  if (newSettings.largeFileThreshold) {
    // 可以在这里更新大文件视图的阈值
    console.log('应用大文件阈值:', newSettings.largeFileThreshold);
  }
  
  // 应用主题设置
  if (newSettings.theme) {
    applyTheme(newSettings.theme);
  }
  
  showToastNotification('设置已保存', '您的偏好设置已成功保存', 'success');
}

function applyTheme(theme: 'light' | 'dark' | 'auto') {
  // TODO: 实现主题切换逻辑
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
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 5v5l3 3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              <path d="M3 10a7 7 0 0 1 12-4.9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              <path d="M17 10a7 7 0 0 1-12 4.9" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
          </button>
          <button class="settings-icon-btn" @click="openSettings" title="设置">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z" stroke="currentColor" stroke-width="1.5"/>
              <path d="M17.5 10.833v-1.666a1.667 1.667 0 0 0-1.25-1.617l-.833-.208a.833.833 0 0 1-.584-.584l-.208-.833a1.667 1.667 0 0 0-1.617-1.25H11.34a1.667 1.667 0 0 0-1.617 1.25l-.208.833a.833.833 0 0 1-.584.584l-.833.208a1.667 1.667 0 0 0-1.25 1.617v1.666a1.667 1.667 0 0 0 1.25 1.617l.833.208a.833.833 0 0 1 .584.584l.208.833a1.667 1.667 0 0 0 1.617 1.25h1.667a1.667 1.667 0 0 0 1.617-1.25l.208-.833a.833.833 0 0 1 .584-.584l.833-.208a1.667 1.667 0 0 0 1.25-1.617z" stroke="currentColor" stroke-width="1.5"/>
            </svg>
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
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" class="btn-icon">
              <circle cx="10" cy="10" r="8" stroke="currentColor" stroke-width="1.5" opacity="0.3"/>
              <path d="M10 6v8M6 10h8" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
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
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" class="btn-icon">
              <path d="M10 3L10 17" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
              <path d="M6 7L10 3L14 7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              <path d="M6 13L10 17L14 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
              <circle cx="10" cy="10" r="2" fill="currentColor" opacity="0.5"/>
            </svg>
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
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none" class="empty-icon">
          <circle cx="32" cy="32" r="30" stroke="currentColor" stroke-width="2"/>
          <path d="M32 16v16l12 0" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
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
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.brand h1 {
  font-size: 1.5rem;
  font-weight: 600;
  color: #2c2c2c;
  letter-spacing: -0.02em;
}

.header-actions {
  display: flex;
  gap: 8px;
}

.settings-icon-btn {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border: none;
  border-radius: 10px;
  color: #57534e;
  cursor: pointer;
  transition: all 0.2s ease;
}

.settings-icon-btn:hover {
  background: #e7e5e4;
  color: #1c1917;
  transform: scale(1.05);
}

.settings-icon-btn:first-child:hover {
  transform: rotate(-15deg) scale(1.05);
}

.settings-icon-btn:last-child:hover {
  transform: rotate(45deg) scale(1.05);
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
  gap: 0.875rem;
}

.scan-btn {
  width: 100%;
  padding: 0;
  font-size: 0.9375rem;
  font-weight: 500;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  position: relative;
  overflow: hidden;
  isolation: isolate;
}

.btn-content {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 1rem 1.25rem;
}

.btn-icon {
  flex-shrink: 0;
  transition: transform 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
}

/* Primary Button - Quick Scan */
.scan-btn.primary {
  background: linear-gradient(135deg, #0066ff 0%, #0047b3 100%);
  color: white;
  box-shadow: 
    0 4px 16px rgba(0, 102, 255, 0.2),
    0 1px 3px rgba(0, 0, 0, 0.1);
}

.scan-btn.primary .btn-shimmer {
  position: absolute;
  top: 0;
  left: -100%;
  width: 100%;
  height: 100%;
  background: linear-gradient(
    90deg,
    transparent 0%,
    rgba(255, 255, 255, 0.2) 50%,
    transparent 100%
  );
  transition: left 0.6s ease;
}

.scan-btn.primary:hover:not(:disabled) {
  transform: translateY(-2px);
  box-shadow: 
    0 8px 24px rgba(0, 102, 255, 0.3),
    0 2px 6px rgba(0, 0, 0, 0.15);
}

.scan-btn.primary:hover:not(:disabled) .btn-shimmer {
  left: 100%;
}

.scan-btn.primary:hover:not(:disabled) .btn-icon {
  transform: rotate(90deg) scale(1.1);
}

/* Deep Scan Button - Distinctive Design */
.scan-btn.deep {
  background: linear-gradient(135deg, #fafaf9 0%, #f5f5f4 100%);
  color: #1c1917;
  border: 1.5px solid #e7e5e4;
  box-shadow: 
    0 2px 8px rgba(0, 0, 0, 0.04),
    inset 0 1px 0 rgba(255, 255, 255, 0.8);
}

.scan-btn.deep .btn-text {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.125rem;
  flex: 1;
}

.scan-btn.deep .btn-label {
  font-weight: 600;
  font-size: 0.9375rem;
  letter-spacing: -0.01em;
}

.scan-btn.deep .btn-hint {
  font-size: 0.75rem;
  font-weight: 400;
  color: #78716c;
  letter-spacing: 0.01em;
}

.scan-btn.deep .btn-glow {
  position: absolute;
  inset: 0;
  background: radial-gradient(
    circle at center,
    rgba(0, 102, 255, 0.08) 0%,
    transparent 70%
  );
  opacity: 0;
  transition: opacity 0.4s ease;
}

.scan-btn.deep:hover:not(:disabled) {
  background: linear-gradient(135deg, #ffffff 0%, #fafaf9 100%);
  border-color: #d6d3d1;
  transform: translateY(-1px);
  box-shadow: 
    0 4px 16px rgba(0, 0, 0, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 1);
}

.scan-btn.deep:hover:not(:disabled) .btn-glow {
  opacity: 1;
}

.scan-btn.deep:hover:not(:disabled) .btn-icon {
  transform: translateY(3px) scale(1.05);
}

.scan-btn.deep:hover:not(:disabled) .btn-hint {
  color: #57534e;
}

/* Active State */
.scan-btn:active:not(:disabled) {
  transform: translateY(0) scale(0.98);
}

/* Disabled State */
.scan-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  transform: none;
}

.scan-btn:disabled .btn-icon {
  transform: none;
}

.scan-btn:disabled .btn-shimmer,
.scan-btn:disabled .btn-glow {
  display: none;
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

.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  backdrop-filter: blur(4px);
}

.modal-content {
  background: white;
  border-radius: 20px;
  width: 90%;
  max-width: 1200px;
  max-height: 90vh;
  overflow: hidden;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  position: relative;
  display: flex;
  flex-direction: column;
}

.modal-close {
  position: absolute;
  top: 20px;
  right: 20px;
  width: 36px;
  height: 36px;
  border: none;
  background: #f5f5f4;
  color: #57534e;
  border-radius: 50%;
  font-size: 20px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s ease;
  z-index: 10;
}

.modal-close:hover {
  background: #e7e5e4;
  color: #1c1917;
  transform: rotate(90deg);
}

.modal-content > * {
  overflow-y: auto;
}
