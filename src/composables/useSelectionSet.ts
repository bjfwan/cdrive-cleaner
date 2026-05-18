import { computed, shallowRef, triggerRef, type ComputedRef } from 'vue';

type ChangeListener<T> = (set: ReadonlySet<T>) => void;

export interface SelectionSet<T> {
  /** 当前 Set 的只读视图。读取后会建立响应式依赖。 */
  readonly value: ReadonlySet<T>;
  /** 响应式 size。 */
  readonly size: number;
  /** 用于模板里直接 v-if/v-bind 的 size computed。 */
  sizeRef: ComputedRef<number>;
  has(key: T): boolean;
  add(key: T): boolean;
  delete(key: T): boolean;
  toggle(key: T): boolean;
  addAll(keys: Iterable<T>): void;
  removeAll(keys: Iterable<T>): void;
  replace(keys: Iterable<T>): void;
  clear(): void;
  values(): IterableIterator<T>;
  forEach(cb: (key: T) => void): void;
  onChange(listener: ChangeListener<T>): () => void;
}

/**
 * 基于 shallowRef + 原地 Set 的选择集合。
 *
 * 与 ref<Set<T>>(new Set()) + new Set(prev) 的写法不同，这里不会每次写入都
 * 重新分配 Set；只在 size 真正改变时（或者经过 toggle/replace 等显式触发）
 * 通过 triggerRef 通知一次依赖，从而避免 O(n) 拷贝 + O(n) 重渲。
 */
export function useSelectionSet<T>(initial?: Iterable<T>): SelectionSet<T> {
  const set = new Set<T>(initial ?? []);
  const setRef = shallowRef<Set<T>>(set);
  const listeners = new Set<ChangeListener<T>>();

  function emit() {
    triggerRef(setRef);
    if (listeners.size > 0) {
      for (const l of listeners) l(set);
    }
  }

  function has(key: T) {
    // 读取 setRef.value 建立依赖
    return setRef.value.has(key);
  }

  function add(key: T) {
    if (set.has(key)) return false;
    set.add(key);
    emit();
    return true;
  }

  function del(key: T) {
    if (!set.delete(key)) return false;
    emit();
    return true;
  }

  function toggle(key: T) {
    if (set.has(key)) {
      set.delete(key);
      emit();
      return false;
    }
    set.add(key);
    emit();
    return true;
  }

  function addAll(keys: Iterable<T>) {
    let changed = false;
    for (const k of keys) {
      if (!set.has(k)) {
        set.add(k);
        changed = true;
      }
    }
    if (changed) emit();
  }

  function removeAll(keys: Iterable<T>) {
    let changed = false;
    for (const k of keys) {
      if (set.delete(k)) changed = true;
    }
    if (changed) emit();
  }

  function replace(keys: Iterable<T>) {
    set.clear();
    for (const k of keys) set.add(k);
    emit();
  }

  function clear() {
    if (set.size === 0) return;
    set.clear();
    emit();
  }

  function onChange(listener: ChangeListener<T>) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  const sizeRef = computed(() => setRef.value.size);

  const api: SelectionSet<T> = {
    get value() {
      return setRef.value;
    },
    get size() {
      return setRef.value.size;
    },
    sizeRef,
    has,
    add,
    delete: del,
    toggle,
    addAll,
    removeAll,
    replace,
    clear,
    values: () => set.values(),
    forEach: (cb) => {
      // 建立依赖
      setRef.value.forEach(cb);
    },
    onChange,
  };

  return api;
}
