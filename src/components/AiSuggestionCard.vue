<script setup lang="ts">
import { computed } from 'vue';
import { IconMigrate, IconClose, IconSparkles } from './icons';
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
  if (!mb || mb <= 0) return null;
  return formatBytes(mb * 1024 * 1024);
});

const actionTone = computed(() => {
  const action = (props.suggestion.action || '').toLowerCase();
  if (action.includes('删除') || action.includes('delete') || action.includes('清理') || action.includes('clean')) {
    return 'danger';
  }
  if (action.includes('迁移') || action.includes('move') || action.includes('migrate')) {
    return 'primary';
  }
  return 'neutral';
});

const canAccept = computed(() => (props.suggestion.action || '').toLowerCase() === 'migrate');
</script>

<template>
  <article class="ai-card" :data-tone="actionTone">
    <div class="ai-card-stripe" aria-hidden="true"></div>

    <header class="ai-card-head">
      <div class="ai-card-icon">
        <IconSparkles :size="14" />
      </div>
      <div class="ai-card-title">
        <span class="ai-card-label">{{ suggestion.target_label }}</span>
        <span class="ai-card-action">{{ suggestion.action }}</span>
      </div>
      <span v-if="estimatedSaveLabel" class="ai-card-savings">
        <span class="ai-card-savings-dot"></span>
        <span>预计节省 {{ estimatedSaveLabel }}</span>
      </span>
    </header>

    <div class="ai-card-reason">
      <p>{{ suggestion.reason }}</p>
    </div>

    <footer class="ai-card-actions">
      <button
        class="ai-card-btn ai-card-btn--ghost"
        :disabled="applying"
        @click="emit('dismiss', suggestion)"
      >
        <IconClose :size="14" />
        <span>忽略</span>
      </button>
      <button
        v-if="canAccept"
        class="ai-card-btn ai-card-btn--primary"
        :disabled="applying"
        @click="emit('accept', suggestion)"
      >
        <span v-if="!applying" class="ai-card-btn-icon"><IconMigrate :size="14" /></span>
        <span v-else class="ai-card-btn-spinner" aria-hidden="true"></span>
        <span>{{ applying ? '准备中…' : '采纳并迁移' }}</span>
      </button>
      <span v-else class="ai-card-unavailable">请在对应功能中处理</span>
    </footer>
  </article>
</template>

<style scoped>
.ai-card {
  position: relative;
  background: var(--surface, #ffffff);
  border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
  border-radius: 16px;
  padding: 18px 20px 16px 22px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.04);
  transition: box-shadow 0.2s ease, transform 0.2s ease, border-color 0.2s ease;
  overflow: hidden;
}

.ai-card:hover {
  border-color: rgba(168, 85, 247, 0.32);
  box-shadow: 0 10px 28px -16px rgba(168, 85, 247, 0.35);
  transform: translateY(-1px);
}

.ai-card-stripe {
  position: absolute;
  left: 0;
  top: 12%;
  bottom: 12%;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: linear-gradient(180deg, #a855f7, #14b8a6);
}

.ai-card[data-tone="danger"] .ai-card-stripe {
  background: linear-gradient(180deg, #f43f5e, #f59e0b);
}

.ai-card[data-tone="neutral"] .ai-card-stripe {
  background: linear-gradient(180deg, #94a3b8, #64748b);
}

.ai-card-head {
  display: flex;
  align-items: center;
  gap: 12px;
}

.ai-card-icon {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.18) 0%, rgba(20, 184, 166, 0.18) 100%);
  color: #a855f7;
}

.ai-card[data-tone="danger"] .ai-card-icon {
  background: linear-gradient(135deg, rgba(244, 63, 94, 0.18) 0%, rgba(245, 158, 11, 0.18) 100%);
  color: #f43f5e;
}

.ai-card-title {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.ai-card-label {
  font-size: 14.5px;
  font-weight: 700;
  color: var(--text-primary, #1d1d1f);
  word-break: break-word;
  letter-spacing: -0.005em;
}

.ai-card-action {
  font-size: 12px;
  color: var(--text-secondary, #6b7280);
  letter-spacing: 0.01em;
}

.ai-card-savings {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 11px;
  border-radius: 999px;
  background: rgba(20, 184, 166, 0.12);
  color: #0d9488;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}

.ai-card-savings-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #14b8a6;
  box-shadow: 0 0 8px rgba(20, 184, 166, 0.55);
}

.ai-card-reason {
  position: relative;
  color: var(--text-secondary, #4b5563);
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.05), rgba(20, 184, 166, 0.04));
  border: 1px solid rgba(168, 85, 247, 0.12);
  padding: 10px 14px;
  border-radius: 10px;
  font-size: 13px;
  line-height: 1.65;
}

.ai-card-reason p {
  margin: 0;
}

.ai-card-reason::before {
  content: '“';
  position: absolute;
  left: 6px;
  top: -6px;
  font-family: serif;
  font-size: 28px;
  color: rgba(168, 85, 247, 0.3);
  line-height: 1;
}

.ai-card-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.ai-card-unavailable {
  align-self: center;
  color: var(--text-secondary, #6b7280);
  font-size: 12px;
}

.ai-card-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 10px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.18s ease, border-color 0.18s ease, transform 0.18s ease, box-shadow 0.18s ease;
}

.ai-card-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.ai-card-btn:focus-visible {
  outline: 2px solid #a855f7;
  outline-offset: 2px;
}

.ai-card-btn--ghost {
  background: transparent;
  color: var(--text-secondary, #4b5563);
  border-color: var(--border, rgba(0, 0, 0, 0.12));
}

.ai-card-btn--ghost:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
  border-color: rgba(0, 0, 0, 0.18);
}

.ai-card-btn--primary {
  background: linear-gradient(135deg, #a855f7 0%, #14b8a6 100%);
  color: #ffffff;
  box-shadow: 0 8px 22px -10px rgba(168, 85, 247, 0.55);
}

.ai-card-btn--primary:hover:not(:disabled) {
  background: linear-gradient(135deg, #9333ea 0%, #0d9488 100%);
  transform: translateY(-1px);
  box-shadow: 0 12px 26px -10px rgba(168, 85, 247, 0.7);
}

.ai-card-btn--primary:active:not(:disabled) {
  transform: translateY(0);
}

.ai-card-btn-icon {
  display: inline-flex;
}

.ai-card-btn-spinner {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-top-color: #ffffff;
  animation: ai-card-spin 0.75s linear infinite;
}

@keyframes ai-card-spin {
  to { transform: rotate(360deg); }
}

[data-theme="dark"] .ai-card {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .ai-card:hover {
  border-color: rgba(168, 85, 247, 0.45);
}

[data-theme="dark"] .ai-card-label {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-card-action {
  color: #aab2c0;
}

[data-theme="dark"] .ai-card-reason {
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.1), rgba(20, 184, 166, 0.08));
  border-color: rgba(168, 85, 247, 0.2);
  color: #cdd3df;
}

[data-theme="dark"] .ai-card-savings {
  background: rgba(20, 184, 166, 0.18);
  color: #5eead4;
}

[data-theme="dark"] .ai-card-btn--ghost {
  border-color: rgba(255, 255, 255, 0.14);
  color: #cdd3df;
}

[data-theme="dark"] .ai-card-btn--ghost:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.22);
}
</style>
