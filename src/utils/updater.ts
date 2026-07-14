import { invoke } from '@tauri-apps/api/core';
import type { UpdateInfo } from '../types';

const CHECK_COOLDOWN_KEY = 'cdrive-cleaner-last-update-check';
const COOLDOWN_MS = 4 * 60 * 60 * 1000; // 4 hours

export async function checkForUpdates(): Promise<UpdateInfo> {
  return invoke<UpdateInfo>('check_for_updates');
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
