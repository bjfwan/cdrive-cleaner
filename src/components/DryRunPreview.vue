<script setup lang="ts">
import { computed, ref } from 'vue';
import type { DryRunReport, DrySkipReason } from '../types/junk';
import { SKIP_REASON_LABEL } from '../types/junk';
import MiddlePath from './MiddlePath.vue';
import IconClose from './icons/common/IconClose.vue';

interface Props {
  show: boolean;
  report: DryRunReport | null;
  busy?: boolean;
  mode?: 'recycle' | 'permanent';
}

const props = withDefaults(defineProps<Props>(), {
  busy: false,
  mode: 'recycle',
});

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const previewLimit = ref(40);

const deleteCount = computed(() => props.report?.will_delete.length ?? 0);
const skipCount = computed(() => props.report?.will_skip.length ?? 0);
const totalFreedLabel = computed(() => {
  const mb = props.report?.estimated_freed_mb ?? 0;
  if (mb < 1024) return `${mb} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
});

const etaLabel = computed(() => {
  const s = props.report?.estimated_seconds ?? 0;
  if (s < 60) return `约 ${s} 秒`;
  return `约 ${Math.ceil(s / 60)} 分钟`;
});

const visibleDelete = computed(() => {
  const list = props.report?.will_delete ?? [];
  return list.slice(0, previewLimit.value);
});

const hiddenDelete = computed(() => Math.max(0, deleteCount.value - visibleDelete.value.length));

interface SkipGroup {
  key: string;
  reason: DrySkipReason['reason'];
  label: string;
  count: number;
  samples: DrySkipReason[];
}

const skipGroups = computed<SkipGroup[]>(() => {
  const list = props.report?.will_skip ?? [];
  const map = new Map<string, SkipGroup>();
  for (const item of list) {
    let g = map.get(item.reason);
    if (!g) {
      g = {
        key: item.reason,
        reason: item.reason,
        label: SKIP_REASON_LABEL[item.reason] ?? item.reason,
        count: 0,
        samples: [],
      };
      map.set(item.reason, g);
    }
    g.count += 1;
    if (g.samples.length < 5) g.samples.push(item);
  }
  return Array.from(map.values()).sort((a, b) => b.count - a.count);
});

const confirmLabel = computed(() => {
  if (props.busy) return '清理中…';
  if (deleteCount.value === 0) return '没有可清理项';
  return props.mode === 'permanent'
    ? `永久删除 ${deleteCount.value} 项`
    : `确认清理 ${deleteCount.value} 项`;
});

const confirmDisabled = computed(() => props.busy || deleteCount.value === 0);

function expand() {
  previewLimit.value = Math.min(previewLimit.value + 40, deleteCount.value);
}

function onCancel() {
  if (props.busy) return;
  emit('cancel');
}

function onConfirm() {
  if (confirmDisabled.value) return;
  emit('confirm');
}
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="dry-overlay" @click.self="onCancel">
      <div class="dry-dialog" role="dialog" aria-labelledby="dry-title">
        <header class="dry-head">
          <div>
            <h3 id="dry-title">清理预览</h3>
            <p>开始清理前的安全检查。所有可疑路径已自动跳过。</p>
          </div>
          <button class="dry-close" type="button" :disabled="busy" aria-label="关闭" @click="onCancel">
            <IconClose :size="18" />
          </button>
        </header>

        <section class="dry-stats">
          <div class="dry-stat dry-stat--go">
            <span class="dry-stat__num">{{ deleteCount }}</span>
            <span class="dry-stat__lbl">将清理</span>
          </div>
          <div class="dry-stat dry-stat--skip">
            <span class="dry-stat__num">{{ skipCount }}</span>
            <span class="dry-stat__lbl">已跳过</span>
          </div>
          <div class="dry-stat dry-stat--size">
            <span class="dry-stat__num">{{ totalFreedLabel }}</span>
            <span class="dry-stat__lbl">预计释放</span>
          </div>
          <div class="dry-stat dry-stat--eta">
            <span class="dry-stat__num">{{ etaLabel }}</span>
            <span class="dry-stat__lbl">预计耗时</span>
          </div>
        </section>

        <section v-if="skipGroups.length > 0" class="dry-skip">
          <h4>跳过原因</h4>
          <div class="dry-skip-grid">
            <article v-for="g in skipGroups" :key="g.key" class="dry-skip-card">
              <header>
                <strong>{{ g.label }}</strong>
                <span>{{ g.count }} 项</span>
              </header>
              <ul>
                <li v-for="s in g.samples" :key="s.path">
                  <MiddlePath :path="s.path" />
                  <small v-if="s.detail">{{ s.detail }}</small>
                </li>
                <li v-if="g.count > g.samples.length" class="dry-skip-more">
                  还有 {{ g.count - g.samples.length }} 项同类
                </li>
              </ul>
            </article>
          </div>
        </section>

        <section class="dry-list">
          <header class="dry-list-head">
            <h4>清理清单 · 前 {{ visibleDelete.length }} / {{ deleteCount }}</h4>
            <button v-if="hiddenDelete > 0" type="button" class="dry-expand" @click="expand">
              展开下一批
            </button>
          </header>
          <div v-if="deleteCount === 0" class="dry-empty">没有满足条件的清理目标。</div>
          <ul v-else class="dry-rows">
            <li v-for="item in visibleDelete" :key="item.path" class="dry-row">
              <MiddlePath class="dry-row-path" :path="item.path" />
              <div class="dry-row-meta">
                <span>{{ item.size_mb }} MB</span>
                <span>{{ item.mtime_iso || '时间未知' }}</span>
                <span v-if="item.is_symlink" class="dry-row-symlink">符号链接</span>
              </div>
            </li>
          </ul>
        </section>

        <footer class="dry-foot">
          <button class="btn-base btn-secondary" type="button" :disabled="busy" @click="onCancel">取消</button>
          <button
            class="btn-base"
            :class="mode === 'permanent' ? 'btn-danger' : 'btn-primary'"
            type="button"
            :disabled="confirmDisabled"
            @click="onConfirm"
          >
            {{ confirmLabel }}
          </button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dry-overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  z-index: 9400;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
}

.dry-dialog {
  width: min(720px, 100%);
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-primary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-medium);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
}

.dry-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 1.25rem 1.4rem;
  border-bottom: 1px solid var(--color-border-light);
}

.dry-head h3 {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.dry-head p {
  margin-top: 0.2rem;
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
}

.dry-close {
  background: transparent;
  border: none;
  color: var(--color-text-secondary);
  cursor: pointer;
  padding: 0.3rem;
  border-radius: var(--radius-xs);
}

.dry-close:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.dry-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.6rem;
  padding: 1rem 1.4rem;
  background: var(--color-surface);
  border-bottom: 1px solid var(--color-border-light);
}

.dry-stat {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
  padding: 0.65rem 0.8rem;
  border-radius: var(--radius-sm);
  background: var(--color-surface-strong);
  border: 1px solid var(--color-border-light);
}

.dry-stat__num {
  font-size: 1.05rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
}

.dry-stat__lbl {
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
}

.dry-stat--go .dry-stat__num { color: var(--color-success); }
.dry-stat--skip .dry-stat__num { color: var(--color-warning); }

.dry-skip {
  padding: 1rem 1.4rem;
  border-bottom: 1px solid var(--color-border-light);
}

.dry-skip h4,
.dry-list-head h4 {
  font-size: 0.86rem;
  font-weight: 700;
  color: var(--color-text-primary);
  margin-bottom: 0.55rem;
}

.dry-skip-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 0.6rem;
}

.dry-skip-card {
  background: var(--color-surface-strong);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  padding: 0.65rem 0.75rem;
}

.dry-skip-card header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.45rem;
}

.dry-skip-card header strong {
  font-size: 0.82rem;
  color: var(--color-text-primary);
}

.dry-skip-card header span {
  font-size: 0.74rem;
  color: var(--color-warning);
  font-weight: 700;
}

.dry-skip-card ul {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.32rem;
}

.dry-skip-card li {
  font-size: 0.76rem;
  color: var(--color-text-secondary);
  word-break: break-all;
}

.dry-skip-card li small {
  display: block;
  color: var(--color-text-tertiary);
  font-size: 0.72rem;
  margin-top: 0.12rem;
}

.dry-skip-more {
  color: var(--color-text-tertiary) !important;
  font-style: italic;
}

.dry-list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0.9rem 1.4rem;
  overflow: hidden;
}

.dry-list-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.dry-expand {
  border: 1px solid var(--color-border-medium);
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  padding: 0.28rem 0.65rem;
  border-radius: var(--radius-xs);
  font-size: 0.74rem;
  font-weight: 600;
  cursor: pointer;
}

.dry-expand:hover {
  color: var(--color-text-primary);
  border-color: var(--color-border-strong);
}

.dry-rows {
  list-style: none;
  padding: 0;
  margin-top: 0.45rem;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.dry-row {
  display: flex;
  flex-direction: column;
  gap: 0.18rem;
  padding: 0.45rem 0.6rem;
  border-radius: var(--radius-xs);
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
}

.dry-row-path {
  font-size: 0.82rem;
  color: var(--color-text-primary);
}

.dry-row-meta {
  display: flex;
  gap: 0.85rem;
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  font-feature-settings: 'tnum';
}

.dry-row-symlink {
  color: var(--color-info);
  font-weight: 700;
}

.dry-empty {
  padding: 1rem 0;
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
}

.dry-foot {
  display: flex;
  justify-content: flex-end;
  gap: 0.6rem;
  padding: 1rem 1.4rem;
  border-top: 1px solid var(--color-border-light);
  background: var(--color-surface);
}
</style>
