<script setup lang="ts">
import { computed, defineAsyncComponent, nextTick, ref, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { DiskInfo, ScanResult, SmartItem, SmartScanReport } from '../types';
import type { SpaceBreakdown, BalanceSuggestion, BreakdownItem, BalanceItem, ReclaimOpportunity, KnownFolderInfo, RedirectResult } from '../types/breakdown';
import { formatBytes, formatNumber } from '../utils/format';
import { useCart } from '../composables/useCart';
import { useCountUp } from '../composables/useCountUp';
import { useToast } from '../composables/useToast';
import SmartGroupCard from './SmartGroupCard.vue';
import SpaceTrend from './SpaceTrend.vue';
import SpaceBreakdownVue from './SpaceBreakdown.vue';
import BalanceSuggestionVue from './BalanceSuggestion.vue';
import CleanupResult from './CleanupResult.vue';
import { IconScanEmpty, IconSuccess, IconClose } from './icons';

const ListView = defineAsyncComponent(() => import('./ListView.vue'));
const LargeFilesView = defineAsyncComponent(() => import('./LargeFilesView.vue'));
const DuplicatesView = defineAsyncComponent(() => import('./DuplicatesView.vue'));
const SystemReclaim = defineAsyncComponent(() => import('./SystemReclaim.vue'));
const FolderRedirect = defineAsyncComponent(() => import('./FolderRedirect.vue'));

interface Props {
  scanResult: ScanResult | null;
  selectedDisk: string;
  selectedDiskInfo: DiskInfo | null;
  availableDisks: DiskInfo[];
  hasDeepScanned: boolean;
  deepScanning: boolean;
  largeFileThreshold: number;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'start-scan': [];
  'navigate': [path: string];
  'migrate-from-cart': [targetDisk: string];
  'migrate-single': [item: SmartItem | { path: string; name: string; size: number; file_count: number }];
  'reveal': [path: string];
}>();

const cart = useCart();
const showToast = useToast();

const smartReport = ref<SmartScanReport | null>(null);
const loadingSmart = ref(false);
const browseOpen = ref(false);
const browseTab = ref<'list' | 'large_files' | 'duplicates'>('list');
const trendRefreshKey = ref(0);

const breakdown = ref<SpaceBreakdown | null>(null);
const loadingBreakdown = ref(false);
const balanceSuggestion = ref<BalanceSuggestion | null>(null);
const loadingBalance = ref(false);
const reclaimOpportunities = ref<ReclaimOpportunity[]>([]);
const loadingReclaim = ref(false);
const knownFolders = ref<KnownFolderInfo[]>([]);
const loadingFolders = ref(false);
const showReclaimPanel = ref(false);
const showRedirectPanel = ref(false);
const cleanupResultVisible = ref(false);
const cleanupResultData = ref({
  beforeUsed: 0,
  afterUsed: 0,
  diskTotal: 0,
  freedBytes: 0,
  itemCount: 0,
  operationType: 'migrate' as 'migrate' | 'delete' | 'reclaim' | 'redirect',
});

const smartGroups = computed(() => smartReport.value?.groups ?? []);
// Workspace is mounted under `v-if="activeTab === 'workspace'"` in App.vue,
// so the tab being inactive already prevents this composable from running
// (effectScope dispose cancels the raf). We still gate raf on the browse
// drawer being closed because the drawer fully overlays the hero card —
// running raf in that period burns cycles for an off-screen update.
const isHeroVisible = () => !browseOpen.value;
const totalSavings = useCountUp(() => smartReport.value?.default_savings ?? 0, {
  durationMs: 700,
  isVisible: isHeroVisible,
});
const potentialSavings = computed(() => smartReport.value?.potential_savings ?? 0);
const itemCount = computed(() =>
  smartGroups.value.reduce((sum, g) => sum + g.item_count, 0),
);

watch(
  () => props.scanResult,
  async (next) => {
    if (next && props.hasDeepScanned) {
      await loadSmart();
      void loadBreakdown();
      void loadBalanceSuggestion();
      void nextTick(() => {
        trendRefreshKey.value += 1;
      });
    } else {
      smartReport.value = null;
      breakdown.value = null;
      balanceSuggestion.value = null;
    }
  },
  { immediate: true },
);

async function loadSmart() {
  if (!props.selectedDisk) return;
  loadingSmart.value = true;
  try {
    const report = await invoke<SmartScanReport>('analyze_smart_groups', {
      rootPath: props.selectedDisk,
    });
    smartReport.value = report;
    cart.setFromSmartGroups(report.groups);
  } catch (err) {
    console.error('[smart-scan] failed:', err);
    showToast('智能扫描失败', String(err), 'warning');
  } finally {
    loadingSmart.value = false;
  }
}

async function loadBreakdown() {
  if (!props.selectedDisk) return;
  loadingBreakdown.value = true;
  try {
    breakdown.value = await invoke<SpaceBreakdown>('get_space_breakdown', {
      rootPath: props.selectedDisk,
    });
  } catch {
    breakdown.value = null;
  } finally {
    loadingBreakdown.value = false;
  }
}

async function loadBalanceSuggestion() {
  if (!props.selectedDisk || props.availableDisks.length < 2) return;
  loadingBalance.value = true;
  try {
    balanceSuggestion.value = await invoke<BalanceSuggestion>('get_balance_suggestion', {
      rootPath: props.selectedDisk,
    });
  } catch {
    balanceSuggestion.value = null;
  } finally {
    loadingBalance.value = false;
  }
}

async function loadReclaimOpportunities() {
  loadingReclaim.value = true;
  try {
    reclaimOpportunities.value = await invoke<ReclaimOpportunity[]>('get_reclaim_opportunities');
  } catch {
    reclaimOpportunities.value = [];
  } finally {
    loadingReclaim.value = false;
  }
}

async function loadKnownFolders() {
  loadingFolders.value = true;
  try {
    knownFolders.value = await invoke<KnownFolderInfo[]>('get_known_folders');
  } catch {
    knownFolders.value = [];
  } finally {
    loadingFolders.value = false;
  }
}

function openReclaimPanel() {
  showReclaimPanel.value = true;
  void loadReclaimOpportunities();
}

function openRedirectPanel() {
  showRedirectPanel.value = true;
  void loadKnownFolders();
}

function onBreakdownMigrate(item: BreakdownItem) {
  emit('migrate-single', { path: item.path, name: item.name, size: item.size, file_count: 1 });
}

function onBreakdownNavigate(path: string) {
  emit('navigate', path);
  openBrowse('list');
}

function onBalanceExecute(items: BalanceItem[]) {
  const batch = items.map(it => ({
    path: it.path,
    name: it.name,
    size: it.size,
    file_count: 1,
    recommendation: 'migrate' as const,
    source: 'browse' as const,
  }));
  cart.addBatch(batch);
  showToast('已加入搬运车', `${items.length} 项 · 共 ${formatBytes(items.reduce((s, i) => s + i.size, 0))}`, 'info');
}

function onReclaimed(freedBytes: number) {
  const diskTotal = props.selectedDiskInfo?.total_space ?? 0;
  const usedBefore = props.selectedDiskInfo?.used_space ?? 0;
  cleanupResultData.value = {
    beforeUsed: usedBefore,
    afterUsed: usedBefore - freedBytes,
    diskTotal,
    freedBytes,
    itemCount: 1,
    operationType: 'reclaim',
  };
  cleanupResultVisible.value = true;
}

function onRedirected(result: RedirectResult) {
  const diskTotal = props.selectedDiskInfo?.total_space ?? 0;
  const usedBefore = props.selectedDiskInfo?.used_space ?? 0;
  cleanupResultData.value = {
    beforeUsed: usedBefore,
    afterUsed: usedBefore - result.moved_bytes,
    diskTotal,
    freedBytes: result.moved_bytes,
    itemCount: result.moved_files,
    operationType: 'redirect',
  };
  cleanupResultVisible.value = true;
}

const isElevated = ref(false);
const hasOtherDisks = computed(() => props.availableDisks.length > 1);

onMounted(async () => {
  try {
    isElevated.value = await invoke<boolean>('is_elevated');
  } catch {}
});


function handleMigrate(item: SmartItem) {
  emit('migrate-single', item);
}

function openBrowse(tab: 'list' | 'large_files' | 'duplicates' = 'list') {
  browseTab.value = tab;
  browseOpen.value = true;
}

function closeBrowse() {
  browseOpen.value = false;
}

function onListMigrate(dir: { path: string; name: string; size: number; file_count: number }) {
  emit('migrate-single', dir);
}

function onListMigrateFile(file: { path: string; name: string; size: number }) {
  emit('migrate-single', { path: file.path, name: file.name, size: file.size, file_count: 1 });
}

function onListBatchMigrate(items: Array<{ path: string; name: string; size: number; file_count?: number }>) {
  let totalSize = 0;
  const batch = items.map((it) => {
    totalSize += it.size;
    return {
      path: it.path,
      name: it.name,
      size: it.size,
      file_count: it.file_count ?? 1,
      recommendation: 'migrate' as const,
      source: 'browse' as const,
    };
  });
  cart.addBatch(batch);
  showToast('已加入搬运车', `${items.length} 项 · 共 ${formatBytes(totalSize)}`, 'info');
}

const heroSubtitle = computed(() => {
  if (props.deepScanning) return '正在分析磁盘…';
  if (loadingSmart.value) return '正在生成智能建议…';
  if (!props.hasDeepScanned) return `${props.selectedDiskInfo?.drive_letter ?? ''} 选择磁盘并点击"开始扫描"`;
  const report = smartReport.value;
  if (!report) return '准备智能建议中…';
  if (report.default_savings === 0) {
    const totalFiles = props.scanResult?.total_files ?? 0;
    return `已扫描 ${formatNumber(totalFiles)} 个文件，未发现明显可省项`;
  }
  return `在 ${itemCount.value} 项建议里默认勾选了安全可搬走的`;
});
</script>

<template>
  <div class="workspace">
    <section class="hero">
      <div class="hero-line">
        <span class="kicker">{{ selectedDiskInfo?.drive_letter ?? 'C:' }} · 智能建议</span>
        <span class="hero-sub">{{ heroSubtitle }}</span>
      </div>

      <div class="hero-row">
        <div class="hero-savings">
          <strong>{{ formatBytes(totalSavings) }}</strong>
          <small>可释放</small>
        </div>
        <div class="hero-detail">
          <strong>{{ formatBytes(potentialSavings) }}</strong>
          <small>建议总量</small>
        </div>
        <div class="hero-detail">
          <strong>{{ selectedDiskInfo ? formatBytes(selectedDiskInfo.free_space) : '--' }}</strong>
          <small>当前可用</small>
        </div>

        <div class="hero-actions">
          <button class="hero-btn ghost" @click="openBrowse('list')" :disabled="!hasDeepScanned">浏览全部</button>
          <button class="hero-btn ghost" @click="openBrowse('large_files')" :disabled="!hasDeepScanned">大文件</button>
          <button class="hero-btn ghost" @click="openBrowse('duplicates')" :disabled="!hasDeepScanned">重复文件</button>
          <button class="hero-btn ghost" @click="openReclaimPanel">系统回收</button>
          <button class="hero-btn ghost" @click="openRedirectPanel">默认存储位置</button>
          <button class="hero-btn refresh" @click="loadSmart" :disabled="!hasDeepScanned || loadingSmart">
            {{ loadingSmart ? '分析中…' : '重新分析' }}
          </button>
        </div>
      </div>
    </section>

    <SpaceTrend :drive="selectedDisk" :refresh-key="trendRefreshKey" />

    <SpaceBreakdownVue
      v-if="hasDeepScanned"
      :breakdown="breakdown"
      :default-savings="smartReport?.default_savings ?? 0"
      :loading="loadingBreakdown"
      @migrate-item="onBreakdownMigrate"
      @navigate="onBreakdownNavigate"
    />

    <BalanceSuggestionVue
      v-if="hasDeepScanned && hasOtherDisks"
      :suggestion="balanceSuggestion"
      :loading="loadingBalance"
      :available-disks="availableDisks"
      @execute="onBalanceExecute"
    />

    <div v-if="!hasDeepScanned" class="empty-state">
      <div class="empty-card">
        <div class="empty-icon"><IconScanEmpty :size="28" /></div>
        <h3>{{ selectedDiskInfo ? `开始扫描 ${selectedDiskInfo.drive_letter}` : '选择磁盘开始' }}</h3>
        <p>扫描完成后我会给你一份"今晚能搬走的"清单，全部勾选后一键搬走。</p>
        <button class="hero-btn primary big" @click="emit('start-scan')" :disabled="deepScanning || !selectedDisk">
          {{ deepScanning ? '扫描中…' : '开始扫描' }}
        </button>
      </div>
    </div>

    <div v-else-if="loadingSmart && smartGroups.length === 0" class="loading-state">
      <div class="spinner"></div>
      <p>分析中…</p>
    </div>

    <div v-else-if="smartGroups.length === 0" class="empty-state">
      <div class="empty-card">
        <div class="empty-icon"><IconSuccess :size="28" /></div>
        <h3>很干净，目前没有明显能省的</h3>
        <p>试试"浏览全部目录"找特定的大目录，或者用 ⌘ K 搜索。</p>
        <button class="hero-btn ghost" @click="openBrowse('list')">浏览全部</button>
      </div>
    </div>

    <div v-else class="groups">
      <SmartGroupCard
        v-for="group in smartGroups"
        :key="group.category"
        :group="group"
        @migrate="handleMigrate"
        @reveal="emit('reveal', $event)"
      />
    </div>

    <Teleport to="body">
      <div v-if="browseOpen" class="browse-overlay" @click="closeBrowse">
        <div class="browse-panel" @click.stop>
          <header class="browse-head">
            <div class="browse-tabs">
              <button :class="{ active: browseTab === 'list' }" @click="browseTab = 'list'">列表浏览</button>
              <button :class="{ active: browseTab === 'large_files' }" @click="browseTab = 'large_files'">大文件雷达</button>
              <button :class="{ active: browseTab === 'duplicates' }" @click="browseTab = 'duplicates'">重复文件</button>
            </div>
            <button class="icon-btn" @click="closeBrowse" aria-label="关闭"><IconClose :size="16" /></button>
          </header>

          <div class="browse-body">
            <ListView
              v-if="browseTab === 'list' && scanResult"
              :directories="scanResult.directories"
              :total-size="scanResult.total_size"
              :deep-scanning="deepScanning"
              :current-path="scanResult.root_path"
              :has-deep-scanned="hasDeepScanned"
              @navigate="emit('navigate', $event)"
              @migrate-dir="onListMigrate"
              @migrate-file="onListMigrateFile"
              @batch-migrate="onListBatchMigrate"
            />
            <LargeFilesView
              v-else-if="browseTab === 'large_files' && scanResult"
              :files="scanResult.large_files"
              :deep-scanning="deepScanning"
              :has-deep-scanned="hasDeepScanned"
              :large-file-threshold="largeFileThreshold"
              @migrate-file="onListMigrateFile"
            />
            <DuplicatesView
              v-else-if="browseTab === 'duplicates'"
              :root-path="selectedDisk"
              :has-deep-scanned="hasDeepScanned"
            />
          </div>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="showReclaimPanel" class="browse-overlay reclaim-overlay" @click="showReclaimPanel = false">
        <div class="browse-panel" @click.stop>
          <SystemReclaim
            :opportunities="reclaimOpportunities"
            :loading="loadingReclaim"
            :is-elevated="isElevated"
            @close="showReclaimPanel = false"
            @reclaimed="onReclaimed"
          />
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="showRedirectPanel" class="browse-overlay reclaim-overlay" @click="showRedirectPanel = false">
        <div class="browse-panel" @click.stop>
          <FolderRedirect
            :folders="knownFolders"
            :available-disks="availableDisks"
            :loading="loadingFolders"
            @close="showRedirectPanel = false"
            @redirected="onRedirected"
          />
        </div>
      </div>
    </Teleport>

    <CleanupResult
      :show="cleanupResultVisible"
      :before-used="cleanupResultData.beforeUsed"
      :after-used="cleanupResultData.afterUsed"
      :disk-total="cleanupResultData.diskTotal"
      :freed-bytes="cleanupResultData.freedBytes"
      :item-count="cleanupResultData.itemCount"
      :operation-type="cleanupResultData.operationType"
      @close="cleanupResultVisible = false"
    />
  </div>
</template>

<style scoped>
.workspace {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 0.85rem 1rem 5rem;
}

.hero {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  padding: 0.85rem 1.1rem 0.95rem;
  border-radius: 18px;
  background: linear-gradient(140deg, var(--color-bg-secondary) 0%, var(--color-bg-tertiary) 100%);
  border: 1px solid var(--color-border-light);
}

.hero-line {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  flex-wrap: wrap;
  min-height: 1.4rem;
}

.kicker {
  display: inline-block;
  padding: 0.18rem 0.5rem;
  border-radius: 999px;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.hero-sub {
  font-size: 0.86rem;
  color: var(--color-text-tertiary);
  flex: 1;
  min-width: 0;
}

.hero-row {
  display: flex;
  align-items: baseline;
  gap: 1.4rem;
  flex-wrap: wrap;
}

.hero-savings strong {
  display: block;
  font-family: var(--font-display);
  font-size: 2.2rem;
  font-weight: 600;
  letter-spacing: -0.02em;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  line-height: 1;
}
.hero-savings small {
  display: block;
  margin-top: 0.22rem;
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  font-weight: 600;
}

.hero-detail strong {
  display: block;
  font-size: 1rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
  line-height: 1;
}
.hero-detail small {
  display: block;
  margin-top: 0.22rem;
  font-size: 0.72rem;
  color: var(--color-text-tertiary);
}

.hero-actions {
  display: flex;
  gap: 0.45rem;
  margin-left: auto;
  align-self: center;
}

.hero-btn {
  padding: 0.5rem 0.85rem;
  border-radius: 10px;
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  font-weight: 600;
  font-size: 0.82rem;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), transform var(--transition-fast), box-shadow var(--transition-fast);
}
.hero-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  transform: translateY(-1px);
}
.hero-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.hero-btn.ghost { background: transparent; }
.hero-btn.refresh { background: var(--color-surface); }

.hero-btn.primary {
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  border-color: transparent;
  box-shadow: 0 12px 24px rgba(17, 24, 39, 0.18);
}
.hero-btn.primary:hover:not(:disabled) {
  box-shadow: 0 20px 36px rgba(17, 24, 39, 0.26);
}
.hero-btn.big {
  padding: 0.85rem 1.4rem;
  font-size: 0.95rem;
  border-radius: 14px;
}

.groups {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.empty-state, .loading-state {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 3rem 1rem;
}
.empty-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.6rem;
  padding: 2.2rem 2.4rem;
  border-radius: 22px;
  border: 1px dashed var(--color-border-medium);
  background: var(--color-surface);
  text-align: center;
  max-width: 36rem;
}
.empty-icon {
  width: 48px; height: 48px;
  border-radius: 14px;
  display: inline-flex; align-items: center; justify-content: center;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  font-size: 1.4rem;
}
.empty-card h3 { font-size: 1.25rem; font-weight: 700; }
.empty-card p { color: var(--color-text-tertiary); font-size: 0.92rem; line-height: 1.6; max-width: 28rem; }

.spinner {
  width: 28px; height: 28px;
  border: 2.5px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
  margin-right: 0.85rem;
}
.loading-state p { color: var(--color-text-tertiary); }
@keyframes spin { to { transform: rotate(360deg); } }

.browse-overlay {
  position: fixed; inset: 0;
  background: rgba(15, 23, 32, 0.36);
  display: flex;
  z-index: 1800;
  animation: fadeIn 0.2s ease;
}
.browse-panel {
  margin-left: auto;
  width: min(960px, 100vw);
  height: 100vh;
  background: var(--color-bg-secondary);
  border-left: 1px solid var(--color-border-light);
  display: flex;
  flex-direction: column;
  box-shadow: -32px 0 80px rgba(17, 24, 39, 0.22);
}
.browse-head {
  display: flex; justify-content: space-between; align-items: center;
  padding: 0.9rem 1.2rem;
  border-bottom: 1px solid var(--color-border-light);
}
.browse-tabs { display: flex; gap: 0.4rem; }
.browse-tabs button {
  padding: 0.55rem 0.9rem;
  border: 1px solid transparent;
  border-radius: 12px;
  background: transparent;
  color: var(--color-text-tertiary);
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
}
.browse-tabs button.active {
  background: var(--color-surface);
  color: var(--color-text-primary);
  border-color: var(--color-border-medium);
}

.icon-btn {
  width: 36px; height: 36px;
  border: 1px solid var(--color-border-light);
  border-radius: 10px;
  background: var(--color-surface);
  color: var(--color-text-secondary);
  cursor: pointer;
}
.icon-btn:hover { background: var(--color-surface-hover); color: var(--color-text-primary); }

.browse-body {
  flex: 1; min-height: 0; overflow: auto;
  padding: 1rem 1.2rem 5rem;
}

@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

.reclaim-overlay {
  z-index: 1900;
}
</style>
