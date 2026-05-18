<script setup lang="ts">
import { computed, ref } from 'vue';
import type { SmartGroup, SmartItem, SmartRisk, SmartCategory, SmartAction } from '../types';
import { formatBytes } from '../utils/format';
import { useCart } from '../composables/useCart';

interface Props {
  group: SmartGroup;
}

const props = defineProps<Props>();
const emit = defineEmits<{ migrate: [item: SmartItem]; reveal: [path: string] }>();

const expanded = ref(false);
const cart = useCart();

const categoryMeta: Record<SmartCategory, { label: string; sub: string; tone: string }> = {
  app_cache: { label: '应用缓存', sub: '浏览器、IDE、桌面应用累积的缓存', tone: 'cyan' },
  dev_tools: { label: '开发工具', sub: 'node_modules / 包管理器缓存 / 构建工具', tone: 'amber' },
  temp_files: { label: '临时文件', sub: '系统与用户 Temp、日志、崩溃转储', tone: 'rose' },
  large_files: { label: '大文件', sub: '单个文件大于 1 GB', tone: 'violet' },
};

const actionMeta: Record<SmartAction, { label: string; verb: string }> = {
  migrate: { label: '推荐迁移', verb: '搬走' },
  delete: { label: '可删除', verb: '清掉' },
  review: { label: '人工判断', verb: '查看' },
};

const riskMeta: Record<SmartRisk, { label: string; tone: string }> = {
  safe: { label: '安全', tone: 'safe' },
  caution: { label: '注意', tone: 'caution' },
  blocked: { label: '阻塞', tone: 'blocked' },
  unknown: { label: '未评估', tone: 'unknown' },
};

const meta = computed(() => categoryMeta[props.group.category]);
const action = computed(() => actionMeta[props.group.recommendation]);

const visibleItems = computed(() =>
  expanded.value ? props.group.items : props.group.items.slice(0, 4),
);
const remainingCount = computed(() => Math.max(0, props.group.items.length - 4));

/**
 * Build a Set of paths that are currently in the cart for this group's
 * items in a single pass. Each item lookup in the template is then O(1)
 * against the Set instead of O(N) `cart.has` calls per render. The
 * `computed` re-runs whenever cart membership changes because `cart.has`
 * reads the underlying reactive Map.
 */
const checkedSet = computed(() => {
  const set = new Set<string>();
  for (const it of props.group.items) {
    if (cart.has(it.path)) set.add(it.path.toLowerCase());
  }
  return set;
});

const checkedInGroup = computed(() => checkedSet.value.size);
const allSafeChecked = computed(() => {
  let safeCount = 0;
  for (const it of props.group.items) {
    if (it.risk !== 'blocked') safeCount += 1;
  }
  return safeCount > 0 && checkedInGroup.value >= safeCount;
});

function isItemChecked(path: string) {
  return checkedSet.value.has(path.toLowerCase());
}

function toggleItem(item: SmartItem) {
  cart.toggle({
    path: item.path,
    name: item.name,
    size: item.size,
    file_count: item.file_count,
    recommendation: item.recommendation,
    source: item.category === 'large_files' ? 'large_file' : 'smart_scan',
  });
}

function selectAll() {
  const batch: Array<{
    path: string;
    name: string;
    size: number;
    file_count: number;
    recommendation: SmartItem['recommendation'];
    source: 'smart_scan';
  }> = [];
  for (const it of props.group.items) {
    if (it.risk !== 'blocked') {
      batch.push({
        path: it.path,
        name: it.name,
        size: it.size,
        file_count: it.file_count,
        recommendation: it.recommendation,
        source: 'smart_scan',
      });
    }
  }
  cart.addBatch(batch);
}

function deselectAll() {
  const paths: string[] = [];
  for (const it of props.group.items) paths.push(it.path);
  cart.removeBatch(paths);
}
</script>

<template>
  <section class="group" :data-tone="meta.tone">
    <header class="group-head">
      <div class="head-text">
        <div class="head-title">
          <h3>{{ meta.label }}</h3>
          <span class="head-action">{{ action.label }}</span>
        </div>
        <p>{{ meta.sub }}</p>
      </div>
      <div class="head-stats">
        <div class="size">
          <strong>{{ formatBytes(group.total_size) }}</strong>
          <small>{{ group.item_count }} 项 · 已选 {{ checkedInGroup }}</small>
        </div>
        <button class="bulk-btn" @click="allSafeChecked ? deselectAll() : selectAll()">
          {{ allSafeChecked ? '全部取消' : '全选安全项' }}
        </button>
      </div>
    </header>

    <ul class="items">
      <li
        v-for="item in visibleItems"
        :key="item.path"
        class="item"
        :data-risk="item.risk"
        :class="{ checked: isItemChecked(item.path), blocked: item.risk === 'blocked' }"
      >
        <button class="item-check" @click="toggleItem(item)" :disabled="item.risk === 'blocked'">
          <svg v-if="isItemChecked(item.path)" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path d="M3 8.5L6.5 12L13 4.5" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" fill="none" />
          </svg>
        </button>
        <div class="item-main">
          <div class="item-name-row">
            <span class="item-name" :title="item.path">{{ item.name }}</span>
            <span class="item-rule">{{ item.rule }}</span>
            <span class="risk" :data-risk="item.risk">{{ riskMeta[item.risk].label }}</span>
          </div>
          <div class="item-path" :title="item.path">{{ item.path }}</div>
        </div>
        <div class="item-size">{{ formatBytes(item.size) }}</div>
        <button class="reveal" @click="emit('reveal', item.path)" title="在浏览器里定位">↗</button>
      </li>
    </ul>

    <button v-if="remainingCount > 0" class="expand" @click="expanded = !expanded">
      {{ expanded ? '收起' : `展开剩余 ${remainingCount} 项` }}
    </button>
  </section>
</template>

<style scoped>
.group {
  display: flex;
  flex-direction: column;
  border-radius: 20px;
  border: 1px solid var(--color-border-light);
  background: var(--color-surface-strong);
  box-shadow: var(--shadow-xs);
  overflow: hidden;
}

.group-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
  padding: 0.85rem 1.05rem;
  border-bottom: 1px solid var(--color-border-light);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.6), rgba(255, 255, 255, 0));
}

.head-text { min-width: 0; }
.head-title { display: flex; align-items: center; gap: 0.6rem; }
.head-title h3 { font-size: 1rem; font-weight: 700; color: var(--color-text-primary); }
.head-action {
  padding: 0.16rem 0.5rem;
  border-radius: 999px;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.head-text p { margin-top: 0.22rem; font-size: 0.78rem; color: var(--color-text-tertiary); }

.head-stats { display: flex; align-items: center; gap: 0.7rem; flex-shrink: 0; }
.size { text-align: right; }
.size strong {
  font-size: 1.1rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
}
.size small { display: block; font-size: 0.7rem; color: var(--color-text-tertiary); margin-top: 0.16rem; }

.bulk-btn {
  padding: 0.5rem 0.85rem;
  border-radius: 12px;
  border: 1px solid var(--color-border-light);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 0.8rem;
  font-weight: 600;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
}
.bulk-btn:hover { background: var(--color-surface-hover); color: var(--color-text-primary); border-color: var(--color-border-medium); }

.items { list-style: none; }

.item {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.7rem;
  padding: 0.55rem 1.05rem;
  border-bottom: 1px solid var(--color-border-light);
  transition: background var(--transition-fast);
}
.item:last-child { border-bottom: none; }
.item:hover { background: rgba(15, 23, 32, 0.02); }
.item.checked { background: rgba(15, 118, 110, 0.04); }
.item.blocked { opacity: 0.5; }

.item-check {
  width: 20px;
  height: 20px;
  border-radius: 6px;
  border: 1.5px solid var(--color-border-medium);
  background: var(--color-surface);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: white;
  transition: background var(--transition-fast), border-color var(--transition-fast);
}
.item-check:hover:not(:disabled) { border-color: var(--color-text-secondary); }
.item-check:disabled { cursor: not-allowed; }
.item.checked .item-check {
  background: var(--color-highlight);
  border-color: var(--color-highlight);
}

.item-main { min-width: 0; }
.item-name-row { display: flex; align-items: center; gap: 0.55rem; }
.item-name {
  font-weight: 600;
  color: var(--color-text-primary);
  font-size: 0.92rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.item-rule {
  padding: 0.1rem 0.45rem;
  border-radius: 999px;
  background: rgba(32, 45, 58, 0.06);
  color: var(--color-text-tertiary);
  font-size: 0.7rem;
}
.risk {
  padding: 0.1rem 0.45rem;
  border-radius: 999px;
  font-size: 0.7rem;
  font-weight: 700;
}
.risk[data-risk='safe'] { background: rgba(15, 159, 110, 0.12); color: #0d8a5f; }
.risk[data-risk='caution'] { background: rgba(217, 119, 6, 0.12); color: #b45309; }
.risk[data-risk='blocked'] { background: rgba(220, 38, 38, 0.12); color: #b91c1c; }
.risk[data-risk='unknown'] { background: rgba(107, 114, 128, 0.12); color: #4b5563; }

.item-path {
  margin-top: 0.18rem;
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--font-mono);
}

.item-size {
  font-size: 0.92rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  text-align: right;
  min-width: 5.5rem;
}

.reveal {
  width: 28px;
  height: 28px;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  font-size: 0.95rem;
  transition: background var(--transition-fast), color var(--transition-fast);
}
.reveal:hover { background: rgba(32, 45, 58, 0.06); color: var(--color-text-primary); }

.expand {
  padding: 0.7rem 1.25rem;
  border: none;
  border-top: 1px solid var(--color-border-light);
  background: rgba(255, 255, 255, 0.4);
  color: var(--color-text-secondary);
  font-size: 0.84rem;
  font-weight: 600;
  cursor: pointer;
  text-align: center;
  transition: background var(--transition-fast), color var(--transition-fast);
}
.expand:hover { background: rgba(255, 255, 255, 0.7); color: var(--color-text-primary); }
</style>
