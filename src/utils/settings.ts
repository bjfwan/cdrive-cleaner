import type { AppSettings, CloseBehavior, DeleteMode, DownloadMirror } from '../types';

const SETTINGS_KEY = 'cdrive-cleaner-settings';

const DEFAULT_SETTINGS: AppSettings = {
  defaultTargetDisk: '',
  largeFileThreshold: 100,
  createSymlink: true,
  defaultDeleteMode: 'recycle',
  autoCheckUpdate: true,
  downloadMirror: 'none',
  downloadMirrorUrl: '',
  closeBehavior: 'exit',
  schedulerEnabled: true,
  schedulerIdleMinutes: 5,
};

function normalizeDeleteMode(value: unknown): DeleteMode {
  return value === 'permanent' ? 'permanent' : 'recycle';
}

function normalizeCloseBehavior(value: unknown): CloseBehavior {
  return value === 'tray' ? 'tray' : 'exit';
}

function normalizeDownloadMirror(value: unknown): DownloadMirror {
  if (value === 'ghproxy' || value === 'custom' || value === 'none') return value;
  return 'none';
}

function normalizeSettings(value: Partial<AppSettings>): AppSettings {
  const threshold = Number(value.largeFileThreshold);
  const theme: AppSettings['theme'] =
    value.theme === 'dark' || value.theme === 'light' || value.theme === 'auto'
      ? value.theme
      : undefined;

  const idleRaw = Number(value.schedulerIdleMinutes);
  const idleMinutes = Number.isFinite(idleRaw)
    ? Math.min(Math.max(Math.round(idleRaw), 1), 240)
    : DEFAULT_SETTINGS.schedulerIdleMinutes!;

  return {
    defaultTargetDisk: typeof value.defaultTargetDisk === 'string' ? value.defaultTargetDisk : '',
    largeFileThreshold: Number.isFinite(threshold) ? Math.min(Math.max(Math.round(threshold), 1), 10000) : DEFAULT_SETTINGS.largeFileThreshold,
    createSymlink: typeof value.createSymlink === 'boolean' ? value.createSymlink : DEFAULT_SETTINGS.createSymlink,
    defaultDeleteMode: normalizeDeleteMode(value.defaultDeleteMode),
    autoCheckUpdate: typeof value.autoCheckUpdate === 'boolean' ? value.autoCheckUpdate : DEFAULT_SETTINGS.autoCheckUpdate,
    downloadMirror: normalizeDownloadMirror(value.downloadMirror),
    downloadMirrorUrl: typeof value.downloadMirrorUrl === 'string' ? value.downloadMirrorUrl : '',
    closeBehavior: normalizeCloseBehavior(value.closeBehavior),
    schedulerEnabled: typeof value.schedulerEnabled === 'boolean' ? value.schedulerEnabled : DEFAULT_SETTINGS.schedulerEnabled,
    schedulerIdleMinutes: idleMinutes,
    ...(theme !== undefined ? { theme } : {}),
  };
}

export function getSettings(): AppSettings {
  const saved = localStorage.getItem(SETTINGS_KEY);
  if (!saved) return normalizeSettings(DEFAULT_SETTINGS);
  try {
    return normalizeSettings({ ...DEFAULT_SETTINGS, ...JSON.parse(saved) });
  } catch {
    return normalizeSettings(DEFAULT_SETTINGS);
  }
}

export function saveSettings(settings: AppSettings): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(normalizeSettings(settings)));
}

/**
 * Track A (v0.1.9)：把后台扫描/托盘相关字段同步到 Rust 端的 settings.json，
 * 这样 Rust 侧的 close 事件处理 + 调度 tick 才能看到用户的最新选择。
 * 主流程仍以 localStorage 为准；这里只是把指定字段推送到统一存储。
 */
export async function syncTrackASettingsToBackend(settings: AppSettings): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  const normalized = normalizeSettings(settings);
  const pairs: Array<[string, unknown]> = [
    ['closeBehavior', normalized.closeBehavior],
    ['schedulerEnabled', normalized.schedulerEnabled],
    ['schedulerIdleMinutes', normalized.schedulerIdleMinutes],
    ['defaultTargetDisk', normalized.defaultTargetDisk],
  ];
  for (const [key, value] of pairs) {
    try {
      await invoke('cmd_settings_set', { path: key, value });
    } catch (err) {
      console.warn(`[settings] sync ${key} -> backend failed`, err);
    }
  }
}
