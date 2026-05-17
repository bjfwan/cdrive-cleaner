import { computed, reactive, readonly } from 'vue';

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

interface CartState {
  items: Map<string, CartEntry>;
}

const state = reactive<CartState>({
  items: new Map(),
});

const items = computed(() => Array.from(state.items.values()));
const totalSize = computed(() => items.value.reduce((sum, it) => sum + it.size, 0));
const count = computed(() => state.items.size);

const migrateItems = computed(() => items.value.filter((it) => it.recommendation === 'migrate'));
const deleteItems = computed(() => items.value.filter((it) => it.recommendation === 'delete'));
const reviewItems = computed(() => items.value.filter((it) => it.recommendation === 'review'));

const migrateSize = computed(() => migrateItems.value.reduce((s, it) => s + it.size, 0));
const deleteSize = computed(() => deleteItems.value.reduce((s, it) => s + it.size, 0));
const reviewSize = computed(() => reviewItems.value.reduce((s, it) => s + it.size, 0));

function add(entry: CartEntry) {
  state.items.set(entry.path.toLowerCase(), entry);
}

function remove(path: string) {
  state.items.delete(path.toLowerCase());
}

function toggle(entry: CartEntry) {
  const key = entry.path.toLowerCase();
  if (state.items.has(key)) {
    state.items.delete(key);
  } else {
    state.items.set(key, entry);
  }
}

function has(path: string) {
  return state.items.has(path.toLowerCase());
}

function clear() {
  state.items.clear();
}

function setFromSmartGroups(
  groups: Array<{ items: Array<{ path: string; name: string; size: number; file_count: number; recommendation: 'migrate' | 'delete' | 'review'; default_selected: boolean }> }>,
) {
  state.items.clear();
  for (const g of groups) {
    for (const it of g.items) {
      if (it.default_selected) {
        state.items.set(it.path.toLowerCase(), {
          path: it.path,
          name: it.name,
          size: it.size,
          file_count: it.file_count,
          recommendation: it.recommendation,
          source: 'smart_scan',
        });
      }
    }
  }
}

export function useCart() {
  return {
    items: readonly(items),
    totalSize: readonly(totalSize),
    count: readonly(count),
    migrateItems: readonly(migrateItems),
    deleteItems: readonly(deleteItems),
    reviewItems: readonly(reviewItems),
    migrateSize: readonly(migrateSize),
    deleteSize: readonly(deleteSize),
    reviewSize: readonly(reviewSize),
    add,
    remove,
    toggle,
    has,
    clear,
    setFromSmartGroups,
  };
}
