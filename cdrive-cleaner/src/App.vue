<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, provide, ref, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import appIcon from './assets/app-icon.svg';
import ConfirmDialog from './components/ConfirmDialog.vue';
import DeepScanProgress from './components/DeepScanProgress.vue';
import DiskCard from './components/DiskCard.vue';
import { IconDeepScan, IconHistory, IconSettings } from './components/icons';
import ScanResults from './components/ScanResults.vue';
import Toast from './components/Toast.vue';
import { TOAST_KEY } from './composables/useToast';
import type { AppSettings, DiskInfo, ScanCapabilities, ScanResult, ToastType } from './types';
import { formatBytes } from './utils/format';
import { getSettings } from './utils/settings';

const Settings = defineAsyncComponent(() => import('./components/Settings.vue'));
const History = defineAsyncComponent(() => import('./components/History.vue'));
const Welcome = defineAsyncComponent(() => import('./components/Welcome.vue'));

const disks = ref<DiskInfo[]>([]);
const selectedDisk = ref<string>('');
const deepScanning = ref(false);
const scanResult = shallowRef<ScanResult | null>(null);
const scanCapabilities = shallowRef<ScanCapabilities | null>(null);
const error = ref<string>('');
const viewMode = ref<'treemap' | 'list' | 'large-files'>('treemap');
const navigationStack = ref<string[]>([]);
const hasDeepScanned = ref(false);
const loadingScanCapabilities = ref(false);
const showAdminRestartConfirm = ref(false);

const showToast = ref(false);
const toastMessage = ref('');
const toastSubMessage = ref('');
const toastType = ref<ToastType>('success');

const showSettings = ref(false);
const showHistory = ref(false);
const showWelcome = ref(false);
const appSettings = ref<AppSettings>(getSettings());

let deepScanResult: ScanResult | null = null;
let deepScanCache = new Map<string, ScanResult>();

const selectedDiskInfo = computed(() =>
  disks.value.find((disk) => `${disk.drive_letter}\\` === selectedDisk.value),
);

const totalStorage = computed(() => disks.value.reduce((sum, disk) => sum + disk.total_space, 0));
const totalFreeSpace = computed(() => disks.value.reduce((sum, disk) => sum + disk.free_space, 0));
const activeAnalysisPath = computed(
  () => navigationStack.value[navigationStack.value.length - 1] || selectedDisk.value,
);
const scanCapabilityTone = computed<'ready' | 'warning' | 'fallback'>(() => {
  if (scanCapabilities.value?.mft_available) {
    return 'ready';
  }

  if (scanCapabilities.value?.admin_recommended) {
    return 'warning';
  }

  return 'fallback';
});
const scanCapabilityLabel = computed(() => {
  if (loadingScanCapabilities.value) {
    return '检测深度扫描能力中';
  }

  if (scanCapabilities.value?.mft_available) {
    return 'MFT + USN 已就绪';
  }

  if (scanCapabilities.value?.admin_recommended) {
    return '建议开启管理员模式';
  }

  return '将回退到原生目录枚举';
});
const scanCapabilityShortLabel = computed(() => {
  if (loadingScanCapabilities.value) {
    return '检测中';
  }

  if (scanCapabilities.value?.mft_available) {
    return 'MFT + USN';
  }

  if (scanCapabilities.value?.admin_recommended) {
    return '需管理员';
  }

  return '目录枚举';
});

const selectedDiskMeterStyle = computed(() => {
  const usage = selectedDiskInfo.value?.usage_percent ?? 0;
  const usageDegrees = Math.min(Math.max(usage, 0), 100) * 3.6;
  return {
    background: `
      radial-gradient(circle at center, rgba(255, 255, 255, 0.88) 0 55%, transparent 56%),
      conic-gradient(
        from -90deg,
        rgba(15, 118, 110, 0.88) 0deg,
        rgba(15, 118, 110, 0.88) ${usageDegrees}deg,
        rgba(15, 118, 110, 0.16) ${usageDegrees}deg,
        rgba(15, 118, 110, 0.08) 360deg
      )
    `,
  };
});

provide(TOAST_KEY, showToastNotification);

onMounted(async () => {
  await loadDisks();
  loadUserSettings();
  checkFirstLaunch();
});

watch(
  selectedDisk,
  (path, prev) => {
    void refreshScanCapabilities(path);

    if (path && path !== prev) {
      // Avoid mixing scan results / caches across volumes.
      scanResult.value = null;
      navigationStack.value = [];
      error.value = '';
      resetDeepState();
    }
  },
);

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
    const next = await invoke<DiskInfo[]>('get_disk_info');
    const previousSelection = selectedDisk.value;
    disks.value = next;

    if (next.length > 0) {
      const previousStillExists =
        !!previousSelection && next.some((disk) => `${disk.drive_letter}\\` === previousSelection);
      if (previousStillExists) {
        selectedDisk.value = previousSelection;
        return;
      }

      const cDrive = next.find((disk) => disk.drive_letter === 'C:');
      selectedDisk.value = cDrive ? 'C:\\' : `${next[0].drive_letter}\\`;
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

function formatScanBackendLabel(backend?: string | null) {
  switch (backend) {
    case 'mft_usn':
      return 'MFT + USN';
    case 'incremental_usn':
      return 'USN 增量合并';
    case 'native':
      return '原生目录枚举';
    default:
      return '自动后端';
  }
}

async function refreshScanCapabilities(path = selectedDisk.value) {
  if (!path) {
    scanCapabilities.value = null;
    return;
  }

  loadingScanCapabilities.value = true;
  try {
    scanCapabilities.value = await invoke<ScanCapabilities>('get_scan_capabilities', { path });
  } catch {
    scanCapabilities.value = null;
  } finally {
    loadingScanCapabilities.value = false;
  }
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

async function startDeepScan() {
  if (!selectedDisk.value) {
    return;
  }

  if (deepScanning.value) {
    showToastNotification('扫描进行中', '请等待当前扫描完成', 'warning');
    return;
  }

  deepScanning.value = true;
  error.value = '';

  const currentPath = navigationStack.value[navigationStack.value.length - 1] || selectedDisk.value;

  if (scanCapabilities.value?.admin_recommended) {
    showToastNotification(
      '当前是标准权限',
      '本次深度扫描会回退到原生目录枚举；若想启用 MFT + USN，请先点击“开启管理员模式”。',
      'warning',
    );
  }

  try {
    const estimatedFiles = deepScanResult?.total_files ?? scanResult.value?.total_files ?? 800000;
    const rootSnapshot = await invoke<ScanResult>('scan_disk_deep', {
      path: selectedDisk.value,
      estimatedFiles,
    });

    await loadDisks();
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

    const diskUsed = selectedDiskInfo.value?.used_space;
    const missing = diskUsed === undefined ? undefined : Math.max(diskUsed - rootSnapshot.total_size, 0);

    showToastNotification(
      '扫描完成',
      `${formatScanBackendLabel(rootSnapshot.scan_backend)} · ${rootSnapshot.total_files.toLocaleString()} 个文件 · 扫描到 ${formatBytes(rootSnapshot.total_size)}${diskUsed === undefined ? '' : ` · 磁盘已用 ${formatBytes(diskUsed)} · 漏算 ${formatBytes(missing ?? 0)}`}`,
      'success',
    );
  } catch (err) {
    const msg = String(err);
    if (msg.includes('取消')) {
      showToastNotification('扫描已取消', '', 'info');
    } else {
      showToastNotification('扫描失败', '无法完成扫描', 'error');
    }
  } finally {
    deepScanning.value = false;
  }
}

function showToastNotification(message: string, subMessage: string, type: ToastType = 'success') {
  const applyToast = () => {
    toastMessage.value = message;
    toastSubMessage.value = subMessage;
    toastType.value = type;
    showToast.value = true;
  };

  if (showToast.value) {
    showToast.value = false;
    window.setTimeout(applyToast, 20);
    return;
  }

  applyToast();
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

  try {
    if (!hasDeepScanned.value) {
      showToastNotification('需要先扫描', '请先执行一次扫描以启用目录下钻', 'warning');
      return;
    }

    scanResult.value = await loadDeepSnapshot(path);
    navigationStack.value.push(path);
  } catch {
    showToastNotification('目录快照失效', '请重新执行一次扫描', 'warning');
  }
}

async function refreshAfterMigration(paths: string[]) {
  if (!selectedDisk.value) {
    return;
  }

  const currentPath = navigationStack.value[navigationStack.value.length - 1] || selectedDisk.value;

  try {
    const estimatedFiles = deepScanResult?.total_files ?? scanResult.value?.total_files ?? 800000;
    const rootSnapshot = await invoke<ScanResult>('scan_disk_deep', {
      path: selectedDisk.value,
      estimatedFiles,
    });

    await loadDisks();
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

  try {
    if (!hasDeepScanned.value) {
      navigationStack.value = [];
      scanResult.value = null;
      return;
    }

    scanResult.value = await loadDeepSnapshot(previousPath);
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
  appSettings.value = newSettings;
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
  appSettings.value = settings;
  if (settings.theme) {
    applyTheme(settings.theme);
  }
}

async function restartAsAdmin() {
  try {
    await invoke('restart_as_admin');
  } catch {
    showToastNotification('管理员重启失败', '请手动以管理员身份运行应用', 'error');
  } finally {
    showAdminRestartConfirm.value = false;
  }
}
</script>

<template>
  <div class="app">
    <aside class="sidebar">
      <div class="brand brand--rail">
        <div class="brand-copy">
          <div class="brand-mark">
            <img :src="appIcon" alt="应用图标" width="40" height="40" />
          </div>
          <div class="brand-text">
            <span class="brand-eyebrow">CSD</span>
            <h1>空间</h1>
            <p>磁盘工作台</p>
          </div>
        </div>
      </div>

      <section class="drive-rail">
        <div class="drive-rail-header">
          <span>磁盘</span>
          <strong>{{ disks.length }}</strong>
        </div>

        <button
          v-for="disk in disks"
          :key="disk.drive_letter"
          class="drive-rail-item"
          :class="{ active: selectedDisk === `${disk.drive_letter}\\` }"
          @click="selectedDisk = `${disk.drive_letter}\\`"
        >
          <div class="drive-rail-main">
            <strong>{{ disk.drive_letter }}</strong>
            <span>{{ disk.file_system }}</span>
          </div>
          <small>{{ disk.usage_percent.toFixed(0) }}%</small>
        </button>
      </section>

      <div class="rail-footnote">
        <span class="rail-footnote-dot"></span>
        <span>
          {{ selectedDiskInfo ? `${selectedDiskInfo.drive_letter} · ${formatBytes(selectedDiskInfo.free_space)} 可用` : '选择磁盘开始' }}
        </span>
      </div>
    </aside>

    <main class="main">
      <header class="workspace-topbar">
        <div class="workspace-topbar-main">
          <div class="workspace-title">
            <span class="workspace-kicker">Workspace</span>
            <h2>
              {{ selectedDiskInfo ? `${selectedDiskInfo.drive_letter} · ${selectedDiskInfo.label || 'Local Disk'}` : '选择磁盘开始' }}
            </h2>
            <p>
              {{ selectedDiskInfo
                ? `${selectedDiskInfo.file_system} · ${formatBytes(selectedDiskInfo.total_space)} 总容量`
                : '从左侧选择一个磁盘开始分析'
              }}
            </p>
          </div>

          <div class="workspace-chips">
            <div class="workspace-chip">
              <span>可用</span>
              <strong>{{ selectedDiskInfo ? formatBytes(selectedDiskInfo.free_space) : formatBytes(totalFreeSpace) }}</strong>
            </div>

            <div class="workspace-chip">
              <span>已用</span>
              <strong>{{ selectedDiskInfo ? `${selectedDiskInfo.usage_percent.toFixed(0)}%` : '--' }}</strong>
            </div>

            <div class="workspace-chip" :data-tone="scanCapabilityTone">
              <span>后端</span>
              <strong>{{ scanCapabilityShortLabel }}</strong>
            </div>
          </div>
        </div>

        <div class="workspace-actions">
          <button class="topbar-tool" @click="openHistory" title="迁移历史">
            <IconHistory :size="18" />
          </button>
          <button class="topbar-tool" @click="openSettings" title="设置">
            <IconSettings :size="18" />
          </button>

          <button
            class="topbar-cta topbar-cta--strong"
            @click="startDeepScan"
            :disabled="deepScanning || !selectedDisk"
          >
            <IconDeepScan :size="18" />
            <span>{{ deepScanning ? '扫描中' : '开始扫描' }}</span>
          </button>
        </div>
      </header>

      <div class="main-frame">
        <div class="main-scroll">
          <DeepScanProgress v-show="deepScanning" :scanning="deepScanning" class="main-deep-progress" />

          <div v-if="!scanResult && !deepScanning" class="empty">
            <div class="dashboard">
              <section class="dashboard-panel dashboard-panel--hero">
                <div class="dashboard-hero-copy">
                  <span class="dashboard-kicker">{{ selectedDiskInfo ? '当前焦点' : '准备开始' }}</span>
                  <h2>{{ selectedDiskInfo ? `${selectedDiskInfo.drive_letter} 空间概览` : '选择磁盘开始扫描' }}</h2>
                  <p>{{ selectedDiskInfo ? '先看概览，再开始扫描。' : '左侧选盘，右侧开始分析。' }}</p>
                </div>

                <div v-if="selectedDiskInfo" class="dashboard-meter" :style="selectedDiskMeterStyle">
                  <strong>{{ selectedDiskInfo.usage_percent.toFixed(0) }}%</strong>
                  <span>已用</span>
                </div>
              </section>

              <aside
                v-if="selectedDisk"
                class="dashboard-panel dashboard-panel--capability"
                :data-tone="scanCapabilityTone"
              >
                <span class="dashboard-label">{{ scanCapabilityLabel }}</span>
                <strong>{{ scanCapabilityShortLabel }}</strong>
                <p>
                  {{
                    loadingScanCapabilities
                      ? '正在检测可用后端'
                      : scanCapabilities?.mft_available
                        ? '适合直接做深度扫描'
                        : scanCapabilities?.admin_recommended
                          ? '管理员模式可启用 MFT + USN'
                          : '将使用目录枚举'
                  }}
                </p>

                <button
                  v-if="scanCapabilities?.admin_recommended"
                  class="dashboard-inline-btn"
                  @click="showAdminRestartConfirm = true"
                >
                  开启管理员模式
                </button>
              </aside>

              <section class="dashboard-metrics">
                <div class="dashboard-stat">
                  <span>当前焦点</span>
                  <strong>{{ selectedDiskInfo?.drive_letter || '--' }}</strong>
                  <small>{{ selectedDiskInfo ? `${selectedDiskInfo.label} · ${selectedDiskInfo.file_system}` : '等待选择磁盘' }}</small>
                </div>

                <div class="dashboard-stat">
                  <span>可用空间</span>
                  <strong>{{ selectedDiskInfo ? formatBytes(selectedDiskInfo.free_space) : formatBytes(totalFreeSpace) }}</strong>
                  <small>{{ selectedDiskInfo ? '当前磁盘剩余' : '全部磁盘合计' }}</small>
                </div>

                <div class="dashboard-stat">
                  <span>总容量</span>
                  <strong>{{ selectedDiskInfo ? formatBytes(selectedDiskInfo.total_space) : formatBytes(totalStorage) }}</strong>
                  <small>{{ selectedDiskInfo ? '当前磁盘容量' : `${disks.length} 个磁盘` }}</small>
                </div>

                <div class="dashboard-stat">
                  <span>扫描建议</span>
                  <strong>{{ hasDeepScanned ? '可继续下钻' : '直接开始扫描' }}</strong>
                  <small>{{ hasDeepScanned ? '结果已支持下钻' : '扫描后可按目录层级浏览与迁移' }}</small>
                </div>
              </section>

              <section class="dashboard-panel dashboard-panel--disks">
                <div class="dashboard-section-head">
                  <div>
                    <h3>磁盘概览</h3>
                    <p>完整卡片放到主工作区，方便比较。</p>
                  </div>
                </div>

                <div class="dashboard-disk-grid">
                  <DiskCard
                    v-for="disk in disks"
                    :key="disk.drive_letter"
                    :disk="disk"
                    :active="selectedDisk === disk.drive_letter + '\\'"
                    @select="selectedDisk = disk.drive_letter + '\\'"
                  />
                </div>
              </section>
            </div>
          </div>

          <ScanResults
            v-if="scanResult"
            :result="scanResult"
            :view-mode="viewMode"
            :can-go-back="navigationStack.length > 1"
            :current-path="activeAnalysisPath"
            :deep-scanning="deepScanning"
            :available-disks="disks"
            :has-deep-scanned="hasDeepScanned"
            :large-file-threshold="appSettings.largeFileThreshold"
            @update:view-mode="viewMode = $event"
            @navigate="navigateToPath"
            @go-back="goBack"
            @start-deep-scan="startDeepScan"
            @migrated="refreshAfterMigration"
          />

          <div v-if="error" class="error">{{ error }}</div>
        </div>
      </div>
    </main>
    
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

    <ConfirmDialog
      :show="showAdminRestartConfirm"
      title="以管理员身份重启以启用 MFT + USN？"
      message="应用会关闭并重新启动。请在 Windows 的 UAC 提示中点击“是”，之后深度扫描会优先走 MFT + USN 路径。"
      confirm-text="立即重启"
      cancel-text="稍后"
      type="warning"
      @confirm="restartAsAdmin"
      @cancel="showAdminRestartConfirm = false"
    />
  </div>
</template>

<style scoped>
/* All styles are imported from external CSS files in main.ts */
</style>
