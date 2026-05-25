<script setup lang="ts">
import { computed } from 'vue';
import { IconMigrate, IconClose, IconInfo } from './icons';
import { formatBytes } from '../utils/format';
import type { AiSuggestion } from '../store/ai';

interface Props {
  suggestion: AiSuggestion;
  applying?: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'accept': [suggestion: AiSuggestion];
  'dismiss': [suggestion: AiSuggestion];
}>();

const estimatedSaveLabel = computed(() => {
  const mb = props.suggestion.estimated_savings_mb;
  if (!mb || mb <= 0) return '估算节省 未知';
  return `预计节省 ${formatBytes(mb * 1024 * 1024)}`;
});
</script>

<template>
  <article class="ai-card">
    <header class="ai-card-head">
      <div class="ai-card-title">
        <span class="ai-card-label">{{ suggestion.target_label }}</span>
        <span class="ai-card-action">{{ suggestion.action }}</span>
      </div>
      <span class="ai-card-savings">{{ estimatedSaveLabel }}</span>
    </header>

    <div class="ai-card-reason">
      <IconInfo :size="16" />
      <p>{{ suggestion.reason }}</p>
    </div>

    <footer class="ai-card-actions">
      <button
        class="ai-btn ai-btn-ghost"
        :disabled="applying"
        @click="emit('dismiss', suggestion)"
      >
        <IconClose :size="14" />
        <span>忽略</span>
      </button>
      <button
        class="ai-btn ai-btn-primary"
        :disabled="applying"
        @click="emit('accept', suggestion)"
      >
        <IconMigrate :size="14" />
        <span>{{ applying ? '准备中…' : '采纳' }}</span>
      </button>
    </footer>
  </article>
</template>

<style scoped>
.ai-card {
  background: var(--surface, #ffffff);
  border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
  border-radius: 14px;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
  transition: box-shadow 0.16s ease, transform 0.16s ease;
}

.ai-card:hover {
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.07);
  transform: translateY(-1px);
}

.ai-card-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.ai-card-title {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.ai-card-label {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary, #1d1d1f);
  word-break: break-word;
}

.ai-card-action {
  font-size: 13px;
  color: var(--text-secondary, #6b7280);
}

.ai-card-savings {
  flex-shrink: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--risk-safe-text);
  background: var(--risk-safe-soft);
  padding: 4px 10px;
  border-radius: 999px;
  white-space: nowrap;
}

.ai-card-reason {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  color: var(--text-secondary, #4b5563);
  background: rgba(15, 23, 42, 0.03);
  padding: 10px 12px;
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.5;
}

.ai-card-reason p {
  margin: 0;
}

.ai-card-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.ai-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 8px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.ai-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ai-btn-ghost {
  background: transparent;
  color: var(--text-secondary, #4b5563);
  border-color: var(--border, rgba(0, 0, 0, 0.12));
}

.ai-btn-ghost:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
}

.ai-btn-primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
}

.ai-btn-primary:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-highlight) 88%, #000 12%);
}

[data-theme="dark"] .ai-card {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .ai-card-label {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-card-action {
  color: #aab2c0;
}

[data-theme="dark"] .ai-card-reason {
  background: rgba(255, 255, 255, 0.05);
  color: #cdd3df;
}

[data-theme="dark"] .ai-card-savings {
  background: var(--risk-safe-soft);
  color: var(--risk-safe-text);
}

[data-theme="dark"] .ai-btn-ghost {
  border-color: rgba(255, 255, 255, 0.14);
  color: #cdd3df;
}

[data-theme="dark"] .ai-btn-ghost:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
}
</style>
