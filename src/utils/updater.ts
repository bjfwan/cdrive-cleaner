import { invoke } from '@tauri-apps/api/core';
import type { UpdateInfo } from '../types';
import { getSettings } from './settings';

const CHECK_COOLDOWN_KEY = 'cdrive-cleaner-last-update-check';
const COOLDOWN_MS = 4 * 60 * 60 * 1000; // 4 hours

const MIRROR_PRESETS: Record<string, string> = {
  ghproxy: 'https://mirror.ghproxy.com/',
};

export function resolveMirrorUrl(originalUrl: string): string {
  const settings = getSettings();
  if (settings.downloadMirror === 'none' || !settings.downloadMirror) {
    return originalUrl;
  }
  if (settings.downloadMirror === 'custom' && settings.downloadMirrorUrl) {
    const base = settings.downloadMirrorUrl.replace(/\/+$/, '');
    return `${base}/${originalUrl}`;
  }
  const preset = MIRROR_PRESETS[settings.downloadMirror];
  if (preset) {
    return `${preset}${originalUrl}`;
  }
  return originalUrl;
}

export async function checkForUpdates(): Promise<UpdateInfo> {
  return invoke<UpdateInfo>('check_for_updates');
}

export async function downloadUpdate(url: string): Promise<string> {
  return invoke<string>('download_update', { url });
}

export async function installUpdate(path: string): Promise<void> {
  return invoke<void>('install_update', { path });
}

export function shouldAutoCheck(): boolean {
  const last = localStorage.getItem(CHECK_COOLDOWN_KEY);
  if (!last) return true;
  const elapsed = Date.now() - Number(last);
  return elapsed >= COOLDOWN_MS;
}

export function markChecked(): void {
  localStorage.setItem(CHECK_COOLDOWN_KEY, String(Date.now()));
}
