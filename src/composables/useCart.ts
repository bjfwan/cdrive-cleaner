import { computed, reactive, readonly } from 'vue';

export interface CartEntry {
  path: string;
  name: string;
  size: number;
  file_count: number;
  recommendation: 'migrate' | 'delete' | 'review';
  source: 'smart_scan' | 'browse' | 'large_file';
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
    add,
    remove,
    toggle,
    has,
    clear,
    setFromSmartGroups,
  };
}
