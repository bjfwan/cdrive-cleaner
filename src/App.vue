<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, provide, ref, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import appIcon from './assets/app-icon.svg';
import ConfirmDialog from './components/ConfirmDialog.vue';
import DeepScanProgress from './components/DeepScanProgress.vue';
import { IconDeepScan, IconHistory, IconSettings } from './components/icons';
import Toast from './components/Toast.vue';
import Cart from './components/Cart.vue';
import Workspace from './components/Workspace.vue';
import { TOAST_KEY } from './composables/useToast';
import { useCart } from './composables/useCart';
import type { AppSettings, DeleteMode, DeleteResult, DiskInfo, ScanCapabilities, ScanResult, ToastType } from './types';
import { formatBytes } from './utils/format';
import { getSettings } from './utils/settings';

const Settings = defineAsyncComponent(() => import('./components/Settings.vue'));
const History = defineAsyncComponent(() => import('./components/History.vue'));
const Welcome = defineAsyncComponent(() => import('./components/Welcome.vue'));
const MigrateDialog = defineAsyncComponent(() => import('./components/MigrateDialog.vue'));
const CommandPalette = defineAsyncComponent(() => import('./components/CommandPalette.vue'));
const GamesView = defineAsyncComponent(() => import('./components/GamesView.vue'));

const disks = ref<DiskInfo[]>([]);
const selectedDisk = ref<string>('');
const deepScanning = ref(false);
const scanResult = shallowRef<ScanResult | null>(null);
const scanCapabilities = shallowRef<ScanCapabilities | null>(null);
const error = ref<string>('');
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
const showCommandPalette = ref(false);
const showMigrateDialog = ref(false);
const activeTab = ref<'workspace' | 'games'>('workspace');
const gameDetectionDone = ref(false);
const migrateTargetItem = ref<{ path: string; name: string; size: number; file_count: number } | null>(null);
const cartBusy = ref(false);
const cartProgress = ref<{ current: number; total: number; currentItem: string }>({ current: 0, total: 0, currentItem: '' });
const cartResults = ref<Array<{ path: string; ok: true } | { path: string; ok: false; name: string; error: string }>>([]);
const cartConfirm = ref<{ open: boolean; targetDisk: string; deleteMode: DeleteMode; migrateCount: number; deleteCount: number; type: 'danger' | 'warning' | 'info' }>({
  open: false,
  targetDisk: '',
  deleteMode: 'recycle',
  migrateCount: 0,
  deleteCount: 0,
  type: 'warning',
});
const appSettings = ref<AppSettings>(getSettings());

const cart = useCart();

let deepScanResult: ScanResult | null = null;
let deepScanCache = new Map<string, ScanResult>();
const driveSessions = new Map<string, { root: ScanResult; navStack: string[] }>();

const selectedDiskInfo = computed(() =>
  disks.value.find((disk) => `${disk.drive_letter}\\` === selectedDisk.value),
);

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

provide(TOAST_KEY, showToastNotification);

onMounted(async () => {
  await loadDisks();
  loadUserSettings();
  checkFirstLaunch();
  window.addEventListener('keydown', onGlobalKey);
  void notifyOnGameDetection();
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
});

function onGlobalKey(e: KeyboardEvent) {
  const ctrlOrMeta = e.ctrlKey || e.metaKey;
  if (ctrlOrMeta && (e.key === 'k' || e.key === 'K')) {
    e.preventDefault();
    showCommandPalette.value = !showCommandPalette.value;
    return;
  }
  if (e.key === 'Escape') {
    if (showCommandPalette.value) {
      showCommandPalette.value = false;
    } else if (showSettings.value) {
      closeSettings();
    } else if (showHistory.value) {
      closeHistory();
    } else if (showMigrateDialog.value) {
      closeMigrateDialog();
    } else if (showAdminRestartConfirm.value) {
      showAdminRestartConfirm.value = false;
    }
  }
}

watch(
  selectedDisk,
  async (path, prev) => {
    void refreshScanCapabilities(path);

    if (!path || path === prev) {
      return;
    }

    if (prev && scanResult.value) {
      driveSessions.set(prev, {
        root: deepScanResult ?? scanResult.value,
        navStack: [...navigationStack.value],
      });
    }

    error.value = '';

    const session = driveSessions.get(path);
    if (session) {
      deepScanResult = session.root;
      deepScanCache = new Map([[path, session.root]]);
      navigationStack.value = session.navStack.length > 0 ? [...session.navStack] : [path];
      hasDeepScanned.value = true;
      const currentPath = navigationStack.value[navigationStack.value.length - 1];
      try {
        scanResult.value = currentPath === path ? session.root : await loadDeepSnapshot(currentPath);
      } catch {
        scanResult.value = session.root;
        navigationStack.value = [path];
      }
      return;
    }

    scanResult.value = null;
    navigationStack.value = [];
    resetDeepState();
  },
);

function checkFirstLaunch() {
  const onboardingDone = localStorage.getItem('cdrive-cleaner-onboarding-completed');
  const legacyShown = localStorage.getItem('cdrive-cleaner-welcome-shown');
  if (!onboardingDone && !legacyShown) {
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
  driveSessions.set(selectedDisk.value, {
    root: result,
    navStack: navigationStack.value.length > 0 ? [...navigationStack.value] : [selectedDisk.value],
  });
}

function syncDriveSession() {
  if (!hasDeepScanned.value || !selectedDisk.value || !deepScanResult) return;
  driveSessions.set(selectedDisk.value, {
    root: deepScanResult,
    navStack: navigationStack.value.length > 0 ? [...navigationStack.value] : [selectedDisk.value],
  });
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
    syncDriveSession();
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
    syncDriveSession();
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

async function runCart(payload: { targetDisk: string; deleteMode: DeleteMode }) {
  if (cart.count.value === 0) return;

  const migrateCount = cart.migrateItems.value.length;
  const deleteCount = cart.deleteItems.value.length + cart.reviewItems.value.length;
  const isPermanent = payload.deleteMode === 'permanent';
  const needsConfirm = deleteCount > 0 && (isPermanent || migrateCount > 0);

  if (needsConfirm) {
    cartConfirm.value = {
      open: true,
      targetDisk: payload.targetDisk,
      deleteMode: payload.deleteMode,
      migrateCount,
      deleteCount,
      type: isPermanent ? 'danger' : 'warning',
    };
    return;
  }

  await executeCart(payload.targetDisk, payload.deleteMode);
}

const cartConfirmTitle = computed(() => {
  const c = cartConfirm.value;
  if (c.deleteMode === 'permanent' && c.deleteCount > 0) {
    return `永久删除 ${c.deleteCount} 项？此操作不可恢复`;
  }
  if (c.migrateCount > 0 && c.deleteCount > 0) {
    return `处理 ${c.migrateCount + c.deleteCount} 项？`;
  }
  if (c.deleteCount > 0) {
    return `清理 ${c.deleteCount} 项到回收站？`;
  }
  return '执行迁移？';
});

const cartConfirmMessage = computed(() => {
  const c = cartConfirm.value;
  const parts: string[] = [];
  if (c.migrateCount > 0) {
    parts.push(`${c.migrateCount} 项搬走到 ${c.targetDisk}`);
  }
  if (c.deleteCount > 0) {
    if (c.deleteMode === 'permanent') {
      parts.push(`${c.deleteCount} 项将从硬盘永久删除（跳过回收站）`);
    } else {
      parts.push(`${c.deleteCount} 项移入 Windows 回收站`);
    }
  }
  return parts.join('，') + '。';
});

async function confirmCart() {
  const c = cartConfirm.value;
  cartConfirm.value = { ...c, open: false };
  await executeCart(c.targetDisk, c.deleteMode);
}

function cancelCartConfirm() {
  cartConfirm.value = { ...cartConfirm.value, open: false };
}

async function executeCart(targetDisk: string, deleteMode: DeleteMode) {
  cartBusy.value = true;
  cartResults.value = [];
  const succeededPaths: string[] = [];
  const succeeded: Array<{ path: string; ok: true }> = [];
  const failed: Array<{ path: string; name: string; error: string }> = [];

  const deleteOnlyItems = [...cart.deleteItems.value];
  const migrateOnlyItems = [...cart.migrateItems.value];
  const total = deleteOnlyItems.length + migrateOnlyItems.length;
  cartProgress.value = { current: 0, total, currentItem: '' };
  let processed = 0;

  try {
    for (const item of deleteOnlyItems) {
      cartProgress.value = { current: processed, total, currentItem: item.name };
      const ok = await runCartDelete(item, deleteMode, failed);
      if (ok) {
        succeeded.push({ path: item.path, ok: true });
        succeededPaths.push(item.path);
        cart.remove(item.path);
      }
      processed += 1;
    }

    for (const item of migrateOnlyItems) {
      cartProgress.value = { current: processed, total, currentItem: item.name };
      const ok = await runCartMigrate(item, targetDisk, failed);
      if (ok) {
        succeeded.push({ path: item.path, ok: true });
        succeededPaths.push(item.path);
        cart.remove(item.path);
      }
      processed += 1;
    }
  } finally {
    cartBusy.value = false;
    cartProgress.value = { current: total, total, currentItem: '' };
    cartResults.value = [
      ...succeeded,
      ...failed.map((f) => ({ path: f.path, ok: false as const, name: f.name, error: f.error })),
    ];
  }

  if (succeededPaths.length > 0) {
    await refreshAfterMigration(succeededPaths);
  }
}

async function runCartMigrate(
  item: { path: string; name: string; size: number; file_count: number; source?: string; game?: { platform: string; app_id: string } },
  targetDisk: string,
  failed: Array<{ path: string; name: string; error: string }>,
): Promise<boolean> {
  try {
    if (item.source === 'game' && item.game) {
      const result = await invoke<{ success: boolean; error?: string | null }>('migrate_game', {
        platform: item.game.platform,
        appId: item.game.app_id,
        sourcePath: item.path,
        targetDisk,
        knownSize: item.size,
        knownFiles: item.file_count,
      });
      if (!result.success) {
        failed.push({ path: item.path, name: item.name, error: humanizeError(result.error || '游戏迁移失败') });
        return false;
      }
      return true;
    }

    const safety = await invoke<{ can_migrate: boolean; findings: Array<{ severity: string; message: string }> }>(
      'analyze_migration_safety',
      {
        path: item.path,
        size: item.size,
        linkType: appSettings.value.createSymlink ? null : 'none',
        targetDisk,
      },
    );
    if (!safety.can_migrate) {
      const reason = safety.findings.filter((f) => f.severity === 'blocker').map((f) => f.message).join('；') || '安全检测未通过';
      failed.push({ path: item.path, name: item.name, error: humanizeError(reason) });
      return false;
    }

    const result = await invoke<{ success: boolean; error?: string | null }>('migrate_file', {
      source: item.path,
      targetDisk,
      linkType: appSettings.value.createSymlink ? null : 'none',
      knownSize: item.size,
      knownFiles: item.file_count,
    });
    if (!result.success) {
      failed.push({ path: item.path, name: item.name, error: humanizeError(result.error || '迁移失败') });
      return false;
    }
    return true;
  } catch (err) {
    failed.push({ path: item.path, name: item.name, error: humanizeError(String(err)) });
    return false;
  }
}

async function runCartDelete(
  item: { path: string; name: string; size: number },
  mode: DeleteMode,
  failed: Array<{ path: string; name: string; error: string }>,
): Promise<boolean> {
  try {
    const result = await invoke<DeleteResult>('delete_path', {
      path: item.path,
      mode,
    });
    if (!result.success) {
      const firstError = result.errors[0]?.error ?? '清理失败';
      failed.push({ path: item.path, name: item.name, error: humanizeError(firstError) });
      return false;
    }
    return true;
  } catch (err) {
    failed.push({ path: item.path, name: item.name, error: humanizeError(String(err)) });
    return false;
  }
}

function humanizeError(raw: string): string {
  const lower = raw.toLowerCase();
  if (lower.includes('系统关键路径') || lower.includes('system_critical')) {
    return '系统关键路径，禁止删除或迁移。';
  }
  if (lower.includes('移入回收站失败')) {
    return '无法移入回收站，可能权限不足或目标在网络盘。可在设置里切换为永久删除。';
  }
  if (lower.includes('移除目录失败')) {
    return '部分文件被进程占用，目录未能完全清空。请关闭相关应用后重试。';
  }
  if (lower.includes('拒绝访问') || lower.includes('access') || lower.includes('error 5') || lower.includes('error 32')) {
    return '文件被进程占用。请关闭对应应用（如 Trae、Chrome、VSCode、Docker 等）后重试。';
  }
  if (lower.includes('disk full') || lower.includes('not enough space') || lower.includes('空间')) {
    return '目标磁盘空间不足。';
  }
  if (lower.includes('not found') || lower.includes('找不到') || lower.includes('路径不存在')) {
    return '源路径已不存在，可能在扫描后被删除。';
  }
  if (lower.includes('permission') || lower.includes('权限')) {
    return '权限不足，请以管理员身份运行。';
  }
  return raw.replace(/^[A-Za-z]+:?\s*/, '');
}

function openMigrateSingle(item: { path: string; name: string; size: number; file_count: number }) {
  migrateTargetItem.value = item;
  showMigrateDialog.value = true;
}

function closeMigrateDialog() {
  showMigrateDialog.value = false;
  migrateTargetItem.value = null;
}

async function notifyOnGameDetection() {
  if (gameDetectionDone.value) return;
  gameDetectionDone.value = true;
  if (localStorage.getItem('cdrive-cleaner-games-toast-shown')) return;
  try {
    const libs = await invoke<Array<{ platform: string; installed: boolean; games: unknown[] }>>('detect_game_libraries');
    const installedNames = libs
      .filter((l) => l.installed && l.games.length > 0)
      .map((l) => {
        switch (l.platform) {
          case 'steam':
            return 'Steam';
          case 'epic':
            return 'Epic';
          case 'game_pass':
            return 'Game Pass';
          default:
            return l.platform;
        }
      });
    if (installedNames.length === 0) return;
    showToastNotification(
      '检测到游戏库',
      `${installedNames.join(' / ')} · 切到「游戏库」标签可一键搬到其他盘`,
      'info',
    );
    localStorage.setItem('cdrive-cleaner-games-toast-shown', '1');
  } catch {
    // 检测失败不打扰用户
  }
}

async function revealInExplorer(path: string) {
  try {
    await invoke('reveal_in_explorer', { path });
  } catch (err) {
    console.warn('reveal failed', err);
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
                ? `${selectedDiskInfo.file_system} · ${formatBytes(selectedDiskInfo.total_space)} 总容量 · 已用 ${selectedDiskInfo.usage_percent.toFixed(0)}%`
                : '从左侧选择一个磁盘开始分析'
              }}
            </p>
          </div>

          <div class="workspace-chips">
            <div class="workspace-tabs">
              <button
                class="workspace-tab"
                :class="{ active: activeTab === 'workspace' }"
                @click="activeTab = 'workspace'"
              >磁盘</button>
              <button
                class="workspace-tab"
                :class="{ active: activeTab === 'games' }"
                @click="activeTab = 'games'"
              >游戏库</button>
            </div>
            <div class="workspace-chip" :data-tone="scanCapabilityTone" :title="scanCapabilityLabel">
              <span>后端</span>
              <strong>{{ scanCapabilityShortLabel }}</strong>
            </div>
            <button
              v-if="scanCapabilities?.admin_recommended"
              class="workspace-chip workspace-chip--btn"
              @click="showAdminRestartConfirm = true"
            >
              <span>权限</span>
              <strong>开启管理员</strong>
            </button>
          </div>
        </div>

        <div class="workspace-actions">
          <button class="topbar-tool" @click="showCommandPalette = true" title="命令面板（Ctrl+K）">
            <span class="kbd">⌘ K</span>
          </button>
          <button class="topbar-tool" @click="openHistory" title="迁移历史">
            <IconHistory :size="18" />
          </button>
          <button class="topbar-tool" @click="openSettings" title="设置">
            <IconSettings :size="18" />
          </button>

          <button
            v-if="activeTab === 'workspace'"
            class="topbar-cta topbar-cta--strong"
            @click="startDeepScan"
            :disabled="deepScanning || !selectedDisk"
          >
            <IconDeepScan :size="18" />
            <span>{{ deepScanning ? '扫描中' : hasDeepScanned ? '重新扫描' : '开始扫描' }}</span>
          </button>
        </div>
      </header>

      <div class="main-frame">
        <div class="main-scroll">
          <DeepScanProgress v-show="deepScanning" :scanning="deepScanning" class="main-deep-progress" />

          <Workspace
            v-if="activeTab === 'workspace'"
            :scan-result="scanResult"
            :selected-disk="selectedDisk"
            :selected-disk-info="selectedDiskInfo ?? null"
            :available-disks="disks"
            :has-deep-scanned="hasDeepScanned"
            :deep-scanning="deepScanning"
            :large-file-threshold="appSettings.largeFileThreshold"
            @start-scan="startDeepScan"
            @navigate="navigateToPath"
            @migrate-single="openMigrateSingle"
            @reveal="revealInExplorer"
          />

          <GamesView
            v-else
            :available-disks="disks"
            :current-drive="selectedDisk"
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

    <Cart
      :available-disks="disks"
      :current-drive="selectedDisk"
      :busy="cartBusy"
      :progress="cartProgress"
      :results="cartResults"
      :default-delete-mode="appSettings.defaultDeleteMode"
      @run="runCart"
      @reset-results="cartResults = []"
    />

    <CommandPalette
      :show="showCommandPalette"
      :scan-result="scanResult"
      @close="showCommandPalette = false"
      @jump="navigateToPath"
    />

    <MigrateDialog
      :show="showMigrateDialog"
      :selected-dir="migrateTargetItem ? { path: migrateTargetItem.path, name: migrateTargetItem.name, size: migrateTargetItem.size, file_count: migrateTargetItem.file_count, dir_count: 0, children: [], has_children: false, is_symlink: false } : null"
      :selected-file="null"
      :selected-items="[]"
      :available-disks="disks.filter((d) => `${d.drive_letter}\\` !== selectedDisk)"
      @close="closeMigrateDialog"
      @migrated="(p) => { closeMigrateDialog(); refreshAfterMigration(p); }"
    />

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

    <ConfirmDialog
      :show="cartConfirm.open"
      :title="cartConfirmTitle"
      :message="cartConfirmMessage"
      :confirm-text="cartConfirm.deleteMode === 'permanent' ? '我确认永久删除' : '继续执行'"
      cancel-text="再想想"
      :type="cartConfirm.type"
      @confirm="confirmCart"
      @cancel="cancelCartConfirm"
    />
  </div>
</template>

<style scoped>
/* All styles are imported from external CSS files in main.ts */
</style>
