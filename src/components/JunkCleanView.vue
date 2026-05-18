<script setup lang="ts">
import { ref, computed, onBeforeUnmount, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ElCollapse, ElCollapseItem, ElCheckbox, ElTag } from 'element-plus';
import 'element-plus/es/components/collapse/style/css';
import 'element-plus/es/components/collapse-item/style/css';
import 'element-plus/es/components/checkbox/style/css';
import 'element-plus/es/components/tag/style/css';
import type { JunkScanResult, JunkItem, JunkCleanResult, JunkCategory } from '../types/junk';
import { CATEGORY_LABELS, CMD_SCAN_JUNK, CMD_CLEAN_JUNK } from '../types/junk';
import { formatBytes } from '../utils/format';
import IconSpinner from './icons/scan/IconSpinner.vue';
import IconRiskSafe from './icons/status/IconRiskSafe.vue';
import IconRiskMedium from './icons/status/IconRiskMedium.vue';
import IconRiskDanger from './icons/status/IconRiskDanger.vue';
import ConfirmDialog from './ConfirmDialog.vue';
import JunkSkippedRulesDialog from './JunkSkippedRulesDialog.vue';
import JunkPresets from './JunkPresets.vue';
import MiddlePath from './MiddlePath.vue';
import VirtualList from './VirtualList.vue';
import { useToast } from '../composables/useToast';
import { useJunkScanCache } from '../composables/useJunkScanCache';
import { useSelectionSet } from '../composables/useSelectionSet';
import { IconSuccess } from './icons';

type Status = 'idle' | 'scanning' | 'scanned' | 'cleaning';

interface JunkScanProgress {
  current_rule: string;
  current_rule_name: string;
  completed_rules: number;
  total_rules: number;
  found_items: number;
  found_size: number;
}

const status = ref<Status>('idle');
const result = shallowRef<JunkScanResult | null>(null);
const checked = useSelectionSet<string>();
const activeCollapse = ref<string[]>([]);
const lastCategories = ref<JunkCategory[] | null>(null);
const scanProgress = ref<JunkScanProgress | null>(null);
const cache = useJunkScanCache();
let unlistenProgress: UnlistenFn | null = null;

const showToast = useToast();

interface CategoryGroup {
  category: JunkCategory;
  label: string;
  items: JunkItem[];
  totalSize: number;
  defaultSelectedCount: number;
}

interface GroupSummary {
  total: number;
  checked: number;
  totalSize: number;
  checkedSize: number;
  all: boolean;
  some: boolean;
}

// 按 path 分类的索引：path -> { item, group }
const itemIndex = new Map<string, { item: JunkItem; group: CategoryGroup }>();

// selectedSize 增量维护：每个写入点直接 +/- delta，避免遍历整个已选集合。
// `replace`/`clear` 会走 `recomputeSelectedSize()` 做一次性重算。
let selectedSizeAcc = 0;
const selectedSizeRef = ref(0);

function bumpSize(delta: number) {
  if (delta === 0) return;
  selectedSizeAcc += delta;
  if (selectedSizeAcc < 0) selectedSizeAcc = 0;
  selectedSizeRef.value = selectedSizeAcc;
}

function recomputeSelectedSize() {
  let size = 0;
  for (const p of checked.value) {
    const idx = itemIndex.get(p);
    if (idx) size += idx.item.size;
  }
  selectedSizeAcc = size;
  selectedSizeRef.value = size;
}

function sizeOfPath(path: string): number {
  return itemIndex.get(path)?.item.size ?? 0;
}

const groups = computed<CategoryGroup[]>(() => {
  const r = result.value;
  if (!r) return [];
  const map = new Map<JunkCategory, CategoryGroup>();
  for (const item of r.items) {
    let g = map.get(item.category);
    if (!g) {
      g = {
        category: item.category,
        label: CATEGORY_LABELS[item.category] ?? item.category_name ?? item.category,
        items: [],
        totalSize: 0,
        defaultSelectedCount: 0,
      };
      map.set(item.category, g);
    }
    g.items.push(item);
    g.totalSize += item.size;
    if (item.default_selected) g.defaultSelectedCount += 1;
  }
  return Array.from(map.values()).sort((a, b) => b.totalSize - a.totalSize);
});

// 模板里通过 Map 直接读取分组汇总，避免多次 groupCheckedCount(g) 函数调用。
const groupSummaries = computed<Map<JunkCategory, GroupSummary>>(() => {
  // 依赖：groups + checked.value
  const checkedSet = checked.value;
  const map = new Map<JunkCategory, GroupSummary>();
  const list = groups.value;
  for (const g of list) {
    let n = 0;
    let cs = 0;
    for (const it of g.items) {
      if (checkedSet.has(it.path)) {
        n += 1;
        cs += it.size;
      }
    }
    const total = g.items.length;
    map.set(g.category, {
      total,
      checked: n,
      totalSize: g.totalSize,
      checkedSize: cs,
      all: total > 0 && n === total,
      some: n > 0 && n < total,
    });
  }
  return map;
});

const selectedCount = checked.sizeRef;
const selectedSize = computed(() => selectedSizeRef.value);

const totalCleanableLabel = computed(() => {
  if (!result.value) return '';
  return formatBytes(result.value.total_size);
});

function rebuildItemIndex(items: JunkItem[]) {
  itemIndex.clear();
  // 临时：构建一个 path -> item 映射，分组在 groups computed 计算时填充
  // 简化处理：先构建 path -> item，group 字段由 groups computed 的下一次访问负责对齐
  // 但因 groupSummaries 不依赖 itemIndex.group，仅 selectedSize 用 itemIndex.item.size 即可
  for (const it of items) {
    itemIndex.set(it.path, { item: it, group: undefined as unknown as CategoryGroup });
  }
}

function applyDefaultSelection(items: JunkItem[]) {
  const next: string[] = [];
  for (const item of items) {
    if (item.default_selected) next.push(item.path);
  }
  checked.replace(next);
  recomputeSelectedSize();
}

function applyDefaultExpansion() {
  const top3 = [...groups.value]
    .sort((a, b) => b.defaultSelectedCount - a.defaultSelectedCount)
    .slice(0, 3)
    .map((g) => g.category as string);
  activeCollapse.value = top3;
}

async function startScan(categories: JunkCategory[] | null = lastCategories.value) {
  if (status.value === 'scanning' || status.value === 'cleaning') return;

  const cached = cache.get(categories);
  if (cached) {
    result.value = cached.result;
    rebuildItemIndex(cached.result.items);
    applyDefaultSelection(cached.result.items);
    applyDefaultExpansion();
    status.value = 'scanned';
    lastCategories.value = categories;
    const ageSec = Math.max(1, Math.round((Date.now() - cached.cachedAt) / 1000));
    showToast('使用缓存结果', `${ageSec} 秒前的扫描，点"重新扫描"刷新`, 'info');
    return;
  }

  status.value = 'scanning';
  result.value = null;
  itemIndex.clear();
  selectedSizeAcc = 0;
  selectedSizeRef.value = 0;
  checked.clear();
  activeCollapse.value = [];
  lastCategories.value = categories;
  scanProgress.value = null;

  try {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    unlistenProgress = await listen<JunkScanProgress>('junk-scan-progress', (e) => {
      scanProgress.value = e.payload;
    });
  } catch {
    void 0;
  }

  try {
    const r = await invoke<JunkScanResult>(CMD_SCAN_JUNK, { categories });
    result.value = r;
    rebuildItemIndex(r.items);
    cache.set(categories, r);
    applyDefaultSelection(r.items);
    applyDefaultExpansion();
    status.value = 'scanned';
    if (r.items.length === 0) {
      showToast('没有发现垃圾', '系统看起来很干净', 'success');
    } else {
      showToast(
        '扫描完成',
        `${r.total_count} 项 · 约 ${formatBytes(r.total_size)}`,
        'success',
      );
    }
  } catch (err) {
    status.value = 'idle';
    showToast('扫描失败', String(err), 'error');
  } finally {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    scanProgress.value = null;
  }
}

function onRescanClick() {
  cache.clear();
  void startScan();
}

onBeforeUnmount(() => {
  if (unlistenProgress) {
    unlistenProgress();
    unlistenProgress = null;
  }
});

function onPresetApply(categories: JunkCategory[] | null) {
  void startScan(categories);
}

function toggleItem(path: string, on: boolean | string | number) {
  if (on) {
    if (checked.add(path)) bumpSize(sizeOfPath(path));
  } else {
    if (checked.delete(path)) bumpSize(-sizeOfPath(path));
  }
}

function isChecked(path: string): boolean {
  return checked.has(path);
}

function selectAll() {
  if (!result.value) return;
  const all: string[] = [];
  for (const item of result.value.items) all.push(item.path);
  checked.replace(all);
  recomputeSelectedSize();
}

function selectNone() {
  checked.clear();
  selectedSizeAcc = 0;
  selectedSizeRef.value = 0;
}

function selectSafeOnly() {
  if (!result.value) return;
  const safeList: string[] = [];
  for (const item of result.value.items) {
    if (item.risk_level === 'safe') safeList.push(item.path);
  }
  checked.replace(safeList);
  recomputeSelectedSize();
}

function toggleGroup(group: CategoryGroup, on: boolean | string | number) {
  let delta = 0;
  if (on) {
    for (const item of group.items) {
      if (checked.add(item.path)) delta += item.size;
    }
  } else {
    for (const item of group.items) {
      if (checked.delete(item.path)) delta -= item.size;
    }
  }
  bumpSize(delta);
}

function riskLabel(risk: JunkItem['risk_level']): string {
  if (risk === 'safe') return '安全';
  if (risk === 'caution') return '注意';
  return '风险';
}

function riskTagType(risk: JunkItem['risk_level']): 'success' | 'warning' | 'danger' {
  if (risk === 'safe') return 'success';
  if (risk === 'caution') return 'warning';
  return 'danger';
}

function riskIconFor(risk: JunkItem['risk_level']) {
  if (risk === 'safe') return IconRiskSafe;
  if (risk === 'caution') return IconRiskMedium;
  return IconRiskDanger;
}

const confirmOpen = ref(false);
const confirmMode = ref<'recycle' | 'permanent'>('recycle');

const skipDialogOpen = ref(false);

function openSkipDialog() {
  if (!result.value || result.value.skipped_rules.length === 0) return;
  skipDialogOpen.value = true;
}

function closeSkipDialog() {
  skipDialogOpen.value = false;
}

async function rescanFromSkipDialog() {
  skipDialogOpen.value = false;
  cache.clear();
  await startScan();
}

const confirmTitle = computed(() =>
  confirmMode.value === 'permanent' ? '永久删除' : '确认清理',
);

const confirmMessage = computed(() => {
  const n = checked.size;
  if (confirmMode.value === 'permanent') {
    return `将永久删除 ${n} 项，无法恢复。`;
  }
  return `将清理 ${n} 项垃圾到 Windows 回收站，可从回收站还原。`;
});

const confirmType = computed<'danger' | 'warning'>(() =>
  confirmMode.value === 'permanent' ? 'danger' : 'warning',
);

const confirmText = computed(() =>
  confirmMode.value === 'permanent' ? '永久删除' : '清理到回收站',
);

function openConfirm(mode: 'recycle' | 'permanent') {
  if (checked.size === 0) return;
  if (status.value === 'cleaning' || status.value === 'scanning') return;
  confirmMode.value = mode;
  confirmOpen.value = true;
}

function cancelConfirm() {
  confirmOpen.value = false;
}

async function performClean() {
  const paths = Array.from(checked.value);
  const toRecycleBin = confirmMode.value === 'recycle';
  confirmOpen.value = false;
  if (paths.length === 0) return;
  status.value = 'cleaning';
  try {
    const r = await invoke<JunkCleanResult>(CMD_CLEAN_JUNK, {
      paths,
      toRecycleBin,
    });
    if (r.success) {
      showToast(
        '清理完成',
        `已释放 ${formatBytes(r.cleaned_size)} · ${r.cleaned_count} 项`,
        'success',
      );
    } else if (r.cleaned_count > 0) {
      showToast(
        '部分清理失败',
        `${r.failed_count} 项失败，已释放 ${formatBytes(r.cleaned_size)}`,
        'warning',
      );
    } else {
      showToast('清理失败', `${r.failed_count} 项均失败`, 'error');
    }
  } catch (err) {
    showToast('清理失败', String(err), 'error');
  } finally {
    status.value = 'scanned';
    checked.clear();
    selectedSizeAcc = 0;
    selectedSizeRef.value = 0;
    cache.clear();
    await startScan();
  }
}

// 当 result 变化时同步 itemIndex（确保 setFromCache 也同步）
watch(
  () => result.value,
  (r) => {
    if (r) rebuildItemIndex(r.items);
  },
);

function summaryOf(category: JunkCategory): GroupSummary {
  return (
    groupSummaries.value.get(category) ?? {
      total: 0,
      checked: 0,
      totalSize: 0,
      checkedSize: 0,
      all: false,
      some: false,
    }
  );
}

function isGroupActive(category: JunkCategory): boolean {
  return activeCollapse.value.includes(category);
}
</script>

<template>
  <div class="junk">
    <header class="junk-head">
      <div class="junk-head-copy">
        <h3>垃圾清理</h3>
        <p v-if="status === 'idle'">选择一种清理强度，开始扫描 Windows 系统临时文件、浏览器缓存、更新残留等。</p>
        <p v-else-if="status === 'scanned' && result">
          共发现 <strong>{{ result.total_count }}</strong> 项垃圾，约可释放
          <strong>{{ totalCleanableLabel }}</strong>
        </p>
        <p v-else-if="status === 'scanning'">正在扫描垃圾文件…</p>
        <p v-else-if="status === 'cleaning'">正在清理…</p>
      </div>
      <div class="junk-head-actions">
        <button
          v-if="status === 'scanned'"
          class="junk-btn junk-btn--ghost"
          @click="onRescanClick"
        >
          重新扫描
        </button>
        <button
          v-else-if="status === 'scanning' || status === 'cleaning'"
          class="junk-btn junk-btn--primary"
          disabled
        >
          <IconSpinner :size="16" />
          <span>{{ status === 'cleaning' ? '清理中…' : '扫描中…' }}</span>
        </button>
      </div>
    </header>

    <JunkPresets v-if="status === 'idle'" @apply="onPresetApply" />

    <div v-if="status === 'scanning'" class="junk-scanning">
      <IconSpinner :size="28" />
      <div class="junk-scanning-copy">
        <strong>正在扫描垃圾文件…</strong>
        <small v-if="scanProgress">
          {{ scanProgress.current_rule_name || scanProgress.current_rule }} · 已找到
          {{ scanProgress.found_items }} 项 · {{ formatBytes(scanProgress.found_size) }}
        </small>
        <small v-else>正在初始化扫描…</small>
        <div class="junk-progress" v-if="scanProgress && scanProgress.total_rules > 0">
          <div
            class="junk-progress-bar"
            :style="{ width: ((scanProgress.completed_rules / scanProgress.total_rules) * 100).toFixed(1) + '%' }"
          ></div>
          <span class="junk-progress-text">
            {{ scanProgress.completed_rules }} / {{ scanProgress.total_rules }}
          </span>
        </div>
      </div>
    </div>

    <template v-else-if="status === 'scanned' && result && result.items.length > 0">
      <div class="junk-toolbar">
        <div class="junk-toolbar-left">
          <button class="junk-btn junk-btn--mini" @click="selectAll">全选</button>
          <button class="junk-btn junk-btn--mini junk-btn--ghost" @click="selectNone">全不选</button>
          <button class="junk-btn junk-btn--mini junk-btn--ghost" @click="selectSafeOnly">仅选安全</button>
        </div>
        <div class="junk-toolbar-right" v-if="result.skipped_rules.length > 0">
          <button class="junk-skip-btn" type="button" @click="openSkipDialog">
            已跳过 {{ result.skipped_rules.length }} 条规则 · 查看
          </button>
        </div>
      </div>

      <ElCollapse v-model="activeCollapse" class="junk-collapse">
        <ElCollapseItem
          v-for="g in groups"
          :key="g.category"
          :name="g.category"
        >
          <template #title>
            <div class="junk-group-title" @click.stop>
              <ElCheckbox
                :model-value="summaryOf(g.category).all"
                :indeterminate="summaryOf(g.category).some"
                @update:model-value="(v) => toggleGroup(g, v)"
                @click.stop
              />
              <strong>{{ g.label }}</strong>
              <span class="junk-group-meta">{{ g.items.length }} 项 · {{ formatBytes(g.totalSize) }}</span>
            </div>
          </template>

          <div
            v-if="isGroupActive(g.category)"
            class="junk-list-shell"
            :style="{ height: Math.min(g.items.length, 10) * 64 + 'px' }"
          >
            <VirtualList
              :items="g.items"
              :item-size="64"
              :buffer="6"
              v-slot="{ item }"
            >
              <div :key="(item as JunkItem).path" class="junk-row">
                <ElCheckbox
                  class="junk-row-check"
                  :model-value="isChecked((item as JunkItem).path)"
                  @update:model-value="(v) => toggleItem((item as JunkItem).path, v)"
                />
                <div class="junk-row-main">
                  <div class="junk-row-name">{{ (item as JunkItem).rule_name }}</div>
                  <MiddlePath class="junk-row-path" :path="(item as JunkItem).path" />
                </div>
                <div class="junk-row-meta">
                  <span class="junk-row-count">{{ (item as JunkItem).file_count }} 个文件</span>
                  <span class="junk-row-size">{{ formatBytes((item as JunkItem).size) }}</span>
                  <ElTag
                    class="junk-row-risk"
                    :type="riskTagType((item as JunkItem).risk_level)"
                    size="small"
                    effect="light"
                    round
                  >
                    <span class="junk-risk-inner">
                      <component :is="riskIconFor((item as JunkItem).risk_level)" :size="12" />
                      <span>{{ riskLabel((item as JunkItem).risk_level) }}</span>
                    </span>
                  </ElTag>
                </div>
              </div>
            </VirtualList>
          </div>
        </ElCollapseItem>
      </ElCollapse>
    </template>

    <div
      v-else-if="status === 'scanned' && result && result.items.length === 0"
      class="junk-empty"
    >
      <div class="junk-empty-mark"><IconSuccess :size="28" /></div>
      <h4>没有发现垃圾文件</h4>
      <p>系统已经很干净了，可以稍后再来扫描。</p>
    </div>

    <footer
      v-if="status !== 'idle' && status !== 'scanning' && result && result.items.length > 0"
      class="junk-foot"
    >
      <div class="junk-foot-stats">
        <strong>已选 {{ selectedCount }} 项</strong>
        <span>{{ formatBytes(selectedSize) }}</span>
      </div>
      <div class="junk-foot-actions">
        <button
          class="junk-btn junk-btn--primary"
          :disabled="selectedCount === 0 || status === 'cleaning'"
          @click="openConfirm('recycle')"
        >
          清理到回收站
        </button>
        <button
          class="junk-btn junk-btn--danger"
          :disabled="selectedCount === 0 || status === 'cleaning'"
          @click="openConfirm('permanent')"
        >
          永久删除
        </button>
      </div>
    </footer>

    <ConfirmDialog
      :show="confirmOpen"
      :title="confirmTitle"
      :message="confirmMessage"
      :type="confirmType"
      :confirm-text="confirmText"
      cancel-text="取消"
      @confirm="performClean"
      @cancel="cancelConfirm"
    />

    <JunkSkippedRulesDialog
      :show="skipDialogOpen"
      :rule-ids="result?.skipped_rules ?? []"
      @close="closeSkipDialog"
      @rescan="rescanFromSkipDialog"
    />
  </div>
</template>

<style scoped>
.junk {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  height: 100%;
  min-height: 0;
}

.junk-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.junk-head-copy h3 {
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.junk-head-copy p {
  margin-top: 0.18rem;
  font-size: 0.85rem;
  color: var(--color-text-tertiary);
}

.junk-head-copy strong {
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.junk-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
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

.junk-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.junk-btn:active:not(:disabled) {
  transform: scale(0.97);
}

.junk-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.junk-btn--lg {
  padding: 0.85rem 1.4rem;
  font-size: 0.95rem;
  border-radius: var(--radius-md);
}

.junk-btn--primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  border-color: transparent;
  box-shadow: 0 10px 22px rgba(15, 118, 110, 0.22);
}

.junk-btn--primary:hover:not(:disabled) {
  box-shadow: 0 14px 26px rgba(15, 118, 110, 0.3);
}

.junk-btn--ghost {
  background: transparent;
}

.junk-btn--mini {
  padding: 0.32rem 0.7rem;
  font-size: 0.78rem;
  border-radius: var(--radius-xs);
}

.junk-btn--danger {
  background: var(--color-error);
  color: var(--color-text-inverse);
  border-color: transparent;
  box-shadow: 0 10px 22px rgba(220, 38, 38, 0.24);
}

.junk-btn--danger:hover:not(:disabled) {
  box-shadow: 0 14px 28px rgba(220, 38, 38, 0.32);
}

.junk-scanning {
  display: flex;
  align-items: center;
  gap: 0.85rem;
  padding: 1.5rem;
  border-radius: var(--radius-md);
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  color: var(--color-highlight);
}

.junk-scanning-copy {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.junk-scanning-copy strong {
  color: var(--color-text-primary);
  font-size: 0.95rem;
}

.junk-scanning-copy small {
  color: var(--color-text-tertiary);
  font-size: 0.8rem;
}

.junk-progress {
  position: relative;
  margin-top: 0.5rem;
  height: 6px;
  border-radius: 999px;
  background: rgba(15, 23, 32, 0.08);
  overflow: hidden;
}

.junk-progress-bar {
  height: 100%;
  background: linear-gradient(90deg, var(--color-highlight), var(--color-info));
  border-radius: 999px;
  transition: width 0.3s ease;
}

.junk-progress-text {
  position: absolute;
  top: 50%;
  right: 0.4rem;
  transform: translateY(-50%);
  font-size: 0.7rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
  background: var(--color-bg-secondary, rgba(255, 255, 255, 0.7));
  padding: 0 0.3rem;
  border-radius: 4px;
}

.junk-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.6rem;
  padding: 0.5rem 0.2rem;
}

.junk-toolbar-left {
  display: flex;
  gap: 0.45rem;
}

.junk-skip-btn {
  border: 1px solid var(--color-border-medium);
  background: transparent;
  color: var(--color-text-secondary);
  padding: 0.32rem 0.7rem;
  border-radius: var(--radius-xs);
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
}

.junk-skip-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  border-color: var(--color-border-strong);
}

.junk-collapse {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border-light);
  background: var(--color-surface-strong);
}

.junk-group-title {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  width: 100%;
}

.junk-group-title strong {
  color: var(--color-text-primary);
  font-weight: 700;
  font-size: 0.92rem;
}

.junk-group-meta {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
}

.junk-list-shell {
  background: var(--color-bg-secondary);
  min-height: 64px;
}

.junk-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.7rem;
  padding: 0.55rem 1rem;
  border-top: 1px solid var(--color-border-light);
  height: 64px;
  box-sizing: border-box;
}

.junk-row-check {
  margin-right: 0;
}

.junk-row-main {
  min-width: 0;
}

.junk-row-name {
  font-weight: 700;
  color: var(--color-text-primary);
  font-size: 0.88rem;
}

.junk-row-path {
  margin-top: 0.2rem;
  font-size: 0.75rem;
  color: var(--color-text-tertiary);
  min-width: 0;
}

.junk-row-path :deep(.mp) {
  color: inherit;
  font-size: inherit;
}

.junk-row-meta {
  display: inline-flex;
  align-items: center;
  gap: 0.65rem;
  font-feature-settings: 'tnum';
}

.junk-row-count {
  font-size: 0.76rem;
  color: var(--color-text-tertiary);
}

.junk-row-size {
  font-size: 0.85rem;
  font-weight: 700;
  color: var(--color-text-secondary);
  min-width: 5rem;
  text-align: right;
}

.junk-risk-inner {
  display: inline-flex;
  align-items: center;
  gap: 0.28rem;
  line-height: 1;
}

.junk-risk-inner svg {
  display: block;
  flex-shrink: 0;
}

.junk-empty {
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

.junk-empty-mark {
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

.junk-empty h4 {
  font-size: 1.05rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.junk-empty p {
  font-size: 0.85rem;
  color: var(--color-text-tertiary);
}

.junk-foot {
  position: sticky;
  bottom: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 0.85rem 1rem;
  padding-right: 6rem;
  border-radius: var(--radius-md);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.92), var(--color-surface-strong));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-sm);
}

.junk-foot-stats {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
}

.junk-foot-stats strong {
  font-family: var(--font-display);
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.junk-foot-stats span {
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
}

.junk-foot-actions {
  display: flex;
  gap: 0.5rem;
}
</style>
