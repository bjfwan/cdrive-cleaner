<script setup lang="ts">
import { computed, ref, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { IconFile, IconDocument, IconMigrate } from './icons';
import type { FileInfo, FilePageSort, LargeFilesPage } from '../types';
import { formatBytes, formatDate, formatNumber } from '../utils/format';
import VirtualList from './VirtualList.vue';
import ExplanationTooltip from './ExplanationTooltip.vue';
import FeatureIntro from './FeatureIntro.vue';
import { stateCopy } from '../utils/state-copy';
import { useToast } from '../composables/useToast';

interface Props {
  rootPath: string;
  path?: string;
  deepScanning?: boolean;
  hasDeepScanned: boolean;
  largeFileThreshold: number;
}

const props = defineProps<Props>();
const showToast = useToast();

defineEmits<{
  'migrate-file': [file: FileInfo];
}>();

const PAGE_SIZE = 200;
const LARGE_FILE_SORT: FilePageSort = 'size_desc';
const tooltipRef = ref<InstanceType<typeof ExplanationTooltip> | null>(null);
const pagedFiles = shallowRef<FileInfo[]>([]);
const total = ref(0);
const totalSize = ref(0);
const filteredTotalSize = ref(0);
const loadingInitial = ref(false);
const loadingMore = ref(false);
const loadError = ref('');
const hasMoreFlag = ref(false);
let pageRequestId = 0;
let largeFilesPageUnavailable = false;

const loaded = computed(() => pagedFiles.value.length);
const isLoading = computed(() => loadingInitial.value || loadingMore.value);
const hasMore = computed(() => hasMoreFlag.value || (total.value > 0 && loaded.value < total.value));

function onRowEnter(file: FileInfo, event: MouseEvent) {
  tooltipRef.value?.show(file.path, event.currentTarget as HTMLElement);
}

function onRowLeave() {
  tooltipRef.value?.hide();
}

function resetPageState() {
  pageRequestId += 1;
  pagedFiles.value = [];
  total.value = 0;
  totalSize.value = 0;
  filteredTotalSize.value = 0;
  loadingInitial.value = false;
  loadingMore.value = false;
  loadError.value = '';
  hasMoreFlag.value = false;
}

async function loadPage(reset: boolean) {
  if (!props.hasDeepScanned || !props.rootPath) return;
  if (!reset && (!hasMore.value || isLoading.value)) return;

  const requestId = ++pageRequestId;
  const offset = reset ? 0 : pagedFiles.value.length;
  const rootPath = props.rootPath;
  const path = props.path || props.rootPath;
  const minSize = props.largeFileThreshold * 1024 * 1024;

  if (reset) {
    loadingInitial.value = true;
    loadingMore.value = false;
  } else {
    loadingMore.value = true;
  }

  loadError.value = '';

  try {
    let page: LargeFilesPage;
    if (largeFilesPageUnavailable) {
      page = await loadLegacyPage(rootPath, path, minSize, offset);
    } else {
      try {
        page = await invoke<LargeFilesPage>('get_large_files_page', {
          rootPath,
          path,
          minSize,
          offset,
          limit: PAGE_SIZE,
          sort: LARGE_FILE_SORT,
        });
      } catch (err) {
        if (!isCommandUnavailable(err)) throw err;
        largeFilesPageUnavailable = true;
        page = await loadLegacyPage(rootPath, path, minSize, offset);
      }
    }
    if (requestId !== pageRequestId || rootPath !== props.rootPath || path !== (props.path || props.rootPath)) return;

    pagedFiles.value = reset ? page.files : [...pagedFiles.value, ...page.files];
    total.value = page.total;
    totalSize.value = page.total_size;
    filteredTotalSize.value = page.filtered_total_size;
    hasMoreFlag.value = page.has_more ?? pagedFiles.value.length < page.total;
  } catch (err) {
    if (requestId !== pageRequestId) return;
    loadError.value = String(err);
    if (reset) pagedFiles.value = [];
    showToast('大文件加载失败', String(err), 'error');
  } finally {
    if (requestId === pageRequestId) {
      loadingInitial.value = false;
      loadingMore.value = false;
    }
  }
}

function loadNextPage() {
  void loadPage(false);
}

async function loadLegacyPage(rootPath: string, path: string, minSize: number, offset: number): Promise<LargeFilesPage> {
  const files = await invoke<FileInfo[]>('get_large_files', { rootPath, path });
  const filtered = files
    .filter((file) => file.size >= minSize)
    .sort((a, b) => b.size - a.size);
  const pageFiles = filtered.slice(offset, offset + PAGE_SIZE);
  return {
    files: pageFiles,
    total: filtered.length,
    offset,
    limit: PAGE_SIZE,
    total_size: files.reduce((sum, file) => sum + file.size, 0),
    filtered_total_size: filtered.reduce((sum, file) => sum + file.size, 0),
    has_more: offset + pageFiles.length < filtered.length,
  };
}

function isCommandUnavailable(err: unknown): boolean {
  const message = String(err).toLowerCase();
  return message.includes('not found') || message.includes('unknown command') || message.includes('找不到');
}

watch(
  () => [props.rootPath, props.path, props.largeFileThreshold, props.hasDeepScanned] as const,
  () => {
    resetPageState();
    void loadPage(true);
  },
  { immediate: true },
);
</script>

<template>
  <div class="large-files-view">
    <FeatureIntro
      storage-key="large-files"
      what="只列出超过指定阈值的大文件，按大小排序，方便挑挑搬走或删掉。"
      when="想给某个盘腾出几个 GB 的空间、找占地最狠的几份文件、决策「哪些值得搬走」时。"
      outcome="点右侧迁移按钮把单个文件搬到其他盘；要删除请到「磁盘」视图加入搬运车统一处理。"
      reversibility="reversible"
      reversibility-note="迁移可从「迁移历史」回滚；本视图不直接删除文件"
    />
    <div class="summary-bar">
      <div>
        <h3>大文件聚焦</h3>
        <p>只保留最值得处理的文件，降低干扰，适合做迁移决策。</p>
      </div>

      <div class="summary-metrics">
        <div class="summary-pill">
          <span>阈值</span>
          <strong>{{ largeFileThreshold }} MB</strong>
        </div>
        <div class="summary-pill">
          <span>命中总数</span>
          <strong>{{ formatNumber(total) }}</strong>
        </div>
        <div class="summary-pill">
          <span>已加载</span>
          <strong>{{ formatNumber(loaded) }}</strong>
        </div>
        <div class="summary-pill">
          <span>命中体积</span>
          <strong>{{ formatBytes(filteredTotalSize) }}</strong>
        </div>
        <div class="summary-pill">
          <span>范围体积</span>
          <strong>{{ formatBytes(totalSize) }}</strong>
        </div>
        <div class="summary-pill" :class="{ muted: !hasDeepScanned }">
          <span>迁移能力</span>
          <strong>{{ hasDeepScanned ? '已启用' : '需深扫' }}</strong>
        </div>
      </div>
    </div>

    <div v-if="loadingInitial && pagedFiles.length === 0" class="empty">
      <IconDocument class="empty-icon" :size="46" />
      <h3>{{ stateCopy.largeFiles.loading.title }}</h3>
      <p>{{ stateCopy.largeFiles.loading.description }}</p>
    </div>

    <div v-else-if="pagedFiles.length === 0" class="empty">
      <IconDocument class="empty-icon" :size="46" />
      <h3>{{ loadError ? stateCopy.largeFiles.error.title : stateCopy.largeFiles.empty.title }}</h3>
      <p>{{ loadError || `当前范围里没有大于 ${largeFileThreshold} MB 的文件。${stateCopy.largeFiles.empty.description}` }}</p>
    </div>

    <div v-else class="table-shell">
      <div class="table-header">
        <div class="th th-name">文件名</div>
        <div class="th th-path">路径</div>
        <div class="th th-size">大小</div>
        <div class="th th-modified">修改时间</div>
        <div class="th th-actions">操作</div>
      </div>

      <div class="table-body">
        <VirtualList
          :items="pagedFiles"
          :item-size="56"
          :buffer="6"
          @scroll-near-end="loadNextPage"
          v-slot="{ item: file }"
        >
          <div :key="(file as FileInfo).path" class="table-row" @mouseenter="onRowEnter(file as FileInfo, $event)" @mouseleave="onRowLeave">
            <div class="td td-name">
              <IconFile :size="18" />
              <span :title="(file as FileInfo).name">{{ (file as FileInfo).name }}</span>
            </div>

            <div class="td td-path" :title="(file as FileInfo).path">{{ (file as FileInfo).path }}</div>
            <div class="td td-size">{{ formatBytes((file as FileInfo).size) }}</div>
            <div class="td td-modified">{{ formatDate((file as FileInfo).modified_at) }}</div>

            <div class="td td-actions">
              <button
                class="action-btn migrate-btn"
                @click.stop="$emit('migrate-file', (file as FileInfo))"
                :disabled="!hasDeepScanned"
                :title="!hasDeepScanned ? '请先做一次深度扫描' : '搬到其他盘（原位置留链接，应用感知不到）'"
              >
                <IconMigrate :size="16" />
              </button>
            </div>
          </div>
        </VirtualList>
      </div>

      <div class="table-footer">
        <span>{{ formatNumber(loaded) }} / {{ formatNumber(total) }} 已加载</span>
        <span v-if="loadingMore">正在加载更多...</span>
        <button v-else-if="hasMore" class="load-more-btn" @click="loadNextPage">加载更多</button>
        <span v-else>已加载全部</span>
      </div>
    </div>

    <ExplanationTooltip ref="tooltipRef" />
  </div>
</template>

<style scoped>
.large-files-view {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  height: 100%;
}

.summary-bar {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1rem;
  padding: 1rem 1.1rem;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.76), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.summary-bar h3 {
  font-size: 1.15rem;
  margin-bottom: 0.24rem;
}

.summary-bar p {
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--color-text-tertiary);
}

.summary-metrics {
  display: flex;
  gap: 0.7rem;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.summary-pill {
  min-width: 6.8rem;
  padding: 0.72rem 0.9rem;
  border-radius: var(--radius-md);
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid rgba(46, 33, 18, 0.08);
}

.summary-pill span {
  display: block;
  margin-bottom: 0.2rem;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.summary-pill strong {
  font-size: 0.94rem;
  font-weight: 800;
  color: var(--color-text-primary);
}

.summary-pill.muted strong {
  color: var(--color-text-tertiary);
}

.table-shell,
.empty {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.76), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.empty {
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  padding: 3rem 2rem;
}

.empty-icon {
  color: rgba(73, 68, 60, 0.34);
}

.empty h3 {
  color: var(--color-text-primary);
}

.empty p {
  color: var(--color-text-tertiary);
}

.table-header,
.table-row {
  display: grid;
  grid-template-columns: minmax(180px, 1fr) minmax(220px, 2fr) minmax(110px, 130px) minmax(120px, 150px) 56px;
  gap: 1rem;
  align-items: center;
}

.table-header {
  padding: 0.95rem 1rem;
  border-bottom: 1px solid var(--color-border-light);
  background: rgba(244, 239, 232, 0.94);
}

.th {
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--color-text-tertiary);
}

.table-body {
  flex: 1;
  min-height: 0;
  padding: 0.3rem 0;
}

.table-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.7rem 1rem;
  border-top: 1px solid var(--color-border-light);
  color: var(--color-text-tertiary);
  font-size: 0.78rem;
  font-weight: 700;
}

.load-more-btn {
  border: 1px solid var(--color-border-light);
  border-radius: 0.78rem;
  padding: 0.42rem 0.7rem;
  background: rgba(255, 255, 255, 0.72);
  color: var(--color-text-secondary);
  font-size: 0.78rem;
  font-weight: 800;
  cursor: pointer;
  transition: transform var(--transition-base), background var(--transition-base), color var(--transition-fast);
}

.load-more-btn:hover {
  transform: translateY(-1px);
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.table-row {
  margin: 0 0.6rem;
  padding: 0.82rem 0.4rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  height: 56px;
  box-sizing: border-box;
  transition: background var(--transition-base), border-color var(--transition-base);
}

.table-row:hover {
  background: rgba(255, 255, 255, 0.72);
  border-color: rgba(46, 33, 18, 0.08);
  box-shadow: var(--shadow-xs);
}

.td {
  display: flex;
  align-items: center;
  min-width: 0;
  font-size: 0.86rem;
}

.td-name {
  gap: 0.65rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.td-name span,
.td-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-path,
.td-modified {
  color: var(--color-text-secondary);
}

.td-size {
  color: var(--color-text-primary);
  font-weight: 700;
}

.td-actions {
  justify-content: center;
}

.action-btn {
  width: 2.3rem;
  height: 2.3rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-border-light);
  border-radius: 0.9rem;
  background: rgba(255, 255, 255, 0.72);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base), color var(--transition-fast), opacity var(--transition-fast);
}

.migrate-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: linear-gradient(135deg, var(--color-accent-primary), var(--color-accent-secondary));
  color: var(--color-text-inverse);
  box-shadow: var(--shadow-sm);
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

@media (max-width: 980px) {
  .summary-bar {
    flex-direction: column;
  }

  .summary-metrics {
    width: 100%;
    justify-content: flex-start;
  }

  .table-header,
  .table-row {
    grid-template-columns: minmax(0, 1fr) minmax(100px, 120px) 56px;
  }

  .th-path,
  .td-path,
  .th-modified,
  .td-modified {
    display: none;
  }
}

@media (max-width: 640px) {
  .summary-bar,
  .table-header {
    padding: 0.82rem;
  }

  .table-row {
    margin: 0 0.45rem;
    padding: 0.76rem 0.32rem;
  }

  .table-header,
  .table-row {
    grid-template-columns: minmax(0, 1fr) 56px;
  }

  .th-size,
  .td-size {
    display: none;
  }
}

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .summary-bar,
[data-theme="dark"] .table-shell,
[data-theme="dark"] .empty {
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .summary-pill {
  background: rgba(255, 255, 255, 0.04);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .table-header {
  background: rgba(255, 255, 255, 0.04);
  border-bottom-color: var(--color-border-medium);
}

[data-theme="dark"] .table-footer {
  border-top-color: var(--color-border-medium);
}

[data-theme="dark"] .table-row:hover {
  background: rgba(255, 255, 255, 0.05);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .action-btn {
  background: rgba(255, 255, 255, 0.05);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .load-more-btn {
  background: rgba(255, 255, 255, 0.05);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .empty-icon {
  color: rgba(243, 244, 246, 0.32);
}
</style>
