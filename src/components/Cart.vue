<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useCart } from '../composables/useCart';
import { formatBytes } from '../utils/format';
import type { DeleteMode, DiskInfo } from '../types';
import VirtualList from './VirtualList.vue';

interface CartProgress {
  current: number;
  total: number;
  currentItem: string;
}

type CartResult =
  | { path: string; ok: true }
  | { path: string; ok: false; name: string; error: string };

interface Props {
  availableDisks: DiskInfo[];
  currentDrive: string;
  busy?: boolean;
  progress?: CartProgress;
  results?: CartResult[];
  defaultDeleteMode?: DeleteMode;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  run: [payload: { targetDisk: string; deleteMode: DeleteMode }];
  close: [];
  'reset-results': [];
}>();

const cart = useCart();
const { items, totalSize, count, migrateItems, deleteItems, migrateSize, deleteSize } = cart;
const open = ref(false);

watch(
  () => props.busy,
  (busy) => {
    if (busy) open.value = true;
  },
);

watch(
  () => props.results?.length ?? 0,
  (n) => {
    if (n > 0) open.value = true;
  },
);

const targetOptions = computed(() => props.availableDisks.filter((d) => `${d.drive_letter}\\` !== props.currentDrive));
const targetDisk = ref<string>('');
watch(
  targetOptions,
  (next) => {
    if (!targetDisk.value && next.length > 0) targetDisk.value = `${next[0].drive_letter}\\`;
  },
  { immediate: true },
);

const progressPercent = computed(() => {
  const p = props.progress;
  if (!p || p.total === 0) return 0;
  return Math.round((p.current / p.total) * 100);
});

const successCount = computed(() => (props.results ?? []).filter((r) => r.ok).length);
const failedItems = computed(
  () => (props.results ?? []).filter((r): r is Extract<CartResult, { ok: false }> => !r.ok),
);
const hasResults = computed(() => (props.results?.length ?? 0) > 0);

const hasMigrate = computed(() => migrateItems.value.length > 0);
const hasDelete = computed(() => deleteItems.value.length > 0);
const isMixed = computed(() => hasMigrate.value && hasDelete.value);
const deleteMode = computed<DeleteMode>(() => props.defaultDeleteMode ?? 'recycle');
const deleteTargetLabel = computed(() => (deleteMode.value === 'permanent' ? '永久删除' : '回收站'));

const primaryLabel = computed(() => {
  if (props.busy) return '处理中…';
  if (isMixed.value) return `处理 ${count.value} 项（${formatBytes(totalSize.value)}）`;
  if (hasMigrate.value) return `一键搬走 ${formatBytes(migrateSize.value)}`;
  if (hasDelete.value) {
    return deleteMode.value === 'permanent'
      ? `永久删除 ${formatBytes(deleteSize.value)}`
      : `清理 ${formatBytes(deleteSize.value)} → 回收站`;
  }
  return '搬运车为空';
});

const primaryDisabled = computed(() => {
  if (props.busy || count.value === 0) return true;
  if (hasMigrate.value && !targetDisk.value) return true;
  return false;
});

function start() {
  if (primaryDisabled.value) return;
  emit('run', { targetDisk: targetDisk.value, deleteMode: deleteMode.value });
}

function reveal(path: string) {
  cart.remove(path);
}

function clearCart() {
  cart.clear();
}

function dismissResults() {
  emit('reset-results');
}
</script>

<template>
  <Teleport to="body">
    <button
      v-if="count > 0"
      class="cart-fab"
      :class="{ open }"
      @click="open = !open"
    >
      <span class="cart-icon">🛒</span>
      <span class="cart-stats">
        <strong>{{ count }}</strong>
        <small>项 · {{ formatBytes(totalSize) }}</small>
      </span>
    </button>

    <div v-if="open" class="cart-overlay" @click="open = false"></div>

    <aside class="cart-drawer" :class="{ open }" role="dialog" aria-label="搬运车">
      <header class="drawer-head">
        <div>
          <h3>搬运车</h3>
          <p>
            <template v-if="busy">正在处理 · {{ progressPercent }}%</template>
            <template v-else-if="hasResults">完成：成功 {{ successCount }} · 失败 {{ failedItems.length }}</template>
            <template v-else>{{ count }} 项 · 共 {{ formatBytes(totalSize) }}</template>
          </p>
        </div>
        <button class="icon-btn" @click="open = false" :disabled="busy" aria-label="关闭">✕</button>
      </header>

      <div v-if="!busy && !hasResults && (hasMigrate || hasDelete)" class="segments">
        <div v-if="hasMigrate" class="segment segment-migrate">
          <span class="segment-icon">🚚</span>
          <div class="segment-text">
            <strong>搬走 {{ migrateItems.length }} 项</strong>
            <small>{{ formatBytes(migrateSize) }} · 走 junction</small>
          </div>
        </div>
        <div v-if="hasDelete" class="segment segment-delete" :class="{ permanent: deleteMode === 'permanent' }">
          <span class="segment-icon">{{ deleteMode === 'permanent' ? '⚠️' : '🗑️' }}</span>
          <div class="segment-text">
            <strong>清理 {{ deleteItems.length }} 项</strong>
            <small>{{ formatBytes(deleteSize) }} · {{ deleteTargetLabel }}</small>
          </div>
        </div>
      </div>

      <div v-if="busy" class="busy-panel">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: `${progressPercent}%` }"></div>
        </div>
        <div class="progress-meta">
          <span>{{ progress?.current ?? 0 }} / {{ progress?.total ?? 0 }}</span>
          <span class="progress-current" :title="progress?.currentItem">{{ progress?.currentItem || '准备中…' }}</span>
        </div>
      </div>

      <div v-else-if="hasResults" class="results-panel">
        <div class="results-summary">
          <div class="results-stat ok">
            <strong>{{ successCount }}</strong>
            <small>成功搬走</small>
          </div>
          <div class="results-stat fail">
            <strong>{{ failedItems.length }}</strong>
            <small>失败</small>
          </div>
        </div>

        <div v-if="failedItems.length > 0" class="results-failed">
          <div class="results-title">失败明细</div>
          <ul>
            <li v-for="item in failedItems" :key="item.path" class="result-fail-item">
              <div class="result-fail-name">{{ item.name }}</div>
              <div class="result-fail-error">{{ item.error }}</div>
              <div class="result-fail-path">{{ item.path }}</div>
            </li>
          </ul>
        </div>

        <button class="dismiss-btn" @click="dismissResults">知道了</button>
      </div>

      <div v-else class="drawer-list-wrap">
        <VirtualList
          :items="items"
          :item-size="64"
          :buffer="6"
          v-slot="{ item }"
        >
          <div :key="(item as any).path" class="drawer-item">
            <div class="drawer-item-main">
              <div class="drawer-item-name" :title="(item as any).path">
                <span
                  class="drawer-item-tag"
                  :class="`tag-${(item as any).recommendation}`"
                >{{ (item as any).recommendation === 'migrate' ? '搬' : (item as any).recommendation === 'delete' ? '删' : '查' }}</span>
                {{ (item as any).name }}
              </div>
              <div class="drawer-item-path">{{ (item as any).path }}</div>
            </div>
            <div class="drawer-item-size">{{ formatBytes((item as any).size) }}</div>
            <button class="icon-btn small" @click="reveal((item as any).path)" aria-label="移出">×</button>
          </div>
        </VirtualList>
      </div>

      <footer v-if="!busy && !hasResults" class="drawer-foot">
        <template v-if="hasMigrate">
          <label class="label">目标磁盘</label>
          <select v-model="targetDisk" :disabled="busy">
            <option v-for="d in targetOptions" :key="d.drive_letter" :value="`${d.drive_letter}\\`">
              {{ d.drive_letter }} · {{ d.label || 'Local Disk' }} (可用 {{ formatBytes(d.free_space) }})
            </option>
          </select>
        </template>

        <div v-if="hasDelete && !hasMigrate" class="delete-hint" :class="{ danger: deleteMode === 'permanent' }">
          <strong>{{ deleteMode === 'permanent' ? '永久删除模式' : '清理走回收站' }}</strong>
          <span>{{ deleteMode === 'permanent' ? '跳过回收站，删除后无法恢复' : '可在 Windows 回收站还原' }}</span>
        </div>

        <div class="actions">
          <button class="ghost" @click="clearCart" :disabled="busy">全部清空</button>
          <button
            class="primary"
            :class="{ danger: hasDelete && deleteMode === 'permanent' && !hasMigrate }"
            @click="start"
            :disabled="primaryDisabled"
          >
            {{ primaryLabel }}
          </button>
        </div>
      </footer>
    </aside>
  </Teleport>
</template>

<style scoped>
.cart-fab {
  position: fixed;
  right: 1.4rem;
  bottom: 1.4rem;
  display: inline-flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.7rem 1.1rem;
  border: none;
  border-radius: 999px;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-family: var(--font-sans);
  cursor: pointer;
  box-shadow: 0 14px 28px rgba(17, 24, 39, 0.22), 0 4px 8px rgba(17, 24, 39, 0.14);
  z-index: 1500;
  transition: transform var(--transition-base), box-shadow var(--transition-base);
}
.cart-fab:hover { transform: translateY(-1px); box-shadow: 0 20px 36px rgba(17, 24, 39, 0.28); }
.cart-fab.open { transform: translateY(-1px); }

.cart-icon { font-size: 1.05rem; }
.cart-stats { display: flex; align-items: baseline; gap: 0.4rem; line-height: 1; }
.cart-stats strong {
  font-size: 1.05rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  transition: transform 0.18s var(--transition-spring, cubic-bezier(0.16, 1, 0.3, 1));
}
.cart-stats small { font-size: 0.78rem; opacity: 0.8; }

.cart-overlay {
  position: fixed; inset: 0;
  background: rgba(15, 23, 32, 0.32);
  z-index: 1600;
  animation: fadeIn 0.2s ease;
}

.cart-drawer {
  position: fixed;
  top: 0; right: 0; bottom: 0;
  width: min(420px, 100vw);
  background: var(--color-bg-secondary);
  border-left: 1px solid var(--color-border-light);
  box-shadow: -32px 0 80px rgba(17, 24, 39, 0.2);
  display: flex;
  flex-direction: column;
  transform: translateX(100%);
  transition: transform 0.32s cubic-bezier(0.16, 1, 0.3, 1);
  z-index: 1700;
}
.cart-drawer.open { transform: translateX(0); }

.drawer-head {
  display: flex; justify-content: space-between; align-items: center;
  padding: 1.2rem 1.4rem;
  border-bottom: 1px solid var(--color-border-light);
}
.drawer-head h3 { font-size: 1.15rem; font-weight: 700; }
.drawer-head p { margin-top: 0.18rem; font-size: 0.82rem; color: var(--color-text-tertiary); }

.icon-btn {
  width: 32px; height: 32px;
  border: 1px solid var(--color-border-light);
  border-radius: 10px;
  background: var(--color-surface);
  color: var(--color-text-secondary);
  cursor: pointer;
  display: inline-flex; align-items: center; justify-content: center;
  transition: background var(--transition-fast);
}
.icon-btn:hover { background: var(--color-surface-hover); color: var(--color-text-primary); }
.icon-btn.small { width: 24px; height: 24px; border-radius: 8px; font-size: 0.95rem; }

.drawer-list-wrap {
  flex: 1;
  min-height: 0;
}

.drawer-item {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.7rem;
  padding: 0.7rem 1.4rem;
  border-bottom: 1px solid var(--color-border-light);
  height: 64px;
  box-sizing: border-box;
}

.drawer-item-main { min-width: 0; }
.drawer-item-name { font-weight: 600; color: var(--color-text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.drawer-item-path { font-size: 0.74rem; color: var(--color-text-tertiary); font-family: var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 0.18rem; }
.drawer-item-size { font-size: 0.84rem; font-weight: 600; color: var(--color-text-secondary); font-feature-settings: 'tnum'; }

.drawer-foot {
  padding: 1rem 1.4rem 1.3rem;
  border-top: 1px solid var(--color-border-light);
  background: var(--color-surface);
}
.label {
  display: block;
  font-size: 0.74rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-text-tertiary);
  margin-bottom: 0.4rem;
}
select {
  width: 100%;
  padding: 0.65rem 0.85rem;
  border-radius: 12px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  font-size: 0.9rem;
  color: var(--color-text-primary);
}

.actions { display: flex; gap: 0.55rem; margin-top: 0.85rem; }
.ghost, .primary {
  flex: 1;
  padding: 0.78rem 1rem;
  border-radius: 14px;
  border: none;
  font-weight: 700;
  cursor: pointer;
  transition: transform var(--transition-fast), opacity var(--transition-fast), box-shadow var(--transition-fast);
}
.ghost { background: var(--color-surface-strong); color: var(--color-text-secondary); border: 1px solid var(--color-border-medium); }
.ghost:hover:not(:disabled) { background: var(--color-surface-hover); color: var(--color-text-primary); }
.primary {
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  box-shadow: 0 12px 24px rgba(17, 24, 39, 0.22);
}
.primary:hover:not(:disabled) { transform: translateY(-1px); box-shadow: 0 16px 30px rgba(17, 24, 39, 0.28); }
.primary:disabled, .ghost:disabled { opacity: 0.5; cursor: not-allowed; }

.primary.danger {
  background: linear-gradient(135deg, var(--color-error, #ef4444) 0%, #dc2626 100%);
  color: #ffffff;
  box-shadow: 0 12px 24px rgba(239, 68, 68, 0.3);
}
.primary.danger:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 16px 30px rgba(239, 68, 68, 0.4);
}

.segments {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.85rem 1.4rem;
  border-bottom: 1px solid var(--color-border-light);
  background: var(--color-surface);
}
.segment {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.55rem 0.7rem;
  border-radius: 12px;
  border: 1px solid var(--color-border-light);
  background: var(--color-bg-secondary);
}
.segment-migrate {
  background: rgba(59, 130, 246, 0.06);
  border-color: rgba(59, 130, 246, 0.18);
}
.segment-delete {
  background: rgba(245, 158, 11, 0.06);
  border-color: rgba(245, 158, 11, 0.2);
}
.segment-delete.permanent {
  background: rgba(239, 68, 68, 0.06);
  border-color: rgba(239, 68, 68, 0.22);
}
.segment-icon { font-size: 1.05rem; line-height: 1; }
.segment-text { display: flex; flex-direction: column; line-height: 1.2; min-width: 0; }
.segment-text strong { font-size: 0.88rem; font-weight: 700; color: var(--color-text-primary); }
.segment-text small { font-size: 0.74rem; color: var(--color-text-tertiary); margin-top: 0.18rem; }

.delete-hint {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
  padding: 0.7rem 0.85rem;
  border-radius: 12px;
  background: rgba(245, 158, 11, 0.08);
  border: 1px solid rgba(245, 158, 11, 0.2);
}
.delete-hint.danger {
  background: rgba(239, 68, 68, 0.08);
  border-color: rgba(239, 68, 68, 0.24);
}
.delete-hint strong { font-size: 0.86rem; font-weight: 700; color: var(--color-text-primary); }
.delete-hint span { font-size: 0.76rem; color: var(--color-text-tertiary); }

.drawer-item-tag {
  display: inline-block;
  margin-right: 0.4rem;
  padding: 0.05rem 0.4rem;
  border-radius: 6px;
  font-size: 0.7rem;
  font-weight: 700;
  vertical-align: 1px;
}
.tag-migrate { background: rgba(59, 130, 246, 0.14); color: #1d4ed8; }
.tag-delete { background: rgba(245, 158, 11, 0.18); color: #b45309; }
.tag-review { background: rgba(107, 114, 128, 0.14); color: #4b5563; }

.busy-panel {
  padding: 1.4rem 1.4rem;
  display: flex;
  flex-direction: column;
  gap: 0.8rem;
  flex: 1;
}
.progress-bar {
  height: 6px;
  border-radius: 999px;
  background: rgba(15, 23, 32, 0.08);
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--color-highlight), var(--color-info));
  border-radius: 999px;
  transition: width 0.3s ease;
}
.progress-meta {
  display: flex;
  justify-content: space-between;
  gap: 0.8rem;
  font-size: 0.84rem;
  color: var(--color-text-secondary);
}
.progress-current {
  font-family: var(--font-mono);
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  text-align: right;
}

.results-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.2rem 1.4rem 1.3rem;
  min-height: 0;
}
.results-summary {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.6rem;
}
.results-stat {
  padding: 0.85rem;
  border-radius: 14px;
  border: 1px solid var(--color-border-light);
  text-align: center;
}
.results-stat.ok { background: rgba(15, 159, 110, 0.08); }
.results-stat.fail { background: rgba(220, 38, 38, 0.08); }
.results-stat strong {
  display: block;
  font-size: 1.6rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
  line-height: 1;
}
.results-stat small {
  display: block;
  margin-top: 0.32rem;
  font-size: 0.76rem;
  color: var(--color-text-tertiary);
}

.results-failed {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}
.results-title {
  font-size: 0.74rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-text-tertiary);
}
.results-failed ul { list-style: none; display: flex; flex-direction: column; gap: 0.5rem; }
.result-fail-item {
  padding: 0.7rem 0.8rem;
  border-radius: 12px;
  background: rgba(220, 38, 38, 0.05);
  border: 1px solid rgba(220, 38, 38, 0.14);
}
.result-fail-name { font-weight: 600; color: var(--color-text-primary); font-size: 0.9rem; }
.result-fail-error {
  margin-top: 0.28rem;
  font-size: 0.82rem;
  color: #b91c1c;
  line-height: 1.5;
}
.result-fail-path {
  margin-top: 0.32rem;
  font-family: var(--font-mono);
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
  word-break: break-all;
}

.dismiss-btn {
  margin-top: auto;
  padding: 0.78rem 1rem;
  border-radius: 14px;
  border: none;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-weight: 700;
  cursor: pointer;
}
.dismiss-btn:hover { transform: translateY(-1px); }

@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
</style>
