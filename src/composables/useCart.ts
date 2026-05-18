import { computed, shallowRef, triggerRef, type ComputedRef } from 'vue';

import type { GamePlatform } from '../types';

export interface CartEntry {
  path: string;
  name: string;
  size: number;
  file_count: number;
  recommendation: 'migrate' | 'delete' | 'review';
  source: 'smart_scan' | 'browse' | 'large_file' | 'game';
  game?: {
    platform: GamePlatform;
    app_id: string;
  };
}

interface Accumulator {
  count: number;
  totalSize: number;
  migrateCount: number;
  migrateSize: number;
  deleteCount: number;
  deleteSize: number;
  reviewCount: number;
  reviewSize: number;
}

const map = new Map<string, CartEntry>();
const acc: Accumulator = {
  count: 0,
  totalSize: 0,
  migrateCount: 0,
  migrateSize: 0,
  deleteCount: 0,
  deleteSize: 0,
  reviewCount: 0,
  reviewSize: 0,
};

// 单一 trigger ref：每当 map 内容发生变化，bump version 并 triggerRef，
// 让所有依赖此 ref 的 computed 重新计算（懒构建）。
const versionRef = shallowRef(0);
let version = 0;

function bump() {
  version = (version + 1) | 0;
  versionRef.value = version;
  triggerRef(versionRef);
}

function bucketAdd(entry: CartEntry) {
  acc.count += 1;
  acc.totalSize += entry.size;
  if (entry.recommendation === 'migrate') {
    acc.migrateCount += 1;
    acc.migrateSize += entry.size;
  } else if (entry.recommendation === 'delete') {
    acc.deleteCount += 1;
    acc.deleteSize += entry.size;
  } else {
    acc.reviewCount += 1;
    acc.reviewSize += entry.size;
  }
}

function bucketRemove(entry: CartEntry) {
  acc.count -= 1;
  acc.totalSize -= entry.size;
  if (entry.recommendation === 'migrate') {
    acc.migrateCount -= 1;
    acc.migrateSize -= entry.size;
  } else if (entry.recommendation === 'delete') {
    acc.deleteCount -= 1;
    acc.deleteSize -= entry.size;
  } else {
    acc.reviewCount -= 1;
    acc.reviewSize -= entry.size;
  }
}

function bucketReset() {
  acc.count = 0;
  acc.totalSize = 0;
  acc.migrateCount = 0;
  acc.migrateSize = 0;
  acc.deleteCount = 0;
  acc.deleteSize = 0;
  acc.reviewCount = 0;
  acc.reviewSize = 0;
}

// ---------- 公开的 API ----------

function add(entry: CartEntry) {
  const key = entry.path.toLowerCase();
  const prev = map.get(key);
  if (prev) {
    bucketRemove(prev);
  }
  map.set(key, entry);
  bucketAdd(entry);
  bump();
}

/**
 * 批量加入，仅 bump 一次版本，避免在循环里触发 N 次响应式重算。
 */
function addBatch(entries: ReadonlyArray<CartEntry>) {
  if (entries.length === 0) return;
  let changed = false;
  for (let i = 0; i < entries.length; i += 1) {
    const entry = entries[i];
    const key = entry.path.toLowerCase();
    const prev = map.get(key);
    if (prev) bucketRemove(prev);
    map.set(key, entry);
    bucketAdd(entry);
    changed = true;
  }
  if (changed) bump();
}

function remove(path: string) {
  const key = path.toLowerCase();
  const prev = map.get(key);
  if (!prev) return;
  map.delete(key);
  bucketRemove(prev);
  bump();
}

/**
 * 批量移除，仅 bump 一次版本。
 */
function removeBatch(paths: ReadonlyArray<string>) {
  if (paths.length === 0) return;
  let changed = false;
  for (let i = 0; i < paths.length; i += 1) {
    const key = paths[i].toLowerCase();
    const prev = map.get(key);
    if (!prev) continue;
    map.delete(key);
    bucketRemove(prev);
    changed = true;
  }
  if (changed) bump();
}

function toggle(entry: CartEntry) {
  const key = entry.path.toLowerCase();
  const prev = map.get(key);
  if (prev) {
    map.delete(key);
    bucketRemove(prev);
  } else {
    map.set(key, entry);
    bucketAdd(entry);
  }
  bump();
}

function has(path: string) {
  // 读取 versionRef 建立响应式依赖
  void versionRef.value;
  return map.has(path.toLowerCase());
}

function clear() {
  if (map.size === 0) return;
  map.clear();
  bucketReset();
  bump();
}

function setFromSmartGroups(
  groups: Array<{
    items: Array<{
      path: string;
      name: string;
      size: number;
      file_count: number;
      recommendation: 'migrate' | 'delete' | 'review';
      default_selected: boolean;
    }>;
  }>,
) {
  map.clear();
  bucketReset();
  for (const g of groups) {
    for (const it of g.items) {
      if (it.default_selected) {
        const entry: CartEntry = {
          path: it.path,
          name: it.name,
          size: it.size,
          file_count: it.file_count,
          recommendation: it.recommendation,
          source: 'smart_scan',
        };
        map.set(it.path.toLowerCase(), entry);
        bucketAdd(entry);
      }
    }
  }
  bump();
}

// ---------- 懒构建的派生数组 ----------

function makeFiltered(
  predicate: (e: CartEntry) => boolean,
): ComputedRef<readonly CartEntry[]> {
  let cachedVersion = -1;
  let cachedArr: CartEntry[] = [];
  return computed(() => {
    const v = versionRef.value;
    if (v === cachedVersion) return cachedArr;
    const arr: CartEntry[] = [];
    for (const e of map.values()) {
      if (predicate(e)) arr.push(e);
    }
    cachedVersion = v;
    cachedArr = arr;
    return arr;
  });
}

const items = (() => {
  let cachedVersion = -1;
  let cachedArr: CartEntry[] = [];
  return computed(() => {
    const v = versionRef.value;
    if (v === cachedVersion) return cachedArr;
    cachedArr = Array.from(map.values());
    cachedVersion = v;
    return cachedArr;
  });
})();

const migrateItems = makeFiltered((e) => e.recommendation === 'migrate');
const deleteItems = makeFiltered((e) => e.recommendation === 'delete');
const reviewItems = makeFiltered((e) => e.recommendation === 'review');

const totalSize = computed(() => {
  void versionRef.value;
  return acc.totalSize;
});
const count = computed(() => {
  void versionRef.value;
  return acc.count;
});

const migrateSize = computed(() => {
  void versionRef.value;
  return acc.migrateSize;
});
const deleteSize = computed(() => {
  void versionRef.value;
  return acc.deleteSize;
});
const reviewSize = computed(() => {
  void versionRef.value;
  return acc.reviewSize;
});

export function useCart() {
  return {
    items,
    totalSize,
    count,
    migrateItems,
    deleteItems,
    reviewItems,
    migrateSize,
    deleteSize,
    reviewSize,
    add,
    addBatch,
    remove,
    removeBatch,
    toggle,
    has,
    clear,
    setFromSmartGroups,
  };
}
