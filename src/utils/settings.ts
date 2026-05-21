import type { AppSettings, DeleteMode } from '../types';

const SETTINGS_KEY = 'cdrive-cleaner-settings';

const DEFAULT_SETTINGS: AppSettings = {
  defaultTargetDisk: '',
  largeFileThreshold: 100,
  createSymlink: true,
  defaultDeleteMode: 'recycle',
  autoCheckUpdate: true,
};

function normalizeDeleteMode(value: unknown): DeleteMode {
  return value === 'permanent' ? 'permanent' : 'recycle';
}

function normalizeSettings(value: Partial<AppSettings>): AppSettings {
  const threshold = Number(value.largeFileThreshold);
  const theme: AppSettings['theme'] =
    value.theme === 'dark' || value.theme === 'light' || value.theme === 'auto'
      ? value.theme
      : undefined;

  return {
    defaultTargetDisk: typeof value.defaultTargetDisk === 'string' ? value.defaultTargetDisk : '',
    largeFileThreshold: Number.isFinite(threshold) ? Math.min(Math.max(Math.round(threshold), 1), 10000) : DEFAULT_SETTINGS.largeFileThreshold,
    createSymlink: typeof value.createSymlink === 'boolean' ? value.createSymlink : DEFAULT_SETTINGS.createSymlink,
    defaultDeleteMode: normalizeDeleteMode(value.defaultDeleteMode),
    autoCheckUpdate: typeof value.autoCheckUpdate === 'boolean' ? value.autoCheckUpdate : DEFAULT_SETTINGS.autoCheckUpdate,
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
