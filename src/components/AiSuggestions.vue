<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import AiSuggestionCard from './AiSuggestionCard.vue';
import { IconRefresh, IconScanEmpty, IconInfo } from './icons';
import { aiStore, type AiSuggestion } from '../store/ai';
import { useToast } from '../composables/useToast';
import FeatureIntro from './FeatureIntro.vue';
import { stateCopy } from '../utils/state-copy';

const emit = defineEmits<{
  'open-migrate': [item: { path: string; name: string; size: number; file_count: number }];
  'go-settings': [];
  'go-privacy': [];
}>();

const showToast = useToast();
const applyingId = ref<string | null>(null);

const state = aiStore.state;

const enabled = computed(() => state.settings.enabled);
const suggestions = computed<AiSuggestion[]>(() => [...state.suggestions]);
const anyCategoryOn = computed(() =>
  state.settings.categories.disks
  || state.settings.categories.largeFiles
  || state.settings.categories.categories
  || state.settings.categories.duplicates,
);

onMounted(async () => {
  await aiStore.installSuggestionListener();
});

async function refresh() {
  if (!enabled.value) {
    showToast('AI 分析未开启', '请先在设置中启用 AI 服务', 'warning');
    return;
  }
  if (!anyCategoryOn.value) {
    showToast('未勾选任何数据类别', '请到设置中至少勾选一个发送类别', 'warning');
    return;
  }
  await aiStore.requestAnalyze();
  showToast('已请求 AI 分析', '建议将在分析完成后自动出现', 'info');
}

async function onAccept(suggestion: AiSuggestion) {
  if (applyingId.value) return;
  applyingId.value = suggestion.id;
  try {
    const path = await aiStore.resolveSuggestionPath(suggestion.id);
    if (!path) {
      showToast('无法定位路径', '建议可能已过期，请重新触发 AI 分析', 'warning');
      return;
    }
    const lastSep = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
    const name = lastSep >= 0 ? path.slice(lastSep + 1) : path;
    // size / file_count are unknown at this stage; MigrateDialog will refresh
    // them via safety analysis when it opens. Passing 0 mirrors what the
    // existing single-item migrate path tolerates.
    emit('open-migrate', {
      path,
      name: name || suggestion.target_label,
      size: 0,
      file_count: 0,
    });
    aiStore.dismiss(suggestion.id);
  } finally {
    applyingId.value = null;
  }
}

function onDismiss(suggestion: AiSuggestion) {
  aiStore.dismiss(suggestion.id);
}
</script>

<template>
  <section class="ai-panel">
    <FeatureIntro
      storage-key="ai-suggestions"
      what="把你勾选的数据类别（脱敏后）发给 AI，让模型给出清理 / 迁移建议。"
      when="自己想不清楚该从哪下手、或者扫描结果太多想要个推荐顺序时。"
      outcome="每条建议都要你手动点「采纳」才会真做；不采纳就只是一段文字，不会动你的盘。"
      reversibility="reversible"
      reversibility-note="采纳后走正常迁移/清理流程，仍可在「迁移历史」回滚"
    />
    <header class="ai-panel-head">
      <div class="ai-panel-title">
        <h2>AI 建议</h2>
        <p>
          AI 会根据你勾选的数据类别给出磁盘清理 / 迁移建议。
          <strong>每一条都需要你手动采纳，不会自动执行任何操作。</strong>
        </p>
      </div>
      <div class="ai-panel-actions">
        <button class="ai-toolbar-btn" @click="emit('go-settings')">
          <span>打开 AI 设置</span>
        </button>
        <button class="ai-toolbar-btn" @click="emit('go-privacy')">
          <span>查看隐私政策</span>
        </button>
        <button class="ai-toolbar-btn ai-toolbar-btn--primary" :disabled="state.loadingSuggestions" @click="refresh">
          <IconRefresh :size="14" />
          <span>{{ state.loadingSuggestions ? '分析中…' : '让 AI 看一眼盘里情况' }}</span>
        </button>
      </div>
    </header>

    <div v-if="!enabled" class="ai-empty">
      <IconInfo :size="40" />
      <h3>{{ stateCopy.aiSuggestions.empty.title }}</h3>
      <p>请先到「设置 → AI 服务」启用 AI 分析，并勾选要发送的数据类别。</p>
      <button class="ai-toolbar-btn ai-toolbar-btn--primary" @click="emit('go-settings')">去开启 AI</button>
    </div>

    <div v-else-if="suggestions.length === 0" class="ai-empty">
      <IconScanEmpty :size="56" />
      <h3>暂无 AI 建议</h3>
      <p>等待下一次扫描完成，或点击右上角让 AI 立刻看一眼盘里情况。</p>
    </div>

    <div v-else class="ai-list">
      <AiSuggestionCard
        v-for="suggestion in suggestions"
        :key="suggestion.id"
        :suggestion="suggestion"
        :applying="applyingId === suggestion.id"
        @accept="onAccept"
        @dismiss="onDismiss"
      />
    </div>
  </section>
</template>

<style scoped>
.ai-panel {
  display: flex;
  flex-direction: column;
  gap: 18px;
  padding: 4px 0 32px;
}

.ai-panel-head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
}

.ai-panel-title h2 {
  margin: 0 0 4px;
  font-size: 22px;
  color: var(--text-primary, #111827);
}

.ai-panel-title p {
  margin: 0;
  color: var(--text-secondary, #6b7280);
  font-size: 13px;
  line-height: 1.6;
  max-width: 60ch;
}

.ai-panel-actions {
  display: inline-flex;
  gap: 8px;
}

.ai-toolbar-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border-radius: 8px;
  border: 1px solid var(--border, rgba(0, 0, 0, 0.12));
  background: transparent;
  font-size: 13px;
  color: var(--text-primary, #111827);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.ai-toolbar-btn:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
}

.ai-toolbar-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ai-toolbar-btn--primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  border-color: var(--color-highlight);
}

.ai-toolbar-btn--primary:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-highlight) 88%, #000 12%);
  border-color: color-mix(in srgb, var(--color-highlight) 88%, #000 12%);
}

.ai-list {
  display: grid;
  gap: 12px;
}

.ai-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 56px 24px;
  border: 1px dashed var(--border, rgba(0, 0, 0, 0.12));
  border-radius: 18px;
  text-align: center;
  color: var(--text-secondary, #6b7280);
}

.ai-empty h3 {
  margin: 4px 0 0;
  color: var(--text-primary, #111827);
}

.ai-empty p {
  margin: 0;
  max-width: 46ch;
  line-height: 1.6;
}

[data-theme="dark"] .ai-panel-title h2,
[data-theme="dark"] .ai-empty h3 {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-panel-title p,
[data-theme="dark"] .ai-empty {
  color: #aab2c0;
}

[data-theme="dark"] .ai-toolbar-btn {
  color: #f5f5f7;
  border-color: rgba(255, 255, 255, 0.14);
}

[data-theme="dark"] .ai-toolbar-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .ai-empty {
  border-color: rgba(255, 255, 255, 0.14);
}
</style>
