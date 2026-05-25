<script setup lang="ts">
import { ref, computed, onBeforeUnmount, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { ElCollapse, ElCollapseItem, ElCheckbox } from 'element-plus';
import 'element-plus/es/components/collapse/style/css';
import 'element-plus/es/components/collapse-item/style/css';
import 'element-plus/es/components/checkbox/style/css';
import type {
  JunkScanResult,
  JunkItem,
  JunkCleanResult,
  JunkCategory,
  JunkCleanError,
  DryRunReport,
  DryRunInput,
  JunkFeedback,
  JunkRiskLevel,
} from '../types/junk';

import {
  CATEGORY_LABELS,
  CMD_SCAN_JUNK,
  CMD_CLEAN_JUNK,
  CMD_DRY_RUN_JUNK,
  CMD_REPORT_JUNK_FEEDBACK,
} from '../types/junk';
import { formatBytes } from '../utils/format';
import IconSpinner from './icons/scan/IconSpinner.vue';
import ConfirmDialog from './ConfirmDialog.vue';
import JunkSkippedRulesDialog from './JunkSkippedRulesDialog.vue';
import JunkPresets from './JunkPresets.vue';
import MiddlePath from './MiddlePath.vue';
import VirtualList from './VirtualList.vue';
import FeatureIntro from './FeatureIntro.vue';
import RiskBadge from './RiskBadge.vue';
import WhyCleanable from './WhyCleanable.vue';
import DryRunPreview from './DryRunPreview.vue';
import { stateCopy } from '../utils/state-copy';
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

interface JunkCleanProgress {
  current_path: string;
  completed_items: number;
  total_items: number;
  cleaned_size: number;
  total_size: number;
  current_deleted_files: number;
  current_total_files: number;
  current_file: string;
  progress_percent: number;
  error_count: number;
}

const status = ref<Status>('idle');
const result = shallowRef<JunkScanResult | null>(null);
const checked = useSelectionSet<string>();
const activeCollapse = ref<string[]>([]);
const lastCategories = ref<JunkCategory[] | null>(null);
const scanProgress = ref<JunkScanProgress | null>(null);
const cleanProgress = ref<JunkCleanProgress | null>(null);
const lastCleanResult = ref<JunkCleanResult | null>(null);
const showAllCleanErrors = ref(false);
const cache = useJunkScanCache();
let unlistenProgress: UnlistenFn | null = null;
let unlistenCleanProgress: UnlistenFn | null = null;

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

interface CleanErrorView {
  path: string;
  error: string;
  reason: string;
  suggestion: string;
}

interface CleanErrorGroup {
  reason: string;
  suggestion: string;
  count: number;
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

const cleanErrors = computed<CleanErrorView[]>(() =>
  (lastCleanResult.value?.errors ?? []).map((err) => ({
    path: err.path,
    error: err.error,
    reason: cleanErrorReason(err),
    suggestion: cleanErrorSuggestion(err),
  })),
);

const cleanErrorGroups = computed<CleanErrorGroup[]>(() => {
  const map = new Map<string, CleanErrorGroup>();
  for (const err of cleanErrors.value) {
    const key = `${err.reason}\n${err.suggestion}`;
    const existing = map.get(key);
    if (existing) {
      existing.count += 1;
    } else {
      map.set(key, { reason: err.reason, suggestion: err.suggestion, count: 1 });
    }
  }
  return Array.from(map.values()).sort((a, b) => b.count - a.count);
});

const visibleCleanErrors = computed(() =>
  showAllCleanErrors.value ? cleanErrors.value : cleanErrors.value.slice(0, 8),
);

const hiddenCleanErrorCount = computed(() =>
  Math.max(0, cleanErrors.value.length - visibleCleanErrors.value.length),
);

const cleanResultTone = computed<'success' | 'warning' | 'danger'>(() => {
  const r = lastCleanResult.value;
  if (!r || cleanErrors.value.length === 0) return 'success';
  if (r.cleaned_count > 0 || r.cleaned_size > 0) return 'warning';
  return 'danger';
});

const cleanResultTitle = computed(() => {
  const r = lastCleanResult.value;
  if (!r) return '';
  if (cleanErrors.value.length === 0) return '清理完成';
  if (r.cleaned_count > 0 || r.cleaned_size > 0) return '部分清理完成';
  return '清理未完成';
});

const cleanResultDescription = computed(() => {
  const r = lastCleanResult.value;
  if (!r) return '';
  const failed = cleanErrors.value.length;
  if (failed === 0) {
    return `已处理 ${r.cleaned_count} 个文件，释放 ${formatBytes(r.cleaned_size)}。`;
  }
  return `已处理 ${r.cleaned_count} 个文件，释放 ${formatBytes(r.cleaned_size)}；${failed} 个路径未能处理。`;
});

function cleanErrorReason(err: JunkCleanError): string {
  if (err.reason) return err.reason;
  const text = `${err.error} ${err.path}`.toLowerCase();
  if (text.includes('some operations were aborted') || text.includes('operation was aborted')) {
    return 'Windows 回收站操作被中断';
  }
  if (text.includes('being used') || text.includes('in use') || text.includes('sharing violation') || text.includes('正在使用') || text.includes('占用')) {
    return '文件正在被其他程序占用';
  }
  if (text.includes('access is denied') || text.includes('permission denied') || text.includes('拒绝访问') || text.includes('权限')) {
    return '权限不足或系统保护';
  }
  if (text.includes('directory not empty') || text.includes('目录不是空的')) {
    return '目录内仍有未删除内容';
  }
  if (text.includes('not found') || text.includes('路径不存在') || text.includes('找不到')) {
    return '文件已不存在';
  }
  return 'Windows 未返回明确原因';
}

function cleanErrorSuggestion(err: JunkCleanError): string {
  if (err.suggestion) return err.suggestion;
  const reason = cleanErrorReason(err);
  if (reason === 'Windows 回收站操作被中断') return '关闭相关程序后重试，或改用永久删除。';
  if (reason === '文件正在被其他程序占用') return '关闭 Chrome、Edge、VS Code、Windsurf 或相关后台程序后重新扫描并清理。';
  if (reason === '权限不足或系统保护') return '确认已用管理员身份运行；系统保护目录可能仍会被 Windows 拦截。';
  if (reason === '目录内仍有未删除内容') return '先处理失败文件后再重试删除目录。';
  if (reason === '文件已不存在') return '重新扫描后列表会刷新。';
  return '建议关闭相关应用后重试；如果仍失败，可查看系统详情。';
}

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

async function startScan(categories: JunkCategory[] | null = lastCategories.value, keepCleanResult = false) {
  if (status.value === 'scanning' || status.value === 'cleaning') return;

  const cached = cache.get(categories);
  if (cached) {
    if (!keepCleanResult) {
      lastCleanResult.value = null;
    }
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
  cleanProgress.value = null;
  if (!keepCleanResult) {
    lastCleanResult.value = null;
  }

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
    const t_invoke_start = performance.now();
    const r = await invoke<JunkScanResult>(CMD_SCAN_JUNK, { categories });
    const invokeMs = performance.now() - t_invoke_start;

    const t_index = performance.now();
    result.value = r;
    rebuildItemIndex(r.items);
    const indexMs = performance.now() - t_index;

    const t_select = performance.now();
    cache.set(categories, r);
    applyDefaultSelection(r.items);
    applyDefaultExpansion();
    const selectMs = performance.now() - t_select;

    status.value = 'scanned';
    // eslint-disable-next-line no-console
    console.info(
      `[junk-perf] invoke=${invokeMs.toFixed(0)}ms index=${indexMs.toFixed(0)}ms postprocess=${selectMs.toFixed(0)}ms items=${r.items.length} reported_scan=${r.scan_duration_ms}ms`,
    );
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
  if (unlistenCleanProgress) {
    unlistenCleanProgress();
    unlistenCleanProgress = null;
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

function badgeLevelFor(risk: JunkRiskLevel): 'safe' | 'caution' | 'risky' {
  if (risk === 'safe') return 'safe';
  if (risk === 'caution') return 'caution';
  return 'risky';
}

function categoryRiskColor(category: JunkCategory): string {
  const cautionCategories: JunkCategory[] = ['app_logs' as JunkCategory, 'windows_update' as JunkCategory];
  if (cautionCategories.includes(category)) return 'var(--color-warning)';
  return 'var(--color-success)';
}

const confirmOpen = ref(false);
const confirmMode = ref<'recycle' | 'permanent'>('recycle');

const dryRunOpen = ref(false);
const dryRunReport = shallowRef<DryRunReport | null>(null);
const dryRunLoading = ref(false);
const pendingMode = ref<'recycle' | 'permanent'>('recycle');

const feedbackOpen = ref(false);
const feedbackTarget = ref<JunkItem | null>(null);
const feedbackNote = ref('');
const feedbackSending = ref(false);

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

async function requestDryRun(mode: 'recycle' | 'permanent') {
  if (checked.size === 0) return;
  if (status.value === 'cleaning' || status.value === 'scanning') return;
  if (dryRunLoading.value) return;
  pendingMode.value = mode;
  dryRunLoading.value = true;
  const items: DryRunInput[] = [];
  for (const path of checked.value) {
    const idx = itemIndex.get(path);
    if (!idx) continue;
    items.push({ path, rule_id: idx.item.rule_id, size_bytes: idx.item.size });
  }
  try {
    const report = await invoke<DryRunReport>(CMD_DRY_RUN_JUNK, { items });
    dryRunReport.value = report;
    dryRunOpen.value = true;
  } catch (err) {
    showToast('预检失败', String(err), 'error');
  } finally {
    dryRunLoading.value = false;
  }
}

function dryRunCancel() {
  if (status.value === 'cleaning') return;
  dryRunOpen.value = false;
  dryRunReport.value = null;
}

function dryRunConfirm() {
  const report = dryRunReport.value;
  if (!report) return;
  const allowed = new Set(report.will_delete.map((p) => p.path));
  const filtered: string[] = [];
  for (const path of checked.value) {
    if (allowed.has(path)) filtered.push(path);
  }
  if (filtered.length === 0) {
    dryRunOpen.value = false;
    dryRunReport.value = null;
    showToast('没有可清理项', '所有目标都被安全检查跳过', 'warning');
    return;
  }
  checked.replace(filtered);
  recomputeSelectedSize();
  dryRunOpen.value = false;
  dryRunReport.value = null;
  confirmMode.value = pendingMode.value;
  if (pendingMode.value === 'permanent' || selectedSize.value > 1073741824) {
    confirmOpen.value = true;
  } else {
    void performClean();
  }
}

function cancelConfirm() {
  confirmOpen.value = false;
}

function openFeedback(item: JunkItem) {
  feedbackTarget.value = item;
  feedbackNote.value = '';
  feedbackOpen.value = true;
}

function cancelFeedback() {
  if (feedbackSending.value) return;
  feedbackOpen.value = false;
  feedbackTarget.value = null;
  feedbackNote.value = '';
}

async function submitFeedback() {
  const target = feedbackTarget.value;
  if (!target) return;
  const note = feedbackNote.value.trim();
  if (!note) {
    showToast('请填写描述', '简短说明这条为什么不该清', 'info');
    return;
  }
  feedbackSending.value = true;
  const payload: JunkFeedback = {
    path: target.path,
    rule_id: target.rule_id,
    rule_name: target.rule_name,
    user_note: note,
    reported_at_iso: new Date().toISOString(),
  };
  try {
    await invoke(CMD_REPORT_JUNK_FEEDBACK, { feedback: payload });
    showToast('反馈已记录', '感谢反馈，下个版本会优化此规则', 'success');
    feedbackOpen.value = false;
    feedbackTarget.value = null;
    feedbackNote.value = '';
  } catch (err) {
    showToast('反馈失败', String(err), 'error');
  } finally {
    feedbackSending.value = false;
  }
}

async function performClean() {
  const paths = Array.from(checked.value);
  const toRecycleBin = confirmMode.value === 'recycle';
  confirmOpen.value = false;
  if (paths.length === 0) return;
  lastCleanResult.value = null;
  showAllCleanErrors.value = false;
  status.value = 'cleaning';
  cleanProgress.value = {
    current_path: '',
    completed_items: 0,
    total_items: paths.length,
    cleaned_size: 0,
    total_size: paths.reduce((sum, path) => sum + sizeOfPath(path), 0),
    current_deleted_files: 0,
    current_total_files: 0,
    current_file: '',
    progress_percent: 0,
    error_count: 0,
  };

  try {
    if (unlistenCleanProgress) {
      unlistenCleanProgress();
      unlistenCleanProgress = null;
    }
    unlistenCleanProgress = await listen<JunkCleanProgress>('junk-clean-progress', (e) => {
      cleanProgress.value = e.payload;
    });
  } catch {
    void 0;
  }
  try {
    const r = await invoke<JunkCleanResult>(CMD_CLEAN_JUNK, {
      paths,
      toRecycleBin,
    });
    lastCleanResult.value = r;
    if (r.success) {
      showToast(
        '清理完成',
        `已释放 ${formatBytes(r.cleaned_size)} · ${r.cleaned_count} 项`,
        'success',
      );
    } else if (r.cleaned_count > 0) {
      showToast(
        '部分清理失败',
        `${r.errors.length || r.failed_count} 项失败，已释放 ${formatBytes(r.cleaned_size)}`,
        'warning',
      );
    } else {
      showToast('清理失败', `${r.errors.length || r.failed_count} 项均失败`, 'error');
    }
  } catch (err) {
    showToast('清理失败', String(err), 'error');
  } finally {
    status.value = 'scanned';
    checked.clear();
    selectedSizeAcc = 0;
    selectedSizeRef.value = 0;
    cache.clear();
    if (unlistenCleanProgress) {
      unlistenCleanProgress();
      unlistenCleanProgress = null;
    }
    await startScan(lastCategories.value, true);
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
    <FeatureIntro
      storage-key="junk"
      what="自动找出 Windows 和应用产生的临时垃圾，分类列出每一项的来源、大小、风险标签。"
      when="感觉系统卡顿、C 盘红条、或者上次清理超过 30 天。"
      outcome="勾选 → 一键清理。默认走 Windows 回收站可还原；勾「永久删除」才会绕过回收站。"
      reversibility="reversible"
      reversibility-note="多数项目下次软件运行会自动重新生成"
    />
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
          <div class="junk-progress-track">
            <div
              class="junk-progress-bar"
              :style="{ width: ((scanProgress.completed_rules / scanProgress.total_rules) * 100).toFixed(1) + '%' }"
            ></div>
          </div>
          <span class="junk-progress-text">
            {{ scanProgress.completed_rules }} / {{ scanProgress.total_rules }}
          </span>
        </div>
      </div>
    </div>

    <div v-else-if="status === 'cleaning'" class="junk-scanning">
      <IconSpinner :size="28" />
      <div class="junk-scanning-copy">
        <strong>正在清理垃圾文件…</strong>
        <small v-if="cleanProgress">
          {{ cleanProgress.current_path || cleanProgress.current_file || '准备中…' }} ·
          {{ cleanProgress.completed_items }} / {{ cleanProgress.total_items }} 项 ·
          {{ formatBytes(cleanProgress.cleaned_size) }} / {{ formatBytes(cleanProgress.total_size) }}
        </small>
        <small v-else>正在初始化清理任务…</small>
        <div class="junk-progress" v-if="cleanProgress && cleanProgress.total_items > 0">
          <div class="junk-progress-track">
            <div
              class="junk-progress-bar"
              :style="{ width: `${cleanProgress.progress_percent.toFixed(1)}%` }"
            ></div>
          </div>
          <span class="junk-progress-text">
            {{ cleanProgress.completed_items }} / {{ cleanProgress.total_items }}
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
              <span class="junk-group-dot" :style="{ backgroundColor: categoryRiskColor(g.category) }"></span>
              <strong>{{ g.label }}</strong>
              <span class="junk-group-meta">{{ g.items.length }} 项 · {{ formatBytes(g.totalSize) }}</span>
              <span class="junk-group-bar">
                <span class="junk-group-bar-fill" :style="{ width: ((g.totalSize / (result?.total_size || 1)) * 100) + '%', backgroundColor: categoryRiskColor(g.category) }"></span>
              </span>
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
              <div :key="(item as JunkItem).path" class="junk-row" :class="{ 'junk-row--checked': isChecked((item as JunkItem).path) }">
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
                  <RiskBadge :level="badgeLevelFor((item as JunkItem).risk_level)" />
                  <WhyCleanable
                    :why-safe="(item as JunkItem).why_safe"
                    :rule-name="(item as JunkItem).rule_name"
                    :risk="(item as JunkItem).risk_level"
                  />
                  <button
                    type="button"
                    class="junk-feedback-btn"
                    @click.stop="openFeedback(item as JunkItem)"
                    title="这条规则识别有误？告诉我们"
                  >
                    这条不对？
                  </button>
                </div>
              </div>
            </VirtualList>
          </div>
        </ElCollapseItem>
      </ElCollapse>
    </template>

    <div
      v-if="status === 'scanned' && lastCleanResult"
      class="junk-clean-result"
      :class="`junk-clean-result--${cleanResultTone}`"
    >
      <div class="junk-clean-result-head">
        <div>
          <strong>{{ cleanResultTitle }}</strong>
          <span>{{ cleanResultDescription }}</span>
        </div>
        <button
          v-if="cleanErrors.length > 8"
          type="button"
          class="junk-clean-toggle"
          @click="showAllCleanErrors = !showAllCleanErrors"
        >
          {{ showAllCleanErrors ? '收起' : `查看全部 ${cleanErrors.length} 项` }}
        </button>
      </div>

      <div v-if="cleanErrorGroups.length > 0" class="junk-error-groups">
        <div
          v-for="group in cleanErrorGroups"
          :key="`${group.reason}-${group.suggestion}`"
          class="junk-error-group"
        >
          <div>
            <strong>{{ group.reason }}</strong>
            <span>{{ group.count }} 项</span>
          </div>
          <small>{{ group.suggestion }}</small>
        </div>
      </div>

      <div v-if="cleanErrors.length > 0" class="junk-errors-list">
        <div
          v-for="err in visibleCleanErrors"
          :key="`${err.path}-${err.error}`"
          class="junk-error-item"
        >
          <div class="junk-error-path">{{ err.path }}</div>
          <div class="junk-error-reason">{{ err.reason }}</div>
          <div class="junk-error-text">系统详情：{{ err.error }}</div>
        </div>
      </div>
      <div v-if="hiddenCleanErrorCount > 0" class="junk-error-more">
        还有 {{ hiddenCleanErrorCount }} 项未显示，点右上角查看全部。
      </div>
    </div>

    <div
      v-if="status === 'scanned' && result && result.items.length === 0"
      class="junk-empty"
    >
      <div class="junk-empty-mark"><IconSuccess :size="28" /></div>
      <h4>{{ stateCopy.junk.empty.title }}</h4>
      <p>{{ stateCopy.junk.empty.description }}</p>
    </div>

    <footer
      v-if="status === 'scanned' && result && result.items.length > 0"
      class="junk-foot"
    >
      <div class="junk-foot-stats">
        <strong>已选 {{ selectedCount }} 项</strong>
        <span>{{ formatBytes(selectedSize) }}</span>
      </div>
      <div class="junk-foot-actions">
        <button
          class="junk-btn junk-btn--primary"
          :disabled="selectedCount === 0 || dryRunLoading"
          @click="requestDryRun('recycle')"
        >
          <IconSpinner v-if="dryRunLoading && pendingMode === 'recycle'" :size="14" />
          <span>{{ dryRunLoading && pendingMode === 'recycle' ? '预检中…' : '清理到回收站' }}</span>
        </button>
        <button
          class="junk-btn junk-btn--danger"
          :disabled="selectedCount === 0 || dryRunLoading"
          @click="requestDryRun('permanent')"
        >
          <IconSpinner v-if="dryRunLoading && pendingMode === 'permanent'" :size="14" />
          <span>{{ dryRunLoading && pendingMode === 'permanent' ? '预检中…' : '永久删除' }}</span>
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
      :high-risk="confirmMode === 'permanent' || selectedSize > 1073741824"
      @confirm="performClean"
      @cancel="cancelConfirm"
    />

    <JunkSkippedRulesDialog
      :show="skipDialogOpen"
      :rule-ids="result?.skipped_rules ?? []"
      @close="closeSkipDialog"
      @rescan="rescanFromSkipDialog"
    />

    <DryRunPreview
      :show="dryRunOpen"
      :report="dryRunReport"
      :busy="status === 'cleaning'"
      :mode="pendingMode"
      @cancel="dryRunCancel"
      @confirm="dryRunConfirm"
    />

    <div v-if="feedbackOpen && feedbackTarget" class="junk-fb-overlay" @click.self="cancelFeedback">
      <div class="junk-fb-dialog" role="dialog" aria-labelledby="junk-fb-title">
        <h3 id="junk-fb-title">这条规则不对？</h3>
        <p class="junk-fb-meta">
          规则：<strong>{{ feedbackTarget.rule_name }}</strong>
        </p>
        <p class="junk-fb-path">{{ feedbackTarget.path }}</p>
        <textarea
          v-model="feedbackNote"
          class="junk-fb-input"
          rows="4"
          placeholder="为什么这条不该被识别为可清？（例：这是我的配置文件，不是缓存）"
          :disabled="feedbackSending"
        />
        <div class="junk-fb-actions">
          <button class="btn-base btn-secondary" type="button" :disabled="feedbackSending" @click="cancelFeedback">取消</button>
          <button class="btn-base btn-primary" type="button" :disabled="feedbackSending || !feedbackNote.trim()" @click="submitFeedback">
            {{ feedbackSending ? '提交中…' : '提交反馈' }}
          </button>
        </div>
      </div>
    </div>
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
  box-shadow: 0 10px 22px var(--color-highlight-soft);
}

.junk-btn--primary:hover:not(:disabled) {
  box-shadow: 0 14px 26px var(--color-highlight-soft);
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
  background: var(--risk-risky-base);
  color: var(--risk-risky-on);
  border-color: transparent;
  box-shadow: 0 10px 22px var(--risk-risky-ring);
}

.junk-btn--danger:hover:not(:disabled) {
  box-shadow: 0 14px 28px var(--risk-risky-ring);
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
  margin-top: 0.6rem;
  display: flex;
  align-items: center;
  gap: 0.6rem;
}

.junk-progress-track {
  position: relative;
  flex: 1 1 auto;
  height: 6px;
  border-radius: 999px;
  background: rgba(15, 23, 32, 0.08);
  overflow: hidden;
}

[data-theme="dark"] .junk-progress-track {
  background: rgba(255, 255, 255, 0.08);
}

.junk-progress-bar {
  height: 100%;
  background: linear-gradient(90deg, var(--color-highlight), var(--color-info));
  border-radius: 999px;
  transition: width 0.3s ease;
}

.junk-progress-text {
  flex-shrink: 0;
  font-size: 0.72rem;
  line-height: 1;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
  font-variant-numeric: tabular-nums;
  min-width: 4rem;
  text-align: right;
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

.junk-group-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.junk-group-title strong {
  color: var(--color-text-primary);
  font-weight: 700;
  font-size: 0.92rem;
}

.junk-group-meta {
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
}

.junk-group-bar {
  width: 60px;
  height: 4px;
  border-radius: 999px;
  background: rgba(15, 23, 32, 0.06);
  overflow: hidden;
  flex-shrink: 0;
  margin-left: auto;
}

.junk-group-bar-fill {
  display: block;
  height: 100%;
  border-radius: 999px;
  transition: width var(--transition-fast);
}

.junk-list-shell {
  background: var(--color-bg-secondary);
  min-height: 64px;
}

.junk-clean-result {
  border: 1px solid var(--risk-safe-ring);
  background: var(--risk-safe-soft);
  border-radius: var(--radius-md);
  padding: 0.9rem 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.junk-clean-result--warning {
  border-color: var(--risk-caution-ring);
  background: var(--risk-caution-soft);
}

.junk-clean-result--danger {
  border-color: var(--risk-risky-ring);
  background: var(--risk-risky-soft);
}

.junk-clean-result-head {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: center;
}

.junk-clean-result-head > div {
  display: flex;
  flex-direction: column;
  gap: 0.16rem;
}

.junk-clean-result-head strong {
  color: var(--color-text-primary);
  font-size: 0.88rem;
}

.junk-clean-result-head span {
  color: var(--color-text-tertiary);
  font-size: 0.78rem;
}

.junk-clean-toggle {
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  padding: 0.32rem 0.65rem;
  border-radius: var(--radius-xs);
  font-size: 0.76rem;
  font-weight: 700;
  cursor: pointer;
}

.junk-error-groups {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 0.55rem;
}

.junk-error-group {
  padding: 0.65rem 0.75rem;
  border-radius: var(--radius-sm);
  background: rgba(255, 255, 255, 0.56);
  border: 1px solid rgba(15, 23, 32, 0.06);
}

[data-theme="dark"] .junk-error-group {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.08);
}

.junk-error-group div {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  align-items: center;
}

.junk-error-group strong {
  color: var(--color-text-primary);
  font-size: 0.8rem;
}

.junk-error-group span {
  color: var(--color-warning);
  font-size: 0.76rem;
  font-weight: 700;
  white-space: nowrap;
}

.junk-error-group small {
  display: block;
  margin-top: 0.22rem;
  color: var(--color-text-tertiary);
  font-size: 0.74rem;
  line-height: 1.35;
}

.junk-errors-list {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  max-height: 180px;
  overflow: auto;
}

.junk-error-item {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  padding: 0.35rem 0;
  border-top: 1px solid rgba(15, 23, 32, 0.06);
}

.junk-error-path {
  font-size: 0.8rem;
  color: var(--color-text-primary);
  word-break: break-all;
}

.junk-error-reason {
  font-size: 0.76rem;
  color: var(--color-warning);
  font-weight: 700;
}

.junk-error-text {
  font-size: 0.76rem;
  color: var(--color-text-tertiary);
  word-break: break-word;
}

.junk-error-more {
  color: var(--color-text-tertiary);
  font-size: 0.75rem;
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
  transition: background var(--transition-fast);
}

.junk-row--checked {
  background: rgba(16, 185, 129, 0.04);
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

[data-theme="dark"] .junk-foot {
  background: linear-gradient(180deg, var(--color-surface), var(--color-surface-strong));
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

/* ===== Element Plus collapse / checkbox / tag dark-mode patch ===== */
[data-theme="dark"] .junk-collapse {
  background: var(--color-surface-strong);
}

[data-theme="dark"] .junk-collapse :deep(.el-collapse) {
  --el-collapse-header-bg-color: transparent;
  --el-collapse-content-bg-color: transparent;
  --el-collapse-header-text-color: var(--color-text-primary);
  --el-collapse-border-color: var(--color-border-light);
  border-color: var(--color-border-light);
  background: transparent;
}

[data-theme="dark"] .junk-collapse :deep(.el-collapse-item__header),
[data-theme="dark"] .junk-collapse :deep(.el-collapse-item__wrap),
[data-theme="dark"] .junk-collapse :deep(.el-collapse-item__content) {
  background: transparent;
  color: var(--color-text-primary);
  border-color: var(--color-border-light);
}

[data-theme="dark"] .junk-collapse :deep(.el-collapse-item__arrow) {
  color: var(--color-text-secondary);
}

[data-theme="dark"] .junk-collapse :deep(.el-collapse-item__header):hover {
  background: rgba(255, 255, 255, 0.03);
}

[data-theme="dark"] .junk-list-shell {
  background: transparent;
}

[data-theme="dark"] .junk-row {
  border-top-color: var(--color-border-light);
}

[data-theme="dark"] .junk-row:hover {
  background: rgba(255, 255, 255, 0.03);
}

[data-theme="dark"] .junk-row--checked {
  background: rgba(52, 211, 153, 0.06);
}

[data-theme="dark"] .junk-group-bar {
  background: rgba(255, 255, 255, 0.08);
}

/* Checkbox tokens */
[data-theme="dark"] :deep(.el-checkbox) {
  --el-checkbox-bg-color: transparent;
  --el-checkbox-input-background-color: transparent;
  --el-checkbox-input-border-color: var(--color-border-strong);
  --el-checkbox-input-border-color-hover: var(--color-highlight);
  --el-checkbox-checked-bg-color: var(--color-highlight);
  --el-checkbox-checked-input-border-color: var(--color-highlight);
  --el-checkbox-text-color: var(--color-text-primary);
}

[data-theme="dark"] :deep(.el-checkbox__inner) {
  background-color: transparent;
  border-color: var(--color-border-strong);
}

/* Tag risk badges */
[data-theme="dark"] :deep(.el-tag) {
  --el-tag-bg-color: rgba(255, 255, 255, 0.06);
  --el-tag-border-color: var(--color-border-medium);
  --el-tag-text-color: var(--color-text-primary);
}

[data-theme="dark"] :deep(.el-tag--success) {
  background: rgba(52, 211, 153, 0.12);
  border-color: rgba(52, 211, 153, 0.28);
  color: var(--color-success);
}

[data-theme="dark"] :deep(.el-tag--warning) {
  background: rgba(251, 191, 36, 0.12);
  border-color: rgba(251, 191, 36, 0.28);
  color: var(--color-warning);
}

[data-theme="dark"] :deep(.el-tag--danger) {
  background: rgba(248, 113, 113, 0.12);
  border-color: rgba(248, 113, 113, 0.28);
  color: var(--color-error);
}

[data-theme="dark"] :deep(.el-tag--info) {
  background: rgba(96, 165, 250, 0.12);
  border-color: rgba(96, 165, 250, 0.28);
  color: var(--color-info);
}

.junk-feedback-btn {
  border: 1px dashed var(--color-border-medium);
  background: transparent;
  color: var(--color-text-tertiary);
  padding: 0.18rem 0.5rem;
  border-radius: var(--radius-pill);
  font-size: 0.7rem;
  font-weight: 600;
  cursor: pointer;
  transition: color var(--transition-fast), border-color var(--transition-fast), background var(--transition-fast);
}

.junk-feedback-btn:hover {
  color: var(--color-warning);
  border-color: var(--risk-caution-ring);
  background: var(--risk-caution-soft);
}

.junk-fb-overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  z-index: 2400;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
}

.junk-fb-dialog {
  width: min(440px, 100%);
  background: var(--color-bg-primary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-medium);
  box-shadow: var(--shadow-xl);
  padding: 1.25rem 1.4rem;
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}

.junk-fb-dialog h3 {
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.junk-fb-meta {
  font-size: 0.84rem;
  color: var(--color-text-secondary);
}

.junk-fb-meta strong {
  color: var(--color-text-primary);
}

.junk-fb-path {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  word-break: break-all;
  padding: 0.45rem 0.6rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-xs);
}

.junk-fb-input {
  width: 100%;
  font-family: inherit;
  font-size: 0.88rem;
  padding: 0.6rem 0.75rem;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-primary);
  resize: vertical;
  box-sizing: border-box;
}

.junk-fb-input:focus-visible {
  outline: none;
  border-color: var(--color-highlight);
  box-shadow: 0 0 0 3px var(--color-highlight-soft);
}

.junk-fb-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.55rem;
  margin-top: 0.3rem;
}
</style>
