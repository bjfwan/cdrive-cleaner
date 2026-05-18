import { invoke } from '@tauri-apps/api/core';

import type { GameLibraryInfo } from '../types';

/**
 * Module-scoped cache for `detect_game_libraries`.
 *
 * `GamesView.vue` is mounted via `v-else-if` and unmounted whenever the user
 * leaves the "游戏库" tab, so any cache living inside the component is wiped on
 * every tab switch. Each detection takes 1~2s scanning Steam/Epic/Game Pass,
 * so repeatedly re-running it for tab toggles is wasteful. The cache here
 * survives remounts and also de-dupes concurrent in-flight calls (e.g. the
 * mount-time auto-load racing with the toast in `App.vue`).
 */

const TTL_MS = 5 * 60 * 1000;

interface CacheEntry {
  result: GameLibraryInfo[];
  cachedAt: number;
}

let cache: CacheEntry | null = null;
let inflight: Promise<GameLibraryInfo[]> | null = null;

function isFresh(entry: CacheEntry): boolean {
  return Date.now() - entry.cachedAt <= TTL_MS;
}

export function getCachedGameLibraries(): GameLibraryInfo[] | null {
  if (cache && isFresh(cache)) return cache.result;
  return null;
}

/**
 * Get the latest game libraries.
 *
 * - When `forceRefresh` is false (default) and a fresh cached result is
 *   available, returns it without invoking the backend.
 * - Concurrent callers share a single in-flight invocation.
 * - On force refresh, the cache is bypassed and replaced on success.
 */
export async function fetchGameLibraries(
  options: { forceRefresh?: boolean } = {},
): Promise<GameLibraryInfo[]> {
  const { forceRefresh = false } = options;

  if (!forceRefresh && cache && isFresh(cache)) {
    return cache.result;
  }

  if (inflight) {
    return inflight;
  }

  inflight = (async () => {
    try {
      const result = await invoke<GameLibraryInfo[]>('detect_game_libraries');
      cache = { result, cachedAt: Date.now() };
      return result;
    } finally {
      inflight = null;
    }
  })();

  return inflight;
}

export function invalidateGameLibrariesCache(): void {
  cache = null;
}
