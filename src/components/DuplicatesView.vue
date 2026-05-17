<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { formatBytes } from '../utils/format';
import { useToast } from '../composables/useToast';

interface DuplicateFile {
  path: string;
  modified_at: string;
}

interface DuplicateGroup {
  size: number;
  files: DuplicateFile[];
  wasted_bytes: number;
}

interface DuplicateProgress {
  current_size: number;
  scanned_files: number;
  found_groups: number;
}

interface DeleteResult {
  success: boolean;
  deleted_size: number;
  deleted_files: number;
  errors: Array<{ path: string; error: string }>;
}

interface Props {
  rootPath: string;
  hasDeepScanned: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{ refresh: [] }>();

const showToast = useToast();
const groups = ref<DuplicateGroup[]>([]);
const scanning = ref(false);
const progress = ref<DuplicateProgress | null>(null);
const expanded = ref<Set<number>>(new Set());
const checked = ref<Set<string>>(new Set());
const deleting = ref(false);
const deleteProgress = ref<{ deleted: number; total: number }>({ deleted: 0, total: 0 });

let unlisten: UnlistenFn | null = null;

watch(
  () => props.rootPath,
  () => {
    groups.value = [];
    checked.value = new Set();
    expanded.value = new Set();
  },
);

onBeforeUnmount(() => {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
});

async function startScan() {
  if (!props.hasDeepScanned) {
    showToast('需要先扫描', '请先执行一次深度扫描', 'warning');
    return;
  }
  if (scanning.value) return;
  scanning.value = true;
  groups.value = [];
  checked.value = new Set();
  expanded.value = new Set();
  progress.value = { current_size: 0, scanned_files: 0, found_groups: 0 };

  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  unlisten = await listen<DuplicateProgress>('duplicate-progress', (event) => {
    progress.value = event.payload;
  });

  try {
    const result = await invoke<DuplicateGroup[]>('find_duplicates', {
      rootPath: props.rootPath,
    });
    groups.value = result;
    applyDefaultSelection(result);
    if (result.length === 0) {
      showToast('没有发现重复文件', '在 ≥ 100 MB 的大文件里没找到重复', 'info');
    } else {
      showToast(
        '重复文件分析完成',
        `${result.length} 组 · 共 ${formatBytes(result.reduce((s, g) => s + g.wasted_bytes, 0))} 冗余`,
        'success',
      );
    }
  } catch (err) {
    showToast('重复文件分析失败', String(err), 'error');
  } finally {
    scanning.value = false;
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
  }
}

function applyDefaultSelection(list: DuplicateGroup[]) {
  const next = new Set<string>();
  for (const group of list) {
    const sorted = [...group.files].sort((a, b) => b.modified_at.localeCompare(a.modified_at));
    for (let i = 1; i < sorted.length; i++) {
      next.add(sorted[i].path);
    }
  }
  checked.value = next;
}

const totalChecked = computed(() => checked.value.size);
const totalCheckedSize = computed(() => {
  let sum = 0;
  for (const group of groups.value) {
    for (const file of group.files) {
      if (checked.value.has(file.path)) {
        sum += group.size;
      }
    }
  }
  return sum;
});

function toggleExpand(index: number) {
  const next = new Set(expanded.value);
  if (next.has(index)) {
    next.delete(index);
  } else {
    next.add(index);
  }
  expanded.value = next;
}

function toggleFile(path: string) {
  const next = new Set(checked.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  checked.value = next;
}

function selectAllInGroup(group: DuplicateGroup) {
  const next = new Set(checked.value);
  const sorted = [...group.files].sort((a, b) => b.modified_at.localeCompare(a.modified_at));
  for (let i = 1; i < sorted.length; i++) {
    next.add(sorted[i].path);
  }
  checked.value = next;
}

function clearGroup(group: DuplicateGroup) {
  const next = new Set(checked.value);
  for (const file of group.files) {
    next.delete(file.path);
  }
  checked.value = next;
}

async function performDelete() {
  const paths = Array.from(checked.value);
  if (paths.length === 0) return;
  if (deleting.value) return;
  deleting.value = true;
  deleteProgress.value = { deleted: 0, total: paths.length };

  try {
    const result = await invoke<DeleteResult>('delete_duplicate_files', {
      paths,
      mode: 'permanent',
    });
    if (result.success) {
      showToast(
        '清理完成',
        `已释放 ${formatBytes(result.deleted_size)} · ${result.deleted_files} 项`,
        'success',
      );
    } else {
      showToast(
        '部分清理失败',
        `${result.errors.length} 项失败，已释放 ${formatBytes(result.deleted_size)}`,
        'warning',
      );
    }
    groups.value = groups.value
      .map((group) => ({
        ...group,
        files: group.files.filter((file) => !paths.includes(file.path)),
        wasted_bytes: Math.max(0, group.size * Math.max(0, group.files.filter((f) => !paths.includes(f.path)).length - 1)),
      }))
      .filter((group) => group.files.length >= 2);
    checked.value = new Set();
    emit('refresh');
  } catch (err) {
    showToast('清理失败', String(err), 'error');
  } finally {
    deleting.value = false;
  }
}

function isChecked(path: string) {
  return checked.value.has(path);
}

function formatModified(value: string) {
  if (!value) return '';
  return value.replace(/T/, ' ').slice(0, 19);
}
</script>

<template>
  <div class="dupe">
    <header class="dupe-head">
      <div class="dupe-head-copy">
        <h3>重复文件</h3>
        <p>从 ≥ 100 MB 大文件里找出 size + 内容完全相同的副本</p>
      </div>
      <div class="dupe-head-actions">
        <button
          v-if="groups.length === 0 && !scanning"
          class="dupe-btn dupe-btn--primary"
          :disabled="!hasDeepScanned"
          @click="startScan"
        >
          {{ hasDeepScanned ? '开始分析' : '请先深度扫描' }}
        </button>
        <button
          v-else-if="!scanning"
          class="dupe-btn dupe-btn--ghost"
          @click="startScan"
        >
          重新分析
        </button>
      </div>
    </header>

    <div v-if="scanning" class="dupe-scanning">
      <div class="dupe-scanning-spinner"></div>
      <div class="dupe-scanning-copy">
        <strong>分析中…</strong>
        <small v-if="progress">
          已扫 {{ progress.scanned_files }} 个候选 · 当前 {{ formatBytes(progress.current_size) }} · 找到 {{ progress.found_groups }} 组
        </small>
      </div>
    </div>

    <div v-else-if="groups.length === 0 && hasDeepScanned" class="dupe-empty">
      <div class="dupe-empty-mark">✓</div>
      <h4>没有发现重复文件</h4>
      <p>在 ≥ 100 MB 的大文件里没有找到 size + hash 都一致的副本。</p>
    </div>

    <div v-else-if="groups.length === 0" class="dupe-empty">
      <div class="dupe-empty-mark dupe-empty-mark--info">i</div>
      <h4>请先完成一次深度扫描</h4>
      <p>重复文件分析依赖于扫描结果里的大文件清单。</p>
    </div>

    <ul v-else class="dupe-list">
      <li v-for="(group, idx) in groups" :key="idx" class="dupe-group">
        <header class="dupe-group-head" @click="toggleExpand(idx)">
          <div class="dupe-group-meta">
            <strong>{{ formatBytes(group.size) }}</strong>
            <span>{{ group.files.length }} 份副本</span>
            <span class="dupe-group-waste">浪费 {{ formatBytes(group.wasted_bytes) }}</span>
          </div>
          <div class="dupe-group-actions" @click.stop>
            <button class="dupe-btn dupe-btn--mini" @click="selectAllInGroup(group)">保留最新</button>
            <button class="dupe-btn dupe-btn--mini dupe-btn--ghost" @click="clearGroup(group)">全部取消</button>
            <span class="dupe-toggle" :class="{ open: expanded.has(idx) }">▾</span>
          </div>
        </header>

        <ul v-if="expanded.has(idx)" class="dupe-files">
          <li v-for="file in [...group.files].sort((a, b) => b.modified_at.localeCompare(a.modified_at))" :key="file.path" class="dupe-file">
            <button
              class="dupe-check"
              :class="{ checked: isChecked(file.path) }"
              @click="toggleFile(file.path)"
              :aria-label="isChecked(file.path) ? '取消选择' : '选择'"
            >
              <svg v-if="isChecked(file.path)" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                <path d="M3 8.5L6.5 12L13 4.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" fill="none" />
              </svg>
            </button>
            <div class="dupe-file-main">
              <div class="dupe-file-path" :title="file.path">{{ file.path }}</div>
              <small>修改于 {{ formatModified(file.modified_at) }}</small>
            </div>
          </li>
        </ul>
      </li>
    </ul>

    <footer v-if="groups.length > 0 && !scanning" class="dupe-foot">
      <div class="dupe-foot-stats">
        <strong>{{ formatBytes(totalCheckedSize) }}</strong>
        <span>已选 {{ totalChecked }} 项</span>
      </div>
      <button
        class="dupe-btn dupe-btn--danger"
        :disabled="totalChecked === 0 || deleting"
        @click="performDelete"
      >
        {{ deleting ? '清理中…' : `清理 ${formatBytes(totalCheckedSize)}（已选 ${totalChecked} 项）` }}
      </button>
    </footer>
  </div>
</template>

<style scoped>
.dupe {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  height: 100%;
  min-height: 0;
}

.dupe-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.dupe-head-copy h3 {
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.dupe-head-copy p {
  margin-top: 0.18rem;
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
}

.dupe-btn {
  padding: 0.55rem 0.95rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  font-size: 0.84rem;
  font-weight: 600;
  cursor: pointer;
  transition: transform var(--transition-fast), background var(--transition-fast), color var(--transition-fast), box-shadow var(--transition-fast);
}

.dupe-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.dupe-btn:active:not(:disabled) {
  transform: scale(0.97);
}

.dupe-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.dupe-btn--primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  border-color: transparent;
  box-shadow: 0 10px 22px rgba(15, 118, 110, 0.22);
}

.dupe-btn--primary:hover:not(:disabled) {
  box-shadow: 0 14px 26px rgba(15, 118, 110, 0.3);
}

.dupe-btn--ghost {
  background: transparent;
}

.dupe-btn--mini {
  padding: 0.32rem 0.65rem;
  font-size: 0.76rem;
  border-radius: var(--radius-xs);
}

.dupe-btn--danger {
  background: var(--color-error);
  color: var(--color-text-inverse);
  border-color: transparent;
  box-shadow: 0 10px 22px rgba(220, 38, 38, 0.24);
}

.dupe-btn--danger:hover:not(:disabled) {
  box-shadow: 0 14px 28px rgba(220, 38, 38, 0.32);
}

.dupe-scanning {
  display: flex;
  align-items: center;
  gap: 0.85rem;
  padding: 1.5rem;
  border-radius: var(--radius-md);
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
}

.dupe-scanning-spinner {
  width: 28px;
  height: 28px;
  border: 2.5px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: var(--radius-pill);
  animation: dupe-spin 0.8s linear infinite;
}

@keyframes dupe-spin {
  to {
    transform: rotate(360deg);
  }
}

.dupe-scanning-copy {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.dupe-scanning-copy strong {
  color: var(--color-text-primary);
  font-size: 0.95rem;
}

.dupe-scanning-copy small {
  color: var(--color-text-tertiary);
  font-size: 0.8rem;
}

.dupe-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.5rem;
  padding: 2.5rem 1.5rem;
  border-radius: var(--radius-md);
  background: var(--color-surface);
  border: 1px dashed var(--color-border-medium);
  text-align: center;
}

.dupe-empty-mark {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-pill);
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  font-size: 1.2rem;
}

.dupe-empty-mark--info {
  background: rgba(37, 99, 235, 0.12);
  color: var(--color-info);
  font-family: var(--font-display);
}

.dupe-empty h4 {
  font-size: 1.05rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.dupe-empty p {
  font-size: 0.85rem;
  color: var(--color-text-tertiary);
  max-width: 26rem;
  line-height: 1.5;
}

.dupe-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.dupe-group {
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: var(--color-surface-strong);
  overflow: hidden;
}

.dupe-group-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.85rem;
  padding: 0.7rem 1rem;
  cursor: pointer;
  user-select: none;
  transition: background var(--transition-fast);
}

.dupe-group-head:hover {
  background: var(--color-surface-hover);
}

.dupe-group-meta {
  display: flex;
  align-items: baseline;
  gap: 0.7rem;
  flex-wrap: wrap;
}

.dupe-group-meta strong {
  font-family: var(--font-display);
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.dupe-group-meta span {
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
}

.dupe-group-waste {
  padding: 0.18rem 0.5rem;
  border-radius: var(--radius-pill);
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
  font-size: 0.74rem;
  font-weight: 600;
  font-feature-settings: 'tnum';
}

.dupe-group-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.dupe-toggle {
  color: var(--color-text-tertiary);
  font-size: 1rem;
  transition: transform var(--transition-fast);
}

.dupe-toggle.open {
  transform: rotate(180deg);
}

.dupe-files {
  list-style: none;
  padding: 0.5rem 0;
  margin: 0;
  border-top: 1px solid var(--color-border-light);
  background: var(--color-bg-secondary);
}

.dupe-file {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 0.7rem;
  padding: 0.5rem 1rem;
}

.dupe-check {
  width: 20px;
  height: 20px;
  border-radius: var(--radius-xs);
  border: 1.5px solid var(--color-border-medium);
  background: var(--color-surface);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-inverse);
  transition: background var(--transition-fast), border-color var(--transition-fast);
}

.dupe-check:hover {
  border-color: var(--color-text-secondary);
}

.dupe-check.checked {
  background: var(--color-highlight);
  border-color: var(--color-highlight);
}

.dupe-file-main {
  min-width: 0;
}

.dupe-file-path {
  font-family: var(--font-mono);
  font-size: 0.82rem;
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.dupe-file-main small {
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  margin-top: 0.18rem;
  display: block;
}

.dupe-foot {
  position: sticky;
  bottom: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 0.85rem 1rem;
  border-radius: var(--radius-md);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.92), var(--color-surface-strong));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-sm);
}

.dupe-foot-stats {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.dupe-foot-stats strong {
  font-family: var(--font-display);
  font-size: 1.2rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.dupe-foot-stats span {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}
</style>
