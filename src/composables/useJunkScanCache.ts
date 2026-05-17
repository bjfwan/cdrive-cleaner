import type { JunkCategory, JunkScanResult } from '../types/junk';

export interface CachedScan {
  result: JunkScanResult;
  cachedAt: number;
  categories: JunkCategory[] | null;
}

export interface JunkScanCache {
  get(categories: JunkCategory[] | null): CachedScan | null;
  set(categories: JunkCategory[] | null, result: JunkScanResult): void;
  clear(): void;
}

const TTL_MS = 5 * 60 * 1000;

const store = new Map<string, CachedScan>();

function keyOf(categories: JunkCategory[] | null): string {
  if (categories === null) return '__ALL__';
  return [...categories].sort().join(',');
}

export function useJunkScanCache(): JunkScanCache {
  return {
    get(categories) {
      try {
        const key = keyOf(categories);
        const entry = store.get(key);
        if (!entry) return null;
        if (Date.now() - entry.cachedAt > TTL_MS) {
          store.delete(key);
          return null;
        }
        return entry;
      } catch {
        return null;
      }
    },
    set(categories, result) {
      try {
        const key = keyOf(categories);
        store.clear();
        store.set(key, {
          result,
          cachedAt: Date.now(),
          categories: categories === null ? null : [...categories],
        });
      } catch {
        return;
      }
    },
    clear() {
      try {
        store.clear();
      } catch {
        return;
      }
    },
  };
}
