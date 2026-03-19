import type { AppSettings } from '../types';

const SETTINGS_KEY = 'cdrive-cleaner-settings';

const DEFAULT_SETTINGS: AppSettings = {
  defaultTargetDisk: '',
  largeFileThreshold: 100,
  createSymlink: true,
};

export function getSettings(): AppSettings {
  const saved = localStorage.getItem(SETTINGS_KEY);
  if (!saved) return { ...DEFAULT_SETTINGS };
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(saved) };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings: AppSettings): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}
