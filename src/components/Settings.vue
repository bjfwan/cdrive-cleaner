<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import ConfirmDialog from './ConfirmDialog.vue';
import { IconClose, IconRefresh, IconLink, IconInfo, IconWarning, IconShield } from './icons';
import appIcon from '../assets/app-icon.png';
import type { DiskInfo, CacheEntry, CacheInfo, AppSettings } from '../types';
import { formatBytes, formatDate } from '../utils/format';
import { getSettings, saveSettings as persistSettings, syncTrackASettingsToBackend } from '../utils/settings';
import { useToast } from '../composables/useToast';
import DiskSelect from './DiskSelect.vue';
import { aiStore, BUILTIN_MODELS, CURRENT_CONSENT_VERSION, type AiSnapshot } from '../store/ai';

const showToast = useToast();

type ThemeMode = 'light' | 'dark' | 'auto';

function readInitialTheme(): ThemeMode {
  // Prefer the unified AppSettings.theme, fall back to the legacy
  // standalone key (where 'system' meant 'auto') so existing users keep their pick.
  const fromSettings = getSettings().theme;
  if (fromSettings === 'light' || fromSettings === 'dark' || fromSettings === 'auto') {
    return fromSettings;
  }
  const legacy = localStorage.getItem('cdrive-cleaner-theme');
  if (legacy === 'light' || legacy === 'dark') return legacy;
  if (legacy === 'system' || legacy === 'auto') return 'auto';
  return 'auto';
}

const themeMode = ref<ThemeMode>(readInitialTheme());

function applyTheme(mode: ThemeMode) {
  let effective: 'light' | 'dark' = 'light';
  if (mode === 'auto') {
    effective = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } else {
    effective = mode;
  }
  document.documentElement.setAttribute('data-theme', effective);
}

function setTheme(mode: ThemeMode) {
  themeMode.value = mode;
  // Keep the legacy key in sync for any old reader (App.vue onMounted),
  // and persist the unified AppSettings.theme too so 保存设置 doesn't reset it.
  localStorage.setItem('cdrive-cleaner-theme', mode);
  applyTheme(mode);
}

onMounted(() => {
  applyTheme(themeMode.value);
});

interface Props {
  show: boolean;
  availableDisks: DiskInfo[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'close': [];
  'save': [settings: AppSettings];
  'restart-onboarding': [];
  'show-about': [];
  'check-update': [];
  'show-privacy': [];
  'show-ai-suggestions': [];
}>();

const settings = ref<AppSettings>({
  defaultTargetDisk: '',
  largeFileThreshold: 100,
  createSymlink: true,
  defaultDeleteMode: 'recycle',
});

const showRestartConfirm = ref(false);
const showDisableAdminConfirm = ref(false);
const showClearCacheConfirm = ref(false);
const showDeleteCacheConfirm = ref(false);
const showAbout = ref(false);
const deletingCacheEntry = ref<CacheEntry | null>(null);
const isElevated = ref(false);
const isCheckingElevation = ref(true);
const cacheInfo = ref<CacheInfo | null>(null);
const loadingCache = ref(false);
type SettingsTab = 'basic' | 'scan' | 'clean' | 'privacy' | 'ai' | 'cache' | 'about';
const activeTab = ref<SettingsTab>('basic');

const settingsGroups: Array<{ key: SettingsTab; label: string; lead: string }> = [
  { key: 'basic', label: '基本', lead: '主题、关闭行为、自动更新这些"用一次设一次"的偏好。' },
  { key: 'scan', label: '扫描', lead: '后台扫描的触发条件，跳过路径会在 v0.2 加入。' },
  { key: 'clean', label: '清理', lead: '默认目标盘、删除模式、迁移时是否留链接。' },
  { key: 'privacy', label: '隐私与权限', lead: '管理员模式、本地日志、谁能看到你的数据。' },
  { key: 'ai', label: 'AI 服务', lead: '默认全部关闭。要勾选才会发数据，发什么都能预览。' },
  { key: 'cache', label: '缓存', lead: '扫描结果数据库。删除后下次扫描会重建。' },
  { key: 'about', label: '关于', lead: '版本、引导、诊断包。' },
];
const exportingDiagnostics = ref(false);

const ai = aiStore.state;
const showConsentModal = ref(false);
const consentAgreed = ref(false);
const showPreviewModal = ref(false);
const previewSnapshot = ref<AiSnapshot | null>(null);
const previewLoading = ref(false);
const aiBuiltinModels = BUILTIN_MODELS;

const aiCategoryItems = [
  { key: 'disks', label: '磁盘容量与剩余空间', detail: '每个盘符的容量与可用空间（MB）' },
  { key: 'largeFiles', label: '大文件列表（已脱敏）', detail: '仅大小、扩展名以及脱敏 token' },
  { key: 'categories', label: '文件类别统计', detail: '按类别聚合的总大小与数量' },
  { key: 'duplicates', label: '重复文件', detail: '重复组的总大小与数量' },
] as const;

const allCategoriesOn = computed(() =>
  ai.settings.categories.disks
  && ai.settings.categories.largeFiles
  && ai.settings.categories.categories
  && ai.settings.categories.duplicates,
);

const byokValid = computed(() =>
  ai.settings.byok.baseUrl.trim().length > 0
  && ai.settings.byok.apiKey.trim().length > 0
  && ai.settings.byok.model.trim().length > 0,
);

const aiHasConsent = computed(() => ai.settings.consentVersion >= CURRENT_CONSENT_VERSION);

const previewSnapshotJson = computed(() =>
  previewSnapshot.value ? JSON.stringify(previewSnapshot.value, null, 2) : '',
);

onMounted(() => {
  loadSettings();
  checkElevation();
  loadCacheInfo();
  void aiStore.pullFromBackend();
});

watch(
  () => props.show,
  (show) => {
    if (!show) {
      return;
    }

    loadSettings();
    checkElevation();
    loadCacheInfo();
    void aiStore.pullFromBackend();
  },
);

watch(activeTab, (tab) => {
  if (tab === 'cache') {
    loadCacheInfo();
  }
});

async function checkElevation() {
  isCheckingElevation.value = true;
  try {
    isElevated.value = await invoke<boolean>('is_elevated');
  } catch {
    showToast('权限检测失败', '无法检测当前权限状态', 'warning');
  } finally {
    isCheckingElevation.value = false;
  }
}

function loadSettings() {
  settings.value = getSettings();
}

function saveSettings() {
  // Persist current theme alongside the rest of the settings so 保存设置 doesn't
  // accidentally reset it (normalizeSettings used to default missing theme to 'light').
  persistSettings({ ...settings.value, theme: themeMode.value });
  settings.value = getSettings();
  // Track A：把后台扫描/托盘字段同步到 Rust 端的 settings.json
  void syncTrackASettingsToBackend(settings.value);
  emit('save', settings.value);
  emit('close');
}

function handleAdminToggle(event: Event) {
  const target = event.target as HTMLInputElement;
  const wantsElevated = target.checked;

  if (wantsElevated && !isElevated.value) {
    showRestartConfirm.value = true;
  } else if (!wantsElevated && isElevated.value) {
    showDisableAdminConfirm.value = true;
  }

  target.checked = isElevated.value;
}

async function confirmRestartAsAdmin() {
  try {
    // 先把当前选中的盘 / 默认目标盘当作意图写入 pending_scan.json，
    // UAC 重启后新进程会通过 consume_pending_scan_intent 续扫。
    const disk = settings.value.defaultTargetDisk || 'C:\\';
    await invoke('request_admin_rescan', { disk });
  } catch {
    showToast('重启失败', '请手动以管理员身份运行应用', 'error');
  }
  showRestartConfirm.value = false;
}

async function confirmRestartAsStandard() {
  showDisableAdminConfirm.value = false;
  try {
    await invoke('exit_app');
  } catch {
    window.close();
  }
}

function close() {
  emit('close');
}

function restartOnboarding() {
  localStorage.removeItem('cdrive-cleaner-onboarding-completed');
  localStorage.removeItem('cdrive-cleaner-welcome-shown');
  emit('restart-onboarding');
  emit('close');
}

async function loadCacheInfo() {
  loadingCache.value = true;
  try {
    cacheInfo.value = await invoke<CacheInfo>('get_cache_info');
  } catch {
    showToast('缓存加载失败', '无法读取缓存信息', 'error');
  } finally {
    loadingCache.value = false;
  }
}

async function clearAllCache() {
  try {
    await invoke('clear_scan_cache');
    showClearCacheConfirm.value = false;
    await loadCacheInfo();
    showToast('缓存已清空', '所有扫描缓存已成功删除', 'success');
  } catch {
    showToast('清理缓存失败', '无法清空缓存数据', 'error');
  }
}

async function deleteCacheEntry(entry: CacheEntry) {
  try {
    await invoke('delete_cache_entry', {
      diskPath: entry.disk_path,
      scanType: entry.scan_type
    });
    await loadCacheInfo();
    showDeleteCacheConfirm.value = false;
    deletingCacheEntry.value = null;
  } catch {
    showToast('删除缓存失败', '无法删除此缓存条目', 'error');
  }
}

async function deleteSelectedCacheEntry() {
  if (!deletingCacheEntry.value) {
    showDeleteCacheConfirm.value = false;
    return;
  }

  await deleteCacheEntry(deletingCacheEntry.value);
}

function confirmDeleteCache(entry: CacheEntry) {
  deletingCacheEntry.value = entry;
  showDeleteCacheConfirm.value = true;
}

function getScanTypeLabel(scanType: string): string {
  return scanType === 'deep' ? '深度扫描' : scanType;
}

function onDeleteModeToggle(event: Event) {
  const target = event.target as HTMLInputElement;
  settings.value.defaultDeleteMode = target.checked ? 'permanent' : 'recycle';
}

async function onAiEnableToggle(event: Event) {
  const target = event.target as HTMLInputElement;
  const wantsEnable = target.checked;

  if (wantsEnable && !aiHasConsent.value) {
    // Block the flip until the user accepts the privacy policy.
    target.checked = false;
    consentAgreed.value = false;
    showConsentModal.value = true;
    return;
  }

  await aiStore.patchSettings({ enabled: wantsEnable });
}

function setAiMode(mode: 'builtin' | 'byok') {
  void aiStore.patchSettings({ mode });
}

function setBuiltinModel(model: string) {
  void aiStore.patchSettings({ builtinModel: model });
}

function onBuiltinModelChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  setBuiltinModel(target.value);
}

function onByokBaseUrlInput(event: Event) {
  const target = event.target as HTMLInputElement;
  void aiStore.patchByok({ baseUrl: target.value });
}

function onByokApiKeyInput(event: Event) {
  const target = event.target as HTMLInputElement;
  void aiStore.patchByok({ apiKey: target.value });
}

function onByokModelInput(event: Event) {
  const target = event.target as HTMLInputElement;
  void aiStore.patchByok({ model: target.value });
}

function toggleAiCategory(key: keyof typeof ai.settings.categories) {
  void aiStore.patchCategories({ [key]: !ai.settings.categories[key] } as Partial<typeof ai.settings.categories>);
}

function setAllAiCategories(value: boolean) {
  void aiStore.patchCategories({
    disks: value,
    largeFiles: value,
    categories: value,
    duplicates: value,
  });
}

async function openPreviewModal() {
  if (!ai.settings.enabled) {
    showToast('请先启用 AI 分析', '在「启用 AI 分析」开关打开后才能预览', 'warning');
    return;
  }
  previewLoading.value = true;
  previewSnapshot.value = null;
  showPreviewModal.value = true;
  try {
    previewSnapshot.value = await aiStore.previewSnapshot();
  } catch (err) {
    showToast('预览失败', String(err), 'error');
    showPreviewModal.value = false;
  } finally {
    previewLoading.value = false;
  }
}

function closePreviewModal() {
  showPreviewModal.value = false;
  previewSnapshot.value = null;
}

async function copyPreviewSnapshot() {
  try {
    await navigator.clipboard.writeText(previewSnapshotJson.value);
    showToast('已复制', '快照 JSON 已复制到剪贴板', 'success');
  } catch {
    showToast('复制失败', '请手动选中后复制', 'error');
  }
}

async function acceptConsent() {
  if (!consentAgreed.value) {
    showToast('请先勾选同意', '只有勾选「我已阅读并同意」才能启用 AI', 'warning');
    return;
  }
  await aiStore.setConsented();
  await aiStore.patchSettings({ enabled: true });
  showConsentModal.value = false;
}

function rejectConsent() {
  consentAgreed.value = false;
  showConsentModal.value = false;
}

function openPrivacyFromSettings() {
  emit('show-privacy');
}

function openAiSuggestionsFromSettings() {
  emit('show-ai-suggestions');
  emit('close');
}

async function exportDiagnostics() {
  if (exportingDiagnostics.value) return;
  exportingDiagnostics.value = true;
  try {
    const path = await invoke<string>('export_diagnostic_bundle');
    await invoke('reveal_in_explorer', { path });
    showToast('诊断包已导出', path, 'success');
  } catch (err) {
    showToast('诊断包导出失败', String(err), 'error');
  } finally {
    exportingDiagnostics.value = false;
  }
}
</script>

<template>
  <div v-if="show" class="overlay" @click.self="close">
    <div class="settings-panel" @click.stop>
      <div class="panel-header">
        <h2>设置</h2>
        <button class="close-btn" @click="close">
          <IconClose :size="20" />
        </button>
      </div>

      <div class="settings-split">
      <nav class="settings-sidenav" role="tablist" aria-label="设置分组">
        <button
          v-for="group in settingsGroups"
          :key="group.key"
          type="button"
          class="settings-sidenav-item"
          :class="{ active: activeTab === group.key }"
          role="tab"
          :aria-selected="activeTab === group.key"
          @click="activeTab = group.key"
        >
          <strong>{{ group.label }}</strong>
          <small>{{ group.lead }}</small>
        </button>
      </nav>

      <div class="panel-body">
        <div class="tab-content settings-active-tab">
        <header v-if="settingsGroups.find((g) => g.key === activeTab)" class="settings-section-lead">
          <h3>{{ settingsGroups.find((g) => g.key === activeTab)?.label }}</h3>
          <p>{{ settingsGroups.find((g) => g.key === activeTab)?.lead }}</p>
        </header>

        <div v-show="activeTab === 'basic'" class="setting-section">
          <div class="section-header">
            <h3>外观</h3>
            <p>设置应用的颜色主题</p>
          </div>

          <div class="setting-item theme-setting">
            <div class="setting-label">
              <label>主题模式</label>
              <span class="setting-description">选择亮色、暗色或跟随系统</span>
            </div>
            <div class="theme-options">
              <button
                :class="['theme-btn', { active: themeMode === 'light' }]"
                @click="setTheme('light')"
              >亮色</button>
              <button
                :class="['theme-btn', { active: themeMode === 'dark' }]"
                @click="setTheme('dark')"
              >暗色</button>
              <button
                :class="['theme-btn', { active: themeMode === 'auto' }]"
                @click="setTheme('auto')"
              >跟随系统</button>
            </div>
          </div>
        </div>

        <div v-show="activeTab === 'clean'" class="setting-section">
          <div class="section-header">
            <h3>迁移设置</h3>
            <p>把文件搬到其他盘时的默认动作，链接保持下来软件就不用重装。</p>
          </div>

          <div class="setting-item setting-item--stacked setting-item--disk-target">
            <div class="setting-label">
              <label for="target-disk">默认目标磁盘</label>
              <span class="setting-description">迁移文件时的默认目标位置</span>
            </div>
            <DiskSelect
              id="target-disk"
              v-model="settings.defaultTargetDisk"
              class="setting-disk-select setting-disk-select--full"
              :disks="availableDisks"
              placeholder="每次选择"
              empty-label="每次选择"
              include-empty
            />
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label for="threshold">大文件阈值</label>
              <span class="setting-description">超过此大小的文件将在"大文件"视图中显示</span>
            </div>
            <div class="threshold-input-group">
              <input 
                id="threshold" 
                type="number" 
                v-model.number="settings.largeFileThreshold" 
                min="1" 
                max="10000"
                class="setting-input"
                @click.stop
              />
              <span class="input-suffix">MB</span>
            </div>
          </div>

          <div class="setting-item symlink-setting">
            <div class="setting-label">
              <div class="label-with-icon">
                <IconLink :size="20" />
                <label for="symlink">创建符号链接</label>
              </div>
              <span class="setting-description">迁移后在原位置创建符号链接，保持路径可访问</span>
            </div>
            <label class="toggle-switch">
              <input 
                id="symlink"
                type="checkbox" 
                v-model="settings.createSymlink"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div v-if="settings.createSymlink" class="info-card info-card-enabled">
            <div class="info-icon">
              <IconInfo :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">符号链接的作用</div>
              <ul class="info-list">
                <li>保持原路径可访问，应用程序无需修改配置</li>
                <li>透明重定向到新位置，用户体验无感知</li>
                <li>支持回滚操作，可随时恢复原状</li>
              </ul>
            </div>
          </div>

          <div v-if="!settings.createSymlink" class="info-card info-card-warning">
            <div class="info-icon warning">
              <IconWarning :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">不创建符号链接的影响</div>
              <ul class="info-list">
                <li>文件将被完全移动，原路径将不再存在</li>
                <li>依赖此路径的程序可能无法正常运行</li>
                <li>需要手动更新应用程序的配置路径</li>
              </ul>
            </div>
          </div>
        </div>

        <div v-show="activeTab === 'clean'" class="setting-section">
          <div class="section-header">
            <h3>清理设置</h3>
            <p>删除推荐项目时是走回收站还是直接抹掉。</p>
          </div>

          <div class="setting-item delete-mode-setting">
            <div class="setting-label">
              <div class="label-with-icon">
                <IconWarning :size="20" />
                <label for="delete-mode">默认永久删除（跳过回收站）</label>
              </div>
              <span class="setting-description">开启后清理操作将直接删除文件，无法通过回收站恢复</span>
            </div>
            <label class="toggle-switch">
              <input
                id="delete-mode"
                type="checkbox"
                :checked="settings.defaultDeleteMode === 'permanent'"
                @change="onDeleteModeToggle"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div v-if="settings.defaultDeleteMode === 'permanent'" class="info-card info-card-danger">
            <div class="info-icon danger">
              <IconWarning :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">永久删除已开启</div>
              <ul class="info-list">
                <li>清理推荐项时将直接调用文件系统删除，跳过回收站</li>
                <li>删除后无法回滚或恢复，请谨慎勾选</li>
                <li>每次清理仍会弹出红色警告确认对话框</li>
              </ul>
            </div>
          </div>

          <div v-else class="info-card info-card-enabled">
            <div class="info-icon">
              <IconInfo :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">默认走回收站</div>
              <ul class="info-list">
                <li>清理推荐项时会移入 Windows 回收站</li>
                <li>误删后可从回收站还原</li>
                <li>只有开启永久删除才会绕过回收站</li>
              </ul>
            </div>
          </div>
        </div>

        <div v-show="activeTab === 'privacy'" class="setting-section admin-section">
          <div class="section-header">
            <h3>权限</h3>
            <p>管理员权限可以访问系统保护的位置（影子副本、WinSxS、Windows.old 等）；不开也能用，只是少几个功能。</p>
          </div>

          <div v-if="!isCheckingElevation" class="setting-item admin-setting">
            <div class="setting-label">
              <div class="label-with-status">
                <label for="admin-mode">管理员模式</label>
                <div :class="['status-badge', isElevated ? 'elevated' : 'standard']">
                  {{ isElevated ? '已启用' : '未启用' }}
                </div>
              </div>
              <span class="setting-description">
                {{ isElevated ? '当前以管理员权限运行' : '当前以标准权限运行' }}
              </span>
            </div>
            <label class="toggle-switch">
              <input 
                id="admin-mode"
                type="checkbox" 
                :checked="isElevated"
                @change="handleAdminToggle"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div v-if="!isCheckingElevation && !isElevated" class="info-card info-card-warning">
            <div class="info-icon warning">
              <IconWarning :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">启用管理员模式需要重启</div>
              <ul class="info-list">
                <li>应用将关闭并以管理员权限重新启动</li>
                <li>可以访问系统保护的文件和文件夹</li>
                <li>执行需要提升权限的操作</li>
              </ul>
            </div>
          </div>

          <div v-if="!isCheckingElevation && isElevated" class="info-card info-card-enabled">
            <div class="info-icon">
              <IconInfo :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">管理员模式已启用</div>
              <ul class="info-list">
                <li>可以访问所有系统文件和文件夹</li>
                <li>可以执行需要提升权限的操作</li>
                <li>关闭此模式需要重启应用</li>
              </ul>
            </div>
          </div>
        </div>

        <div v-show="activeTab === 'basic'" class="setting-section">
          <div class="section-header">
            <h3>更新</h3>
            <p>是否自动联 GitHub 检查新版本。</p>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label for="auto-update">启动时自动检查更新</label>
              <span class="setting-description">每次打开应用时自动检测是否有新版本</span>
            </div>
            <label class="toggle-switch">
              <input
                id="auto-update"
                type="checkbox"
                v-model="settings.autoCheckUpdate"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div class="setting-item setting-item--row">
            <div class="setting-label">
              <label>下载加速镜像</label>
              <span class="setting-description">
                选择国内镜像加速更新下载（不影响版本检查，仅加速安装包下载）
              </span>
            </div>
            <div class="mirror-controls">
              <select v-model="settings.downloadMirror" class="mirror-select">
                <option value="none">直连 GitHub（默认）</option>
                <option value="ghproxy">ghproxy.com 镜像</option>
                <option value="custom">自定义镜像地址</option>
              </select>
              <input
                v-if="settings.downloadMirror === 'custom'"
                v-model="settings.downloadMirrorUrl"
                type="text"
                class="mirror-input"
                placeholder="https://your-mirror.example.com/"
              />
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label>手动检查</label>
              <span class="setting-description">立即连接 GitHub 查看是否有新版本</span>
            </div>
            <button class="btn btn-secondary btn-sm" @click="emit('check-update'); emit('close')">检查更新</button>
          </div>
        </div>

        <!-- === Track A: 关闭行为 + 后台扫描 === -->
        <div v-show="activeTab === 'basic'" class="setting-section">
          <div class="section-header">
            <h3>关闭主窗口时</h3>
            <p>选择点击窗口右上角关闭按钮时的行为</p>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label>关闭按钮的行为</label>
              <span class="setting-description">
                选「最小化到托盘」后，应用会保留在系统托盘中，便于后台扫描继续工作。
              </span>
            </div>
            <div class="theme-options">
              <button
                :class="['theme-btn', { active: settings.closeBehavior !== 'tray' }]"
                @click="settings.closeBehavior = 'exit'"
              >退出应用</button>
              <button
                :class="['theme-btn', { active: settings.closeBehavior === 'tray' }]"
                @click="settings.closeBehavior = 'tray'"
              >最小化到托盘</button>
            </div>
          </div>
        </div>

        <div v-show="activeTab === 'scan'" class="setting-section">
          <div class="section-header">
            <h3>后台扫描</h3>
            <p>在用户空闲且接通电源时自动跑增量/全量扫描</p>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label for="scheduler-enabled">启用后台扫描</label>
              <span class="setting-description">
                每 4 小时尝试一次增量扫描、每 7 天尝试一次全量扫描；电池模式下不触发。
              </span>
            </div>
            <label class="toggle-switch">
              <input
                id="scheduler-enabled"
                type="checkbox"
                v-model="settings.schedulerEnabled"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div class="setting-item" v-if="settings.schedulerEnabled">
            <div class="setting-label">
              <label for="scheduler-idle">用户 idle 阈值</label>
              <span class="setting-description">键鼠静止超过此分钟数后才允许后台扫描启动</span>
            </div>
            <div class="threshold-input-group">
              <input
                id="scheduler-idle"
                type="number"
                v-model.number="settings.schedulerIdleMinutes"
                min="1"
                max="240"
                class="setting-input"
                @click.stop
              />
              <span class="input-suffix">分钟</span>
            </div>
          </div>
        </div>
        <!-- === /Track A === -->

        <div v-show="activeTab === 'about'" class="setting-section">
          <div class="section-header">
            <h3>其他</h3>
            <p>引导与关于信息</p>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label>重新引导</label>
              <span class="setting-description">再次显示新手引导流程</span>
            </div>
            <button class="btn btn-secondary btn-sm" @click="restartOnboarding">重新引导</button>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label>诊断日志</label>
              <span class="setting-description">导出本地日志和环境信息，便于反馈问题</span>
            </div>
            <button class="btn btn-secondary btn-sm" :disabled="exportingDiagnostics" @click="exportDiagnostics">
              {{ exportingDiagnostics ? '导出中' : '导出诊断包' }}
            </button>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label>关于</label>
              <span class="setting-description">查看应用信息</span>
            </div>
            <button class="btn btn-secondary btn-sm" @click="emit('show-about'); emit('close')">关于</button>
          </div>
        </div>
        </div>

        <div v-show="activeTab === 'cache'" class="tab-content">
        <div class="setting-section">
          <div class="section-header">
            <h3>缓存管理</h3>
            <p>查看和管理扫描缓存数据</p>
          </div>

          <div v-if="loadingCache" class="cache-loading">
            <div class="loading-spinner"></div>
            <span>加载缓存信息...</span>
          </div>

          <div v-else-if="cacheInfo" class="cache-info-container">
            <div class="cache-summary">
              <div class="cache-stat">
                <div class="stat-label">缓存位置</div>
                <div class="stat-value path">{{ cacheInfo.cache_path }}</div>
              </div>
              <div class="cache-stat">
                <div class="stat-label">数据库大小</div>
                <div class="stat-value">{{ formatBytes(cacheInfo.total_size) }}</div>
              </div>
              <div class="cache-stat">
                <div class="stat-label">缓存数量</div>
                <div class="stat-value">{{ cacheInfo.caches.length }} 个</div>
              </div>
            </div>

            <div v-if="cacheInfo.caches.length > 0" class="cache-list">
              <div v-for="cache in cacheInfo.caches" :key="`${cache.disk_path}-${cache.scan_type}`" class="cache-item">
                <div class="cache-item-header">
                  <div class="cache-disk">{{ cache.disk_path }}</div>
                  <div class="cache-type-badge cache-type-badge--deep">
                    {{ getScanTypeLabel(cache.scan_type) }}
                  </div>
                </div>
                <div class="cache-item-details">
                  <div class="cache-detail">
                    <span class="detail-label">文件数:</span>
                    <span class="detail-value">{{ cache.file_count.toLocaleString() }}</span>
                  </div>
                  <div class="cache-detail">
                    <span class="detail-label">磁盘大小:</span>
                    <span class="detail-value">{{ formatBytes(cache.total_size) }}</span>
                  </div>
                  <div class="cache-detail">
                    <span class="detail-label">数据大小:</span>
                    <span class="detail-value">{{ formatBytes(cache.cache_size) }}</span>
                  </div>
                  <div class="cache-detail">
                    <span class="detail-label">扫描时间:</span>
                    <span class="detail-value">{{ formatDate(cache.created_at) }}</span>
                  </div>
                </div>
                <button @click.stop="confirmDeleteCache(cache)" class="delete-cache-btn">
                  删除
                </button>
              </div>
            </div>

            <div v-else class="no-cache">
              <IconInfo :size="24" />
              <p>暂无缓存数据</p>
            </div>

            <button
              v-if="cacheInfo.caches.length > 0"
              @click.stop="showClearCacheConfirm = true"
              class="clear-all-cache-btn"
            >
              <IconRefresh :size="16" />
              清空所有缓存
            </button>
          </div>
        </div>
        </div>

        <div v-show="activeTab === 'ai'" class="tab-content">
          <div class="setting-section">
            <div class="section-header">
              <h3>AI 服务</h3>
              <p>所有 AI 数据上送默认关闭，需要你逐项勾选才能加入快照。</p>
            </div>

            <div class="setting-item">
              <div class="setting-label">
                <div class="label-with-icon">
                  <IconShield :size="20" />
                  <label for="ai-enable">启用 AI 分析</label>
                </div>
                <span class="setting-description">关闭时不会向任何 AI 服务发送数据</span>
              </div>
              <label class="toggle-switch">
                <input
                  id="ai-enable"
                  type="checkbox"
                  :checked="ai.settings.enabled"
                  @change="onAiEnableToggle"
                  @click.stop
                />
                <span class="toggle-slider"></span>
              </label>
            </div>

            <div v-if="!aiHasConsent" class="info-card info-card-warning">
              <div class="info-icon warning">
                <IconWarning :size="20" />
              </div>
              <div class="info-content">
                <div class="info-title">首次启用需要同意隐私政策</div>
                <ul class="info-list">
                  <li>点击「启用 AI 分析」开关后，会弹出同意框</li>
                  <li>勾选「我已阅读并同意」后才能继续</li>
                  <li>
                    可先查看 <button class="inline-link" @click="openPrivacyFromSettings">完整隐私政策</button>
                  </li>
                </ul>
              </div>
            </div>
          </div>

          <div class="setting-section">
            <div class="section-header">
              <h3>调用模式</h3>
              <p>Builtin 走我们代理（每日 50 次免费）；BYOK 直连你自己的兼容服务</p>
            </div>

            <div class="ai-mode-row">
              <button
                type="button"
                class="ai-mode-card"
                :class="{ active: ai.settings.mode === 'builtin' }"
                :disabled="!ai.settings.enabled"
                @click="setAiMode('builtin')"
              >
                <strong>Builtin</strong>
                <span>免费 · 每天 50 次 · 模型由我们代理</span>
              </button>
              <button
                type="button"
                class="ai-mode-card"
                :class="{ active: ai.settings.mode === 'byok' }"
                :disabled="!ai.settings.enabled"
                @click="setAiMode('byok')"
              >
                <strong>BYOK</strong>
                <span>自带 OpenAI 兼容服务的 API Key</span>
              </button>
            </div>

            <div v-if="ai.settings.mode === 'builtin'" class="setting-item">
              <div class="setting-label">
                <label for="ai-builtin-model">模型</label>
                <span class="setting-description">默认 deepseek-v4-pro（比 glm-5.1 便宜约 3 倍）</span>
              </div>
              <select
                id="ai-builtin-model"
                class="setting-input ai-select"
                :value="ai.settings.builtinModel"
                :disabled="!ai.settings.enabled"
                @change="onBuiltinModelChange"
                @click.stop
              >
                <option v-for="model in aiBuiltinModels" :key="model" :value="model">{{ model }}</option>
              </select>
            </div>

            <template v-if="ai.settings.mode === 'byok'">
              <div class="setting-item">
                <div class="setting-label">
                  <label for="ai-byok-base">Base URL</label>
                  <span class="setting-description">OpenAI 兼容服务的 chat completions endpoint 根地址</span>
                </div>
                <input
                  id="ai-byok-base"
                  type="text"
                  class="setting-input ai-text-input"
                  placeholder="https://api.openai.com/v1"
                  :value="ai.settings.byok.baseUrl"
                  :disabled="!ai.settings.enabled"
                  @input="onByokBaseUrlInput"
                  @click.stop
                />
              </div>

              <div class="setting-item">
                <div class="setting-label">
                  <label for="ai-byok-key">API Key</label>
                  <span class="setting-description">仅保存在本机，不会上传或被我们代理拿到</span>
                </div>
                <input
                  id="ai-byok-key"
                  type="password"
                  class="setting-input ai-text-input"
                  placeholder="sk-..."
                  autocomplete="off"
                  :value="ai.settings.byok.apiKey"
                  :disabled="!ai.settings.enabled"
                  @input="onByokApiKeyInput"
                  @click.stop
                />
              </div>

              <div class="setting-item">
                <div class="setting-label">
                  <label for="ai-byok-model">Model</label>
                  <span class="setting-description">服务商对外暴露的模型名</span>
                </div>
                <input
                  id="ai-byok-model"
                  type="text"
                  class="setting-input ai-text-input"
                  placeholder="gpt-4o-mini"
                  :value="ai.settings.byok.model"
                  :disabled="!ai.settings.enabled"
                  @input="onByokModelInput"
                  @click.stop
                />
              </div>

              <div v-if="ai.settings.enabled && !byokValid" class="info-card info-card-warning">
                <div class="info-icon warning">
                  <IconWarning :size="20" />
                </div>
                <div class="info-content">
                  <div class="info-title">BYOK 配置不完整</div>
                  <ul class="info-list">
                    <li>Base URL、API Key、Model 三个字段都必须填写</li>
                    <li>未填齐时 AI 分析无法发起请求</li>
                  </ul>
                </div>
              </div>
            </template>
          </div>

          <div class="setting-section">
            <div class="section-header">
              <h3>数据类别</h3>
              <p>勾选后，对应类别的脱敏数据才会出现在发送给 AI 的快照中。</p>
            </div>

            <div class="ai-category-toolbar">
              <button
                class="btn btn-secondary btn-sm"
                :disabled="!ai.settings.enabled || allCategoriesOn"
                @click="setAllAiCategories(true)"
              >全选</button>
              <button
                class="btn btn-secondary btn-sm"
                :disabled="!ai.settings.enabled"
                @click="setAllAiCategories(false)"
              >全不选</button>
            </div>

            <div
              v-for="item in aiCategoryItems"
              :key="item.key"
              class="setting-item"
            >
              <div class="setting-label">
                <label :for="`ai-cat-${item.key}`">{{ item.label }}</label>
                <span class="setting-description">{{ item.detail }}</span>
              </div>
              <label class="toggle-switch">
                <input
                  :id="`ai-cat-${item.key}`"
                  type="checkbox"
                  :checked="ai.settings.categories[item.key]"
                  :disabled="!ai.settings.enabled"
                  @change="toggleAiCategory(item.key)"
                  @click.stop
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>

          <div class="setting-section">
            <div class="section-header">
              <h3>发送前预览</h3>
              <p>查看下一次请求会发出的完整 JSON 快照，没勾选的类别绝不会出现。</p>
            </div>

            <div class="setting-item">
              <div class="setting-label">
                <label>预览快照</label>
                <span class="setting-description">点击后弹出格式化 JSON 视图</span>
              </div>
              <button
                class="btn btn-secondary btn-sm"
                :disabled="!ai.settings.enabled"
                @click="openPreviewModal"
              >发送前预览</button>
            </div>

            <div class="setting-item">
              <div class="setting-label">
                <label>AI 建议面板</label>
                <span class="setting-description">查看已经返回的 AI 建议（采纳前不会执行任何动作）</span>
              </div>
              <button class="btn btn-secondary btn-sm" @click="openAiSuggestionsFromSettings">打开建议面板</button>
            </div>

            <div class="setting-item">
              <div class="setting-label">
                <label>隐私政策</label>
                <span class="setting-description">完整版本：数据范围、保留策略、撤回权利</span>
              </div>
              <button class="btn btn-secondary btn-sm" @click="openPrivacyFromSettings">查看隐私政策</button>
            </div>
          </div>
        </div>

      </div>
      </div>

      <div class="panel-footer">
        <button class="btn btn-secondary" @click="close">取消</button>
        <button class="btn btn-primary" @click="saveSettings">保存设置</button>
      </div>
    </div>

    <ConfirmDialog
      :show="showRestartConfirm"
      title="以管理员身份重启"
      message="应用将关闭并以管理员权限重新启动。请在弹出的 UAC 提示中点击「是」。"
      confirm-text="立即重启"
      cancel-text="稍后手动重启"
      type="warning"
      @confirm="confirmRestartAsAdmin"
      @cancel="showRestartConfirm = false"
    />

    <ConfirmDialog
      :show="showDisableAdminConfirm"
      title="关闭管理员模式"
      message="应用将关闭，请手动以标准权限重新打开应用。"
      confirm-text="立即关闭"
      cancel-text="取消"
      type="info"
      @confirm="confirmRestartAsStandard"
      @cancel="showDisableAdminConfirm = false"
    />

    <ConfirmDialog
      :show="showClearCacheConfirm"
      title="确定要清空所有缓存吗？"
      message="此操作将删除所有扫描缓存，下次扫描将重新生成缓存数据"
      confirm-text="确定清空"
      cancel-text="取消"
      type="warning"
      @confirm="clearAllCache"
      @cancel="showClearCacheConfirm = false"
    />

    <ConfirmDialog
      :show="showDeleteCacheConfirm"
      :title="`删除 ${deletingCacheEntry?.disk_path} 的缓存？`"
      :message="`将删除此磁盘的${getScanTypeLabel(deletingCacheEntry?.scan_type || '')}缓存`"
      confirm-text="确定删除"
      cancel-text="取消"
      type="warning"
      @confirm="deleteSelectedCacheEntry"
      @cancel="showDeleteCacheConfirm = false; deletingCacheEntry = null"
    />

    <Teleport to="body">
      <!-- About dialog -->
      <div v-if="showAbout" class="about-overlay" @click.self="showAbout = false">
        <div class="about-panel" @click.stop>
          <button class="close-btn about-close" @click="showAbout = false">
            <IconClose :size="18" />
          </button>
          <img :src="appIcon" alt="应用图标" class="about-icon" />
          <h3 class="about-title">CDrive Cleaner</h3>
          <p class="about-version">v0.1.10</p>
          <p class="about-desc">
            一键扫描、智能搬运、安全回滚——为 C 盘瘦身的桌面工具。
          </p>

          <div class="about-section">
            <h4>联系方式</h4>
            <ul>
              <li>QQ 邮箱：2632507193@qq.com</li>
              <li>Gmail：bajianfeng302@gmail.com</li>
            </ul>
          </div>

          <div class="about-section">
            <h4>隐私政策</h4>
            <p>
              CDrive Cleaner 仅在本地运行，不收集、不上传任何用户数据。
              扫描结果和迁移记录均存储在本机，应用不包含任何遥测或分析功能。
              我们不会将您的文件信息发送到任何服务器。
            </p>
          </div>

          <p class="about-footer">© 2024–2026 CDrive Cleaner · GPL-3.0 License</p>
        </div>
      </div>

      <div v-if="showConsentModal" class="ai-modal-overlay" @click.self="rejectConsent">
        <div class="ai-modal" @click.stop>
          <header class="ai-modal-head">
            <div class="ai-modal-title">
              <IconShield :size="20" />
              <h3>启用 AI 分析前的同意</h3>
            </div>
            <button class="close-btn" @click="rejectConsent">
              <IconClose :size="18" />
            </button>
          </header>
          <div class="ai-modal-body">
            <p>启用 AI 分析后，<strong>且只有当你在「数据类别」面板里勾选某一项时</strong>，对应类别的脱敏数据才会被打包到请求中。</p>
            <ul>
              <li>Builtin 模式：请求经我们的 Cloudflare Worker 代理到上游模型服务商。</li>
              <li>BYOK 模式：请求直连你填写的服务，不经我们的代理。</li>
              <li>所有路径都会先在本地脱敏，绝不发送真实用户名或绝对路径。</li>
              <li>所有 AI 建议都需要你手动点「采纳」才会触发动作。</li>
            </ul>
            <p>
              完整说明请参见
              <button class="inline-link" @click="openPrivacyFromSettings">隐私政策</button>。
            </p>

            <label class="ai-consent-check">
              <input type="checkbox" v-model="consentAgreed" />
              <span>我已阅读并同意上述说明。</span>
            </label>
          </div>
          <footer class="ai-modal-footer">
            <button class="btn btn-secondary" @click="rejectConsent">取消</button>
            <button class="btn btn-primary" :disabled="!consentAgreed" @click="acceptConsent">同意并启用</button>
          </footer>
        </div>
      </div>

      <div v-if="showPreviewModal" class="ai-modal-overlay" @click.self="closePreviewModal">
        <div class="ai-modal ai-modal--wide" @click.stop>
          <header class="ai-modal-head">
            <div class="ai-modal-title">
              <IconInfo :size="20" />
              <h3>发送前预览</h3>
            </div>
            <button class="close-btn" @click="closePreviewModal">
              <IconClose :size="18" />
            </button>
          </header>
          <div class="ai-modal-body">
            <p class="ai-preview-hint">下面是即将随下一次请求发出的完整 JSON。没勾选的数据类别一定不会出现在这里。</p>
            <div v-if="previewLoading" class="ai-preview-loading">加载中…</div>
            <pre v-else class="ai-preview-json">{{ previewSnapshotJson || '{}' }}</pre>
          </div>
          <footer class="ai-modal-footer">
            <button class="btn btn-secondary" @click="copyPreviewSnapshot" :disabled="previewLoading || !previewSnapshot">复制 JSON</button>
            <button class="btn btn-primary" @click="closePreviewModal">已确认</button>
          </footer>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2300;
  animation: settingsOverlayIn 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  pointer-events: auto;
}

@keyframes settingsOverlayIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.settings-panel {
  width: 90%;
  max-width: 680px;
  max-height: 88vh;
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: 28px;
  box-shadow: 
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  animation: slideUp 0.5s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans);
  position: relative;
  z-index: 2001;
  pointer-events: auto;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(24px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex: 0 0 auto;
  padding: 2rem 2rem 1.5rem;
  border-bottom: 1px solid var(--color-border-light);
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.6) 0%, rgba(255, 252, 245, 0.3) 100%);
}

.tabs-container {
  display: flex;
  gap: 0.5rem;
  flex: 0 0 auto;
  padding: 0 2rem;
  background: linear-gradient(to bottom, rgba(255, 252, 245, 0.3) 0%, transparent 100%);
  border-bottom: 1px solid var(--color-border-light);
}

.tab-btn {
  flex: 1;
  padding: 1rem 1.5rem;
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-tertiary);
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: 0;
  position: relative;
}

.tab-btn:hover {
  color: var(--color-text-secondary);
  background: rgba(139, 92, 46, 0.04);
}

.tab-btn.active {
  color: var(--color-accent-primary);
  border-bottom-color: var(--color-accent-primary);
  background: rgba(139, 115, 85, 0.06);
}

.tab-btn.active::after {
  content: '';
  position: absolute;
  bottom: -1px;
  left: 0;
  right: 0;
  height: 2px;
  background: linear-gradient(90deg, var(--color-accent-primary), var(--color-accent-secondary));
  box-shadow: 0 0 8px rgba(139, 115, 85, 0.3);
}

.tab-content {
  min-height: 0;
  animation: settingsTabIn 0.3s ease-in-out;
}

@keyframes settingsTabIn {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.panel-header h2 {
  font-family: var(--font-serif);
  font-size: 1.75rem;
  font-weight: 600;
  color: var(--color-text-primary);
  letter-spacing: 0;
  margin: 0;
}

.close-btn {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(139, 92, 46, 0.06);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-base);
}

.close-btn:hover {
  background: rgba(139, 92, 46, 0.12);
  border-color: var(--color-border-medium);
  color: var(--color-text-primary);
  transform: scale(1.05);
}

.panel-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 2rem;
}

.panel-body::-webkit-scrollbar {
  width: 6px;
}

.panel-body::-webkit-scrollbar-track {
  background: transparent;
}

.panel-body::-webkit-scrollbar-thumb {
  background: rgba(139, 92, 46, 0.15);
  border-radius: 3px;
}

.setting-section {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
  margin-bottom: 2rem;
}

.section-header h3 {
  font-family: var(--font-serif);
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.375rem;
  letter-spacing: 0;
}

.section-header p {
  font-size: 0.9375rem;
  color: var(--color-text-tertiary);
  line-height: 1.6;
  margin: 0;
}

.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 2rem;
  padding: 1.25rem 1.5rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  transition: all var(--transition-base);
}

.setting-item--stacked {
  flex-direction: column;
  align-items: stretch;
  gap: 0.85rem;
}

.setting-item--row {
  flex-wrap: wrap;
  gap: 1rem;
}

.setting-item--disk-target .setting-label {
  min-width: 0;
}

.setting-item:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-medium);
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

.setting-label {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.setting-label label {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  cursor: pointer;
  letter-spacing: 0;
}

.setting-description {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  line-height: 1.5;
}

.setting-disk-select {
  width: min(100%, 320px);
  min-width: 260px;
  flex-shrink: 0;
}

.setting-disk-select--full {
  width: 100%;
  min-width: 0;
}

.threshold-input-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.setting-input {
  width: 120px;
  padding: 0.75rem 1rem;
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  background: rgba(255, 255, 255, 0.8);
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  transition: all var(--transition-base);
  box-shadow: var(--shadow-sm);
  text-align: center;
  pointer-events: auto;
  position: relative;
  z-index: 10;
}

.setting-input:hover {
  border-color: var(--color-border-strong);
  background: rgba(255, 255, 255, 0.95);
}

.setting-input:focus {
  outline: none;
  border-color: var(--color-accent-primary);
  box-shadow: 0 0 0 3px rgba(139, 115, 85, 0.08);
}

.mirror-controls {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  min-width: 220px;
  flex-shrink: 0;
}

.mirror-select {
  padding: 0.65rem 1rem;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--color-text-primary);
  background: rgba(255, 255, 255, 0.8);
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  transition: all var(--transition-base);
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  appearance: auto;
}

.mirror-select:hover {
  border-color: var(--color-border-strong);
}

.mirror-select:focus {
  outline: none;
  border-color: var(--color-accent-primary);
  box-shadow: 0 0 0 3px rgba(139, 115, 85, 0.08);
}

.mirror-input {
  padding: 0.65rem 0.85rem;
  font-size: 0.8125rem;
  color: var(--color-text-primary);
  background: rgba(255, 255, 255, 0.8);
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  transition: all var(--transition-base);
  box-shadow: var(--shadow-sm);
}

.mirror-input::placeholder {
  color: var(--color-text-tertiary);
  font-size: 0.8125rem;
}

.mirror-input:hover {
  border-color: var(--color-border-strong);
}

.mirror-input:focus {
  outline: none;
  border-color: var(--color-accent-primary);
  box-shadow: 0 0 0 3px rgba(139, 115, 85, 0.08);
}

[data-theme="dark"] .mirror-select,
[data-theme="dark"] .mirror-input {
  background: rgba(255, 255, 255, 0.06);
  color: var(--color-text-primary);
}

.input-suffix {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-tertiary);
}

.panel-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex: 0 0 auto;
  gap: 0.75rem;
  padding: 1.5rem 2rem;
  border-top: 1px solid var(--color-border-light);
  background: linear-gradient(to top, var(--color-bg-tertiary) 0%, rgba(255, 255, 255, 0.4) 100%);
}

.btn {
  padding: 0.75rem 1.75rem;
  font-size: 0.9375rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: 0;
}

.btn-secondary {
  color: var(--color-text-secondary);
  background: rgba(139, 92, 46, 0.06);
  border: 1px solid var(--color-border-light);
}

.btn-secondary:hover {
  background: rgba(139, 92, 46, 0.12);
  border-color: var(--color-border-medium);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.btn-primary {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  box-shadow: 
    0 0 0 1px rgba(139, 115, 85, 0.2),
    0 4px 16px rgba(139, 115, 85, 0.25);
}

.btn-primary:hover {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(139, 115, 85, 0.3),
    0 8px 24px rgba(139, 115, 85, 0.35);
}

.symlink-setting {
  background: rgba(139, 115, 85, 0.04);
  border-color: rgba(139, 115, 85, 0.12);
}

.symlink-setting:hover {
  background: rgba(139, 115, 85, 0.08);
  border-color: rgba(139, 115, 85, 0.2);
}

.delete-mode-setting {
  background: rgba(245, 158, 11, 0.04);
  border-color: rgba(245, 158, 11, 0.16);
}

.delete-mode-setting:hover {
  background: rgba(245, 158, 11, 0.08);
  border-color: rgba(245, 158, 11, 0.24);
}

.label-with-icon {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  color: var(--color-accent-primary);
}

.label-with-icon label {
  color: var(--color-text-primary);
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 52px;
  height: 30px;
  cursor: pointer;
}

.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle-slider {
  position: absolute;
  inset: 0;
  background: rgba(139, 92, 46, 0.15);
  border: 1px solid var(--color-border-medium);
  border-radius: 30px;
  transition: all 0.35s cubic-bezier(0.16, 1, 0.3, 1);
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.08);
}

.toggle-slider::before {
  content: '';
  position: absolute;
  height: 22px;
  width: 22px;
  left: 3px;
  top: 3px;
  background: linear-gradient(to bottom, #ffffff 0%, #fafafa 100%);
  border-radius: 50%;
  transition: all 0.35s cubic-bezier(0.16, 1, 0.3, 1);
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1), 0 1px 2px rgba(0, 0, 0, 0.06);
}

.toggle-switch input:checked + .toggle-slider {
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  border-color: rgba(139, 115, 85, 0.3);
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.1), 0 0 0 3px rgba(139, 115, 85, 0.08);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(22px);
  background: #ffffff;
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15), 0 1px 3px rgba(0, 0, 0, 0.1);
}

.toggle-switch:hover .toggle-slider {
  border-color: var(--color-border-strong);
}

.info-card {
  display: flex;
  gap: 1rem;
  padding: 1.25rem 1.5rem;
  border-radius: var(--radius-md);
  margin-top: 0.5rem;
  animation: slideDown 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  border: 1px solid;
  position: relative;
  overflow: hidden;
}

@keyframes slideDown {
  from {
    opacity: 0;
    transform: translateY(-8px);
    max-height: 0;
  }
  to {
    opacity: 1;
    transform: translateY(0);
    max-height: 300px;
  }
}

.info-card::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  width: 3px;
  height: 100%;
  transition: width var(--transition-base);
}

.info-card:hover::before {
  width: 4px;
}

.info-card-enabled {
  background: rgba(16, 185, 129, 0.04);
  border-color: rgba(16, 185, 129, 0.15);
}

.info-card-enabled::before {
  background: var(--color-success);
}

.info-card-warning {
  background: rgba(245, 158, 11, 0.04);
  border-color: rgba(245, 158, 11, 0.15);
}

.info-card-warning::before {
  background: var(--color-warning);
}

.info-card-danger {
  background: rgba(239, 68, 68, 0.05);
  border-color: rgba(239, 68, 68, 0.18);
}

.info-card-danger::before {
  background: var(--color-error);
}

.info-card-danger .info-list li::before {
  background: var(--color-error);
}

.info-icon {
  flex-shrink: 0;
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  background: rgba(16, 185, 129, 0.1);
  color: var(--color-success);
  border: 1px solid rgba(16, 185, 129, 0.15);
}

.info-icon.warning {
  background: rgba(245, 158, 11, 0.1);
  color: var(--color-warning);
  border-color: rgba(245, 158, 11, 0.15);
}

.info-icon.danger {
  background: rgba(239, 68, 68, 0.1);
  color: var(--color-error);
  border-color: rgba(239, 68, 68, 0.18);
}

.info-content {
  flex: 1;
}

.info-title {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.625rem;
  letter-spacing: 0;
}

.info-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.info-list li {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  line-height: 1.6;
  padding-left: 1rem;
  position: relative;
}

.info-list li::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0.5rem;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: currentColor;
  opacity: 0.5;
}

.info-card-enabled .info-list li::before {
  background: var(--color-success);
}

.info-card-warning .info-list li::before {
  background: var(--color-warning);
}

.admin-section {
  margin-top: 2rem;
}

.admin-setting {
  background: rgba(139, 115, 85, 0.04);
  border-color: rgba(139, 115, 85, 0.12);
}

.admin-setting:hover {
  background: rgba(139, 115, 85, 0.08);
  border-color: rgba(139, 115, 85, 0.2);
}

.label-with-status {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.label-with-status label {
  color: var(--color-text-primary);
}

.status-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.25rem 0.625rem;
  font-size: 0.75rem;
  font-weight: 600;
  border-radius: 12px;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.status-badge.elevated {
  color: var(--color-success);
  background: rgba(16, 185, 129, 0.1);
  border: 1px solid rgba(16, 185, 129, 0.2);
}

.status-badge.standard {
  color: var(--color-text-tertiary);
  background: rgba(139, 92, 46, 0.08);
  border: 1px solid rgba(139, 92, 46, 0.15);
}

@media (max-width: 768px) {
  .settings-panel {
    width: 95%;
    max-height: 90vh;
  }

  .setting-item {
    flex-direction: column;
    align-items: flex-start;
    gap: 1rem;
  }

  .setting-disk-select,
  .threshold-input-group {
    width: 100%;
  }

  .info-card {
    flex-direction: column;
    gap: 1rem;
  }
  
  .label-with-icon {
    flex-wrap: wrap;
  }
}

.cache-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 2rem;
  color: var(--color-text-tertiary);
  font-size: 0.9375rem;
}

.loading-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid rgba(139, 92, 46, 0.15);
  border-top-color: var(--color-accent-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.cache-info-container {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.cache-summary {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 1rem;
}

.cache-stat {
  padding: 1.25rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  transition: all var(--transition-base);
}

.cache-stat:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-medium);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.stat-label {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  margin-bottom: 0.5rem;
}

.stat-value {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--color-text-primary);
  word-break: break-all;
}

.stat-value.path {
  font-size: 0.875rem;
  font-family: 'Consolas', 'Monaco', monospace;
  color: var(--color-text-secondary);
}

.cache-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.cache-item {
  position: relative;
  padding: 1.25rem;
  padding-right: 5rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  transition: all var(--transition-base);
}

.cache-item:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-medium);
  box-shadow: var(--shadow-md);
}

.cache-item-header {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 0.875rem;
  flex-wrap: wrap;
}

.cache-disk {
  font-size: 1rem;
  font-weight: 600;
  color: var(--color-text-primary);
  font-family: 'Consolas', 'Monaco', monospace;
}

.cache-type-badge {
  padding: 0.25rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 600;
  border-radius: 12px;
  text-transform: uppercase;
  letter-spacing: 0.02em;
}

.cache-type-badge--deep {
  color: #8b5cf6;
  background: rgba(139, 92, 246, 0.1);
  border: 1px solid rgba(139, 92, 246, 0.2);
}

.cache-item-details {
  display: flex;
  gap: 1.5rem;
  margin-bottom: 0.875rem;
  flex-wrap: wrap;
}

.cache-detail {
  display: flex;
  gap: 0.5rem;
  font-size: 0.875rem;
}

.detail-label {
  color: var(--color-text-tertiary);
}

.detail-value {
  color: var(--color-text-primary);
  font-weight: 600;
}

.delete-cache-btn {
  position: absolute;
  top: 1.25rem;
  right: 1.25rem;
  padding: 0.5rem 1rem;
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--color-error);
  background: rgba(239, 68, 68, 0.06);
  border: 1px solid rgba(239, 68, 68, 0.15);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-base);
  opacity: 0;
  z-index: 10;
}

.cache-item:hover .delete-cache-btn {
  opacity: 1;
}

.delete-cache-btn:hover {
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.25);
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(239, 68, 68, 0.15);
}

.no-cache {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 3rem 2rem;
  color: var(--color-text-tertiary);
}

.no-cache p {
  margin: 0;
  font-size: 0.9375rem;
}

.clear-all-cache-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.875rem 1.375rem;
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-error);
  background: rgba(239, 68, 68, 0.06);
  border: 1px solid rgba(239, 68, 68, 0.15);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: 0;
}

.clear-all-cache-btn:hover {
  background: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.25);
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(239, 68, 68, 0.15);
}

.clear-all-cache-btn svg {
  transition: transform 0.5s cubic-bezier(0.16, 1, 0.3, 1);
}

.clear-all-cache-btn:hover svg {
  transform: rotate(180deg);
}

.btn-sm {
  padding: 0.5rem 1rem;
  font-size: 0.82rem;
}

.theme-setting {
  flex-direction: column;
  align-items: flex-start;
  gap: 1rem;
}

.theme-options {
  display: flex;
  gap: 0.5rem;
  width: 100%;
}

.theme-btn {
  flex: 1;
  padding: 0.65rem 1rem;
  border-radius: 10px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 0.875rem;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition-base);
}

.theme-btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-strong);
  color: var(--color-text-primary);
}

.theme-btn.active {
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  color: #ffffff;
  border-color: transparent;
  box-shadow: 0 4px 12px rgba(139, 115, 85, 0.25);
}

.about-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 32, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9300;
  animation: settingsOverlayIn 0.2s ease;
}

.about-panel {
  position: relative;
  width: min(400px, 90vw);
  max-height: 80vh;
  overflow-y: auto;
  padding: 2rem;
  border-radius: var(--radius-lg);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xl);
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
}

.about-close {
  position: absolute;
  top: 12px;
  right: 12px;
}

.about-icon {
  width: 56px;
  height: 56px;
  border-radius: 14px;
  box-shadow: var(--shadow-sm);
}

.about-title {
  font-family: var(--font-display);
  font-size: 1.3rem;
  font-weight: 700;
  color: var(--color-text-primary);
  margin-top: 0.4rem;
}

.about-version {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
}

.about-desc {
  font-size: 0.88rem;
  color: var(--color-text-secondary);
  line-height: 1.5;
  margin-top: 0.3rem;
}

.about-section {
  width: 100%;
  text-align: left;
  margin-top: 0.8rem;
  padding: 0.8rem 1rem;
  border-radius: var(--radius-sm);
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
}

.about-section h4 {
  font-size: 0.82rem;
  font-weight: 700;
  color: var(--color-text-primary);
  margin-bottom: 0.4rem;
}

.about-section p {
  font-size: 0.8rem;
  color: var(--color-text-secondary);
  line-height: 1.6;
}

.about-section ul {
  list-style: none;
  padding: 0;
  margin: 0;
}

.about-section ul li {
  font-size: 0.8rem;
  color: var(--color-text-secondary);
  line-height: 1.8;
}

.about-footer {
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
  margin-top: 1rem;
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .panel-header {
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.04) 0%, rgba(255, 255, 255, 0.02) 100%);
}

[data-theme="dark"] .tabs-container {
  background: linear-gradient(to bottom, rgba(255, 255, 255, 0.02) 0%, transparent 100%);
}

[data-theme="dark"] .tab-btn:hover {
  background: rgba(255, 255, 255, 0.04);
}

[data-theme="dark"] .tab-btn.active {
  background: rgba(94, 234, 212, 0.08);
}

[data-theme="dark"] .close-btn {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .close-btn:hover {
  background: rgba(255, 255, 255, 0.1);
}

[data-theme="dark"] .panel-body::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.12);
}

[data-theme="dark"] .setting-input {
  background: rgba(255, 255, 255, 0.04);
  color: var(--color-text-primary);
}

[data-theme="dark"] .setting-input:hover {
  background: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .panel-footer {
  background: linear-gradient(to top, rgba(255, 255, 255, 0.03) 0%, transparent 100%);
}

[data-theme="dark"] .btn-secondary {
  background: rgba(255, 255, 255, 0.06);
  border-color: var(--color-border-medium);
  color: var(--color-text-secondary);
}

[data-theme="dark"] .btn-secondary:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: var(--color-border-strong);
}

[data-theme="dark"] .btn-primary {
  color: var(--color-bg-primary);
  background: linear-gradient(135deg, var(--color-accent-secondary) 0%, var(--color-highlight) 100%);
  box-shadow:
    0 0 0 1px rgba(94, 234, 212, 0.25),
    0 4px 16px rgba(20, 184, 166, 0.25);
}

[data-theme="dark"] .btn-primary:hover {
  box-shadow:
    0 0 0 1px rgba(94, 234, 212, 0.35),
    0 8px 24px rgba(20, 184, 166, 0.35);
}

[data-theme="dark"] .symlink-setting,
[data-theme="dark"] .admin-setting {
  background: rgba(94, 234, 212, 0.05);
  border-color: rgba(94, 234, 212, 0.15);
}

[data-theme="dark"] .symlink-setting:hover,
[data-theme="dark"] .admin-setting:hover {
  background: rgba(94, 234, 212, 0.08);
  border-color: rgba(94, 234, 212, 0.22);
}

[data-theme="dark"] .delete-mode-setting {
  background: rgba(251, 191, 36, 0.06);
  border-color: rgba(251, 191, 36, 0.2);
}

[data-theme="dark"] .delete-mode-setting:hover {
  background: rgba(251, 191, 36, 0.1);
  border-color: rgba(251, 191, 36, 0.28);
}

[data-theme="dark"] .toggle-slider {
  background: rgba(255, 255, 255, 0.1);
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.3);
}

[data-theme="dark"] .toggle-switch input:checked + .toggle-slider {
  background: linear-gradient(135deg, var(--color-highlight) 0%, var(--color-accent-secondary) 100%);
  border-color: rgba(94, 234, 212, 0.35);
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.2), 0 0 0 3px rgba(94, 234, 212, 0.1);
}

[data-theme="dark"] .info-card-enabled {
  background: rgba(52, 211, 153, 0.06);
  border-color: rgba(52, 211, 153, 0.2);
}

[data-theme="dark"] .info-card-warning {
  background: rgba(251, 191, 36, 0.06);
  border-color: rgba(251, 191, 36, 0.2);
}

[data-theme="dark"] .info-card-danger {
  background: rgba(248, 113, 113, 0.07);
  border-color: rgba(248, 113, 113, 0.22);
}

[data-theme="dark"] .info-icon {
  background: rgba(52, 211, 153, 0.12);
  border-color: rgba(52, 211, 153, 0.22);
}

[data-theme="dark"] .info-icon.warning {
  background: rgba(251, 191, 36, 0.12);
  border-color: rgba(251, 191, 36, 0.22);
}

[data-theme="dark"] .info-icon.danger {
  background: rgba(248, 113, 113, 0.12);
  border-color: rgba(248, 113, 113, 0.22);
}

[data-theme="dark"] .status-badge.elevated {
  background: rgba(52, 211, 153, 0.12);
  border-color: rgba(52, 211, 153, 0.25);
}

[data-theme="dark"] .status-badge.standard {
  background: rgba(255, 255, 255, 0.06);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .loading-spinner {
  border-color: rgba(255, 255, 255, 0.1);
  border-top-color: var(--color-highlight);
}

[data-theme="dark"] .cache-type-badge--deep {
  color: #c4b5fd;
  background: rgba(167, 139, 250, 0.15);
  border-color: rgba(167, 139, 250, 0.3);
}

[data-theme="dark"] .delete-cache-btn,
[data-theme="dark"] .clear-all-cache-btn {
  background: rgba(248, 113, 113, 0.1);
  border-color: rgba(248, 113, 113, 0.25);
  color: var(--color-error);
}

[data-theme="dark"] .delete-cache-btn:hover,
[data-theme="dark"] .clear-all-cache-btn:hover {
  background: rgba(248, 113, 113, 0.16);
  border-color: rgba(248, 113, 113, 0.35);
}

[data-theme="dark"] .theme-btn {
  background: rgba(255, 255, 255, 0.04);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .theme-btn:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: var(--color-border-strong);
}

[data-theme="dark"] .theme-btn.active {
  background: linear-gradient(135deg, var(--color-highlight) 0%, var(--color-accent-secondary) 100%);
  color: var(--color-bg-primary);
  box-shadow: 0 4px 12px rgba(20, 184, 166, 0.3);
}

[data-theme="dark"] .toggle-slider::before {
  background: linear-gradient(to bottom, #e5e7eb 0%, #cbd5e1 100%);
}

.ai-mode-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  margin-bottom: 14px;
}

.ai-mode-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  text-align: left;
  padding: 14px 16px;
  border-radius: 12px;
  border: 1.5px solid rgba(15, 23, 42, 0.1);
  background: rgba(15, 23, 42, 0.02);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease;
  color: inherit;
}

.ai-mode-card strong {
  font-size: 14px;
  color: var(--text-primary, #111827);
}

.ai-mode-card span {
  font-size: 12.5px;
  color: var(--text-secondary, #6b7280);
}

.ai-mode-card:hover:not(:disabled) {
  border-color: rgba(47, 109, 246, 0.5);
}

.ai-mode-card.active {
  border-color: #2f6df6;
  background: rgba(47, 109, 246, 0.08);
}

.ai-mode-card:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ai-select,
.ai-text-input {
  min-width: 220px;
  font-size: 13px;
}

.ai-category-toolbar {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
}

.inline-link {
  background: none;
  border: none;
  padding: 0;
  margin: 0;
  color: #2f6df6;
  cursor: pointer;
  font: inherit;
  text-decoration: underline;
}

.inline-link:hover {
  color: #1d4ed8;
}

.ai-modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 18, 28, 0.5);
  backdrop-filter: blur(4px);
  z-index: 9300;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.ai-modal {
  width: min(520px, 100%);
  max-height: 88vh;
  background: var(--surface, #ffffff);
  border-radius: 16px;
  box-shadow: 0 24px 60px rgba(0, 0, 0, 0.28);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.ai-modal--wide {
  width: min(720px, 100%);
}

.ai-modal-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.08));
}

.ai-modal-title {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.ai-modal-title h3 {
  margin: 0;
  font-size: 16px;
  color: var(--text-primary, #111827);
}

.ai-modal-body {
  padding: 16px 20px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  font-size: 13.5px;
  line-height: 1.65;
  color: var(--text-secondary, #4b5563);
}

.ai-modal-body ul {
  margin: 0;
  padding-left: 20px;
}

.ai-modal-body p {
  margin: 0;
}

.ai-consent-check {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: rgba(47, 109, 246, 0.06);
  border-radius: 8px;
  font-size: 13.5px;
  color: var(--text-primary, #111827);
  user-select: none;
}

.ai-modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 20px 16px;
  border-top: 1px solid var(--border, rgba(0, 0, 0, 0.06));
}

.ai-preview-hint {
  font-size: 13px;
}

.ai-preview-loading {
  padding: 24px;
  text-align: center;
  color: var(--text-secondary, #6b7280);
}

.ai-preview-json {
  background: rgba(15, 23, 42, 0.04);
  border-radius: 10px;
  padding: 14px 16px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12.5px;
  line-height: 1.55;
  max-height: 52vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--text-primary, #111827);
  margin: 0;
}

[data-theme="dark"] .ai-mode-card {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.1);
}

[data-theme="dark"] .ai-mode-card strong {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-mode-card span {
  color: #aab2c0;
}

[data-theme="dark"] .ai-mode-card.active {
  border-color: #2f6df6;
  background: rgba(47, 109, 246, 0.18);
}

[data-theme="dark"] .ai-modal {
  background: #1c1f29;
}

[data-theme="dark"] .ai-modal-head,
[data-theme="dark"] .ai-modal-footer {
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .ai-modal-title h3,
[data-theme="dark"] .ai-consent-check,
[data-theme="dark"] .ai-preview-json {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-modal-body {
  color: #cdd3df;
}

[data-theme="dark"] .ai-preview-json {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .ai-consent-check {
  background: rgba(47, 109, 246, 0.18);
}
</style>

<style scoped>
.settings-split {
  display: grid;
  grid-template-columns: 220px 1fr;
  gap: 0;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.settings-sidenav {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 16px 12px;
  background: var(--color-surface-soft, rgba(0, 0, 0, 0.02));
  border-right: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  overflow-y: auto;
}

.settings-sidenav-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--color-text-primary, #111827);
  cursor: pointer;
  text-align: left;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.settings-sidenav-item:hover {
  background: var(--color-surface-hover, rgba(0, 0, 0, 0.04));
}

.settings-sidenav-item.active {
  background: var(--color-highlight-soft, rgba(47, 109, 246, 0.1));
  border-color: var(--color-highlight, #2f6df6);
}

.settings-sidenav-item strong {
  font-size: 13px;
  font-weight: 700;
}

.settings-sidenav-item small {
  font-size: 11px;
  color: var(--color-text-tertiary, #6b7280);
  font-weight: 400;
  line-height: 1.4;
}

.settings-sidenav-item.active small {
  color: var(--color-text-secondary, #4b5563);
}

.settings-section-lead {
  padding: 4px 4px 12px;
  border-bottom: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.06));
  margin-bottom: 16px;
}

.settings-section-lead h3 {
  margin: 0 0 4px;
  font-size: 16px;
  font-weight: 700;
  color: var(--color-text-primary, #111827);
}

.settings-section-lead p {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-tertiary, #6b7280);
  line-height: 1.5;
}

[data-theme="dark"] .settings-sidenav {
  background: rgba(255, 255, 255, 0.03);
  border-right-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .settings-sidenav-item {
  color: #f5f5f7;
}

[data-theme="dark"] .settings-sidenav-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .settings-sidenav-item.active {
  background: rgba(96, 165, 250, 0.16);
  border-color: #60a5fa;
}

[data-theme="dark"] .settings-sidenav-item small {
  color: #aab2c0;
}

[data-theme="dark"] .settings-section-lead {
  border-bottom-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .settings-section-lead h3 {
  color: #f5f5f7;
}

[data-theme="dark"] .settings-section-lead p {
  color: #aab2c0;
}
</style>

