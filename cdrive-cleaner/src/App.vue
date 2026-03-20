<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, provide, ref, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import appIcon from './assets/app-icon.svg';
import ConfirmDialog from './components/ConfirmDialog.vue';
import DeepScanProgress from './components/DeepScanProgress.vue';
import DiskCard from './components/DiskCard.vue';
import { IconDeepScan, IconHistory, IconScan, IconSettings } from './components/icons';
import ScanProgress from './components/ScanProgress.vue';
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
const scanning = ref(false);
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

let deepScanResult: ScanResult | null = null;
let deepScanCache = new Map<string, ScanResult>();

const selectedDiskInfo = computed(() =>
  disks.value.find((disk) => `${disk.drive_letter}\\` === selectedDisk.value),
);

const totalStorage = computed(() => disks.value.reduce((sum, disk) => sum + disk.total_space, 0));
const totalFreeSpace = computed(() => disks.value.reduce((sum, disk) => sum + disk.free_space, 0));
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
const validationCommand = computed(() => {
  if (!selectedDisk.value) {
    return '';
  }

  return `powershell -ExecutionPolicy Bypass -File .\\scripts\\run-admin-mft-validation.ps1 ${selectedDisk.value}`;
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

watch(selectedDisk, (path) => {
  void refreshScanCapabilities(path);
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
      `${formatScanBackendLabel(rootSnapshot.scan_backend)} · ${rootSnapshot.total_files.toLocaleString()} 个文件 · ${formatBytes(rootSnapshot.total_size)}`,
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
      <div class="brand">
        <div class="brand-copy">
          <div class="brand-mark">
            <img :src="appIcon" alt="应用图标" width="40" height="40" />
          </div>
          <div class="brand-text">
            <span class="brand-eyebrow">Space Intelligence</span>
            <h1>存储空间</h1>
            <p>像看产品仪表盘一样，看清磁盘压力、目录热区与迁移机会。</p>
          </div>
        </div>
        <div class="header-actions">
          <button class="settings-icon-btn" @click="openHistory" title="迁移历史">
            <IconHistory :size="20" />
          </button>
          <button class="settings-icon-btn" @click="openSettings" title="设置">
            <IconSettings :size="20" />
          </button>
        </div>
      </div>

      <div class="sidebar-summary">
        <div class="summary-card">
          <span class="summary-kicker">当前焦点</span>
          <span class="summary-value">{{ selectedDiskInfo?.drive_letter || '--' }}</span>
          <span class="summary-note">
            {{ selectedDiskInfo ? `${selectedDiskInfo.label} · ${selectedDiskInfo.file_system}` : '正在读取磁盘信息' }}
          </span>
        </div>

        <div class="summary-card">
          <span class="summary-kicker">可用空间</span>
          <span class="summary-value">{{ formatBytes(totalFreeSpace) }}</span>
          <span class="summary-note">全部磁盘合计 · {{ disks.length }} 个分区</span>
        </div>

        <div class="summary-card summary-card--wide">
          <div>
            <span class="summary-kicker">容量概况</span>
            <span class="summary-value">{{ formatBytes(totalStorage) }}</span>
            <span class="summary-note">
              {{ selectedDiskInfo ? `当前 ${selectedDiskInfo.drive_letter} 已用 ${selectedDiskInfo.usage_percent.toFixed(0)}%` : '选择磁盘后可立即开始分析' }}
            </span>
          </div>
          <div class="summary-meter" :style="selectedDiskMeterStyle">
            <strong>{{ selectedDiskInfo ? `${selectedDiskInfo.usage_percent.toFixed(0)}%` : '--' }}</strong>
            <span>已用占比</span>
          </div>
        </div>
      </div>

      <section class="disk-panel">
        <div class="disk-panel-header">
          <div>
            <h2>磁盘列表</h2>
            <p>先选中目标磁盘，再决定是快速扫描还是深度分析。</p>
          </div>
          <span class="disk-panel-count">{{ disks.length }} 个磁盘</span>
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
      </section>

      <div class="action">
        <div class="action-header">
          <div>
            <h2>开始分析</h2>
            <p>快速扫描适合先看整体结构，深度扫描适合拿到完整目录树与精确迁移决策。</p>
          </div>
          <span class="action-badge">{{ hasDeepScanned ? 'Deep Ready' : 'Quick Start' }}</span>
        </div>

        <div v-if="selectedDisk" class="scan-capability" :data-tone="scanCapabilityTone">
          <div class="scan-capability-copy">
            <span class="scan-capability-kicker">{{ scanCapabilityLabel }}</span>
            <strong>
              {{
                loadingScanCapabilities
                  ? '正在读取卷信息与当前权限状态'
                  : `${selectedDiskInfo?.drive_letter || selectedDisk} · ${scanCapabilities?.file_system || 'Unknown'}`
              }}
            </strong>
            <p>
              {{
                loadingScanCapabilities
                  ? '检测完成后会告诉你本次深度扫描能否直接走 MFT + USN。'
                  : scanCapabilities?.reason || '选择磁盘后会检测深度扫描能力。'
              }}
            </p>
            <code
              v-if="!loadingScanCapabilities && scanCapabilities?.file_system?.toUpperCase() === 'NTFS'"
              class="scan-capability-command"
            >
              管理员端到端验证：{{ validationCommand }}
            </code>
          </div>

          <div v-if="scanCapabilities?.admin_recommended" class="scan-capability-actions">
            <button class="scan-capability-btn" @click="showAdminRestartConfirm = true">
              开启管理员模式
            </button>
          </div>
        </div>

        <button
          @click="startScan"
          :disabled="scanning || deepScanning || !selectedDisk"
          class="scan-btn primary"
        >
          <div class="btn-content">
            <div class="btn-icon-shell">
              <IconScan :size="20" class="btn-icon" />
            </div>
            <div class="btn-copy">
              <span class="btn-label">{{ scanning ? '扫描中...' : '快速扫描' }}</span>
              <span class="btn-hint">几秒内建立空间全景，适合先筛出热点目录。</span>
            </div>
            <span class="btn-tag">Fast</span>
          </div>
          <div class="btn-shimmer"></div>
        </button>

        <button
          @click="startDeepScan"
          :disabled="deepScanning || scanning || !selectedDisk"
          class="scan-btn deep"
        >
          <div class="btn-content">
            <div class="btn-icon-shell">
              <IconDeepScan :size="20" class="btn-icon" />
            </div>
            <div class="btn-copy">
              <span class="btn-label">{{ deepScanning ? '深度扫描中...' : '深度扫描' }}</span>
              <span class="btn-hint">完整目录树、精确统计、迁移能力与更深层的空间洞察。</span>
            </div>
            <span class="btn-tag">Exact</span>
          </div>
          <div class="btn-glow"></div>
        </button>

        <div class="action-footnote">
          {{ selectedDiskInfo ? `当前目标：${selectedDiskInfo.drive_letter} · ${formatBytes(selectedDiskInfo.free_space)} 可用` : '加载磁盘信息后可开始分析' }}
        </div>

        <DeepScanProgress :scanning="deepScanning" />
      </div>
    </aside>

    <main class="main">
      <div v-if="!scanResult && !scanning" class="empty">
        <div class="empty-shell">
          <section class="empty-hero">
            <div class="empty-visual">
              <img :src="appIcon" alt="应用图标" class="empty-icon" width="84" height="84" />
            </div>
            <span class="empty-kicker">Ready To Inspect</span>
            <h2>选择磁盘，开始一次真正有判断力的扫描。</h2>
            <p>
              {{ selectedDiskInfo
                ? `当前已锁定 ${selectedDiskInfo.drive_letter}，你可以先用快速扫描看整体分布，再决定是否开启深度扫描获取完整目录树。`
                : '先从左侧选择目标磁盘。应用会帮你识别空间压力、目录结构和潜在迁移目标。'
              }}
            </p>

            <div class="empty-meta">
              <div class="empty-meta-chip">
                <strong>快速扫描</strong>
                <span>适合先看空间热区与大体分布</span>
              </div>
              <div class="empty-meta-chip">
                <strong>深度扫描</strong>
                <span>拿到完整目录树与精确迁移能力</span>
              </div>
              <div class="empty-meta-chip">
                <strong>迁移与历史</strong>
                <span>迁移后可追踪与回滚，保持路径更可控</span>
              </div>
            </div>
          </section>

          <aside class="empty-preview">
            <div class="empty-preview-header">
              <div>
                <h3>这次优化后的工作流</h3>
                <span>更清晰，更少打扰，更强的状态层次</span>
              </div>
            </div>

            <div class="preview-stack">
              <div class="preview-card">
                <h4>更高级的操作层级</h4>
                <p>把磁盘选择、扫描动作和结果视图拆成明确的三层，不再像普通工具页那样拥挤和平铺。</p>
                <div class="preview-swatch">
                  <span></span>
                  <span></span>
                  <span></span>
                </div>
              </div>

              <div class="preview-card">
                <h4>更细的反馈节奏</h4>
                <p>按钮、通知、弹窗和列表动作会更统一，扫描、深扫、迁移都会有更明确的状态反馈。</p>
              </div>

              <div class="preview-card">
                <h4>更可靠的结果阅读</h4>
                <p>目录排行、列表视图和大文件视图会使用一致的视觉语言，减少切换成本，强化判断效率。</p>
              </div>
            </div>
          </aside>
        </div>
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
