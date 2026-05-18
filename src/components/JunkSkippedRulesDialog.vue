<script setup lang="ts">
import { computed } from 'vue';

interface Props {
  show: boolean;
  ruleIds: string[];
}

const props = defineProps<Props>();

const emit = defineEmits<{
  close: [];
  rescan: [];
}>();

type Reason = 'admin' | 'missing' | 'unknown';

interface RuleMeta {
  name: string;
  reason: Reason;
}

const RULE_META: Record<string, RuleMeta> = {
  windows_update_download: { name: 'Windows Update 下载缓存', reason: 'admin' },
  windows_update_logs: { name: 'Windows Update 日志', reason: 'admin' },
  font_cache: { name: '字体缓存', reason: 'admin' },
  firefox_cache: { name: 'Firefox 缓存', reason: 'missing' },
  edge_cache: { name: 'Edge 缓存', reason: 'missing' },
  chrome_cache: { name: 'Chrome 缓存', reason: 'missing' },
};

const REASON_TEXT: Record<Reason, string> = {
  admin: '需要管理员权限',
  missing: '路径不存在或软件未安装',
  unknown: '已跳过',
};

interface RuleItem {
  id: string;
  name: string;
  reason: Reason;
  reasonText: string;
}

const items = computed<RuleItem[]>(() =>
  props.ruleIds.map((id) => {
    const meta = RULE_META[id];
    if (meta) {
      return { id, name: meta.name, reason: meta.reason, reasonText: REASON_TEXT[meta.reason] };
    }
    return { id, name: id, reason: 'unknown', reasonText: REASON_TEXT.unknown };
  })
);

const count = computed(() => items.value.length);

const hasAdmin = computed(() => items.value.some((item) => item.reason === 'admin'));
</script>

<template>
  <div v-if="show" class="skipped-overlay" @click.self="emit('close')">
    <div class="skipped-dialog" @click.stop>
      <button class="skipped-close" type="button" aria-label="关闭" @click="emit('close')">×</button>
      <h3>已跳过 {{ count }} 条规则</h3>
      <p class="skipped-subtitle">这些规则因为下面原因没扫描，可能漏报一部分垃圾。</p>
      <ul v-if="count > 0" class="skipped-list">
        <li v-for="item in items" :key="item.id" class="skipped-item">
          <div class="skipped-item-main">
            <span class="skipped-item-name">{{ item.name }}</span>
            <span class="skipped-item-reason">{{ item.reasonText }}</span>
          </div>
          <span class="skipped-tag" :class="`skipped-tag-${item.reason}`">{{ item.reasonText }}</span>
        </li>
      </ul>
      <div v-else class="skipped-empty">没有被跳过的规则。</div>
      <div class="skipped-actions">
        <button v-if="hasAdmin" class="btn btn-primary" type="button" @click="emit('rescan')">
          以管理员身份重扫
        </button>
        <button class="btn btn-secondary" type="button" @click="emit('close')">关闭</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.skipped-overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
  animation: skippedFadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans);
}

@keyframes skippedFadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.skipped-dialog {
  position: relative;
  width: 90%;
  max-width: 520px;
  padding: 2rem 1.75rem 1.5rem;
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: 24px;
  box-shadow:
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  animation: skippedSlideUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

@keyframes skippedSlideUp {
  from {
    opacity: 0;
    transform: translateY(20px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.skipped-close {
  position: absolute;
  top: 0.75rem;
  right: 1rem;
  width: 32px;
  height: 32px;
  border: none;
  background: transparent;
  font-size: 1.5rem;
  line-height: 1;
  color: var(--color-text-tertiary);
  cursor: pointer;
  border-radius: 8px;
  transition: all var(--transition-base);
}

.skipped-close:hover {
  background: rgba(139, 92, 46, 0.12);
  color: var(--color-text-primary);
}

.skipped-dialog h3 {
  font-family: var(--font-serif);
  font-size: 1.375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0 0 0.5rem;
  letter-spacing: 0;
}

.skipped-subtitle {
  font-size: 0.9375rem;
  color: var(--color-text-tertiary);
  line-height: 1.6;
  margin: 0 0 1.25rem;
}

.skipped-list {
  list-style: none;
  margin: 0 0 1.5rem;
  padding: 0;
  max-height: 420px;
  overflow-y: auto;
  border-radius: 14px;
  background: rgba(139, 92, 46, 0.04);
  border: 1px solid var(--color-border-light);
}

.skipped-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.875rem 1rem;
  border-bottom: 1px solid var(--color-border-light);
}

.skipped-item:last-child {
  border-bottom: none;
}

.skipped-item-main {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-width: 0;
  flex: 1;
}

.skipped-item-name {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  word-break: break-all;
}

.skipped-item-reason {
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
}

.skipped-tag {
  flex-shrink: 0;
  padding: 0.25rem 0.625rem;
  border-radius: 999px;
  font-size: 0.75rem;
  font-weight: 600;
  white-space: nowrap;
  border: 1px solid currentColor;
}

.skipped-tag-admin {
  color: var(--color-warning);
  background: rgba(245, 158, 11, 0.1);
}

.skipped-tag-missing {
  color: var(--color-text-tertiary);
  background: rgba(139, 92, 46, 0.08);
}

.skipped-tag-unknown {
  color: var(--color-text-tertiary);
  background: rgba(139, 92, 46, 0.08);
}

.skipped-empty {
  padding: 2rem 1rem;
  text-align: center;
  color: var(--color-text-tertiary);
  font-size: 0.9375rem;
  margin-bottom: 1.5rem;
  border-radius: 14px;
  background: rgba(139, 92, 46, 0.04);
  border: 1px dashed var(--color-border-light);
}

.skipped-actions {
  display: flex;
  gap: 0.75rem;
  justify-content: flex-end;
}

.btn {
  min-width: 120px;
  padding: 0.75rem 1.5rem;
  font-size: 0.9375rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition-base);
  letter-spacing: 0;
}

.btn-secondary {
  color: #1a1a1a;
  background: rgba(139, 92, 46, 0.12);
  border: 1px solid var(--color-border-strong);
}

.btn-secondary:hover {
  background: rgba(139, 92, 46, 0.18);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

.btn-primary {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-primary) 0%, var(--color-primary-dark, #8b5c2e) 100%);
  box-shadow:
    0 0 0 1px rgba(139, 92, 46, 0.2),
    0 4px 16px rgba(139, 92, 46, 0.3);
}

.btn-primary:hover {
  transform: translateY(-3px);
  box-shadow:
    0 0 0 1px rgba(139, 92, 46, 0.3),
    0 8px 24px rgba(139, 92, 46, 0.4);
}

@media (max-width: 480px) {
  .skipped-dialog {
    width: 95%;
    padding: 1.75rem 1.25rem 1.25rem;
  }

  .skipped-actions {
    flex-direction: column-reverse;
  }

  .btn {
    width: 100%;
  }

  .skipped-item {
    flex-wrap: wrap;
  }
}
</style>
