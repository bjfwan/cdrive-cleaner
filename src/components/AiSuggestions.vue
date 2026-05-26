<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import AiSuggestionCard from './AiSuggestionCard.vue';
import { IconRefresh, IconSparkles } from './icons';
import { aiStore, type AiSuggestion } from '../store/ai';
import { useToast } from '../composables/useToast';
import FeatureIntro from './FeatureIntro.vue';

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

const totalSavingsMb = computed(() => suggestions.value.reduce((sum, s) => sum + (s.estimated_savings_mb || 0), 0));
const formattedSavings = computed(() => {
  const mb = totalSavingsMb.value;
  if (mb <= 0) return '—';
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb.toFixed(0)} MB`;
});
const modelLabel = computed(() => {
  if (state.settings.mode === 'byok') {
    return state.settings.byok.model || '自定义模型';
  }
  return state.settings.builtinModel || '内置模型';
});

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

    <div class="ai-hero">
      <svg class="ai-hero-bg" viewBox="0 0 600 240" preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient id="ai-grad-1" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="#a855f7" stop-opacity="0.55" />
            <stop offset="55%" stop-color="#7c3aed" stop-opacity="0.35" />
            <stop offset="100%" stop-color="#14b8a6" stop-opacity="0.45" />
          </linearGradient>
          <radialGradient id="ai-grad-2" cx="80%" cy="20%" r="60%">
            <stop offset="0%" stop-color="#ec4899" stop-opacity="0.5" />
            <stop offset="100%" stop-color="#ec4899" stop-opacity="0" />
          </radialGradient>
          <radialGradient id="ai-grad-3" cx="15%" cy="85%" r="50%">
            <stop offset="0%" stop-color="#22d3ee" stop-opacity="0.45" />
            <stop offset="100%" stop-color="#22d3ee" stop-opacity="0" />
          </radialGradient>
          <pattern id="ai-grid" width="32" height="32" patternUnits="userSpaceOnUse">
            <path d="M 32 0 L 0 0 0 32" fill="none" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          </pattern>
        </defs>
        <rect width="600" height="240" fill="url(#ai-grad-1)" />
        <rect width="600" height="240" fill="url(#ai-grad-2)" />
        <rect width="600" height="240" fill="url(#ai-grad-3)" />
        <rect width="600" height="240" fill="url(#ai-grid)" />
      </svg>

      <svg class="ai-hero-orbit" viewBox="0 0 200 200" aria-hidden="true">
        <defs>
          <linearGradient id="orbit-stroke" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="#ffffff" stop-opacity="0.7" />
            <stop offset="100%" stop-color="#ffffff" stop-opacity="0.1" />
          </linearGradient>
          <radialGradient id="core-grad" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stop-color="#ffffff" stop-opacity="0.95" />
            <stop offset="55%" stop-color="#c084fc" stop-opacity="0.9" />
            <stop offset="100%" stop-color="#7c3aed" stop-opacity="0.4" />
          </radialGradient>
        </defs>
        <g transform="translate(100 100)">
          <circle r="78" fill="none" stroke="url(#orbit-stroke)" stroke-width="1" stroke-dasharray="2 4" opacity="0.6">
            <animateTransform attributeName="transform" type="rotate" from="0" to="360" dur="38s" repeatCount="indefinite" />
          </circle>
          <circle r="58" fill="none" stroke="url(#orbit-stroke)" stroke-width="1" stroke-dasharray="1 3" opacity="0.5">
            <animateTransform attributeName="transform" type="rotate" from="360" to="0" dur="22s" repeatCount="indefinite" />
          </circle>
          <circle r="38" fill="none" stroke="url(#orbit-stroke)" stroke-width="1" opacity="0.3" />
          <circle r="28" fill="url(#core-grad)" opacity="0.95">
            <animate attributeName="r" values="26;30;26" dur="3.6s" repeatCount="indefinite" />
          </circle>
          <g>
            <circle cx="78" cy="0" r="3" fill="#ffffff" opacity="0.9">
              <animateTransform attributeName="transform" type="rotate" from="0" to="360" dur="14s" repeatCount="indefinite" />
            </circle>
          </g>
          <g>
            <circle cx="-58" cy="0" r="2.5" fill="#22d3ee" opacity="0.85">
              <animateTransform attributeName="transform" type="rotate" from="360" to="0" dur="9s" repeatCount="indefinite" />
            </circle>
          </g>
          <g>
            <circle cx="0" cy="38" r="2" fill="#f0abfc" opacity="0.85">
              <animateTransform attributeName="transform" type="rotate" from="0" to="360" dur="6.5s" repeatCount="indefinite" />
            </circle>
          </g>
        </g>
      </svg>

      <div class="ai-hero-copy">
        <div class="ai-hero-eyebrow">
          <span class="ai-hero-dot"></span>
          <span>AI 建议 · {{ modelLabel }}</span>
        </div>
        <h2 class="ai-hero-title">让 AI 帮你看一眼盘里情况</h2>
        <p class="ai-hero-desc">
          把你勾选的数据类别（脱敏后）发给 AI，得到一份清理 / 迁移顺序建议。
          <strong>每一条都要手动点「采纳」才会执行</strong>，不会自动动你的盘。
        </p>

        <div class="ai-hero-stats">
          <div class="ai-hero-stat">
            <span class="ai-hero-stat-value">{{ suggestions.length }}</span>
            <span class="ai-hero-stat-label">待看建议</span>
          </div>
          <div class="ai-hero-stat">
            <span class="ai-hero-stat-value">{{ formattedSavings }}</span>
            <span class="ai-hero-stat-label">预计节省</span>
          </div>
          <div class="ai-hero-stat">
            <span class="ai-hero-stat-value">{{ state.settings.mode === 'byok' ? '自定义' : '内置' }}</span>
            <span class="ai-hero-stat-label">运行模式</span>
          </div>
        </div>

        <div class="ai-hero-actions">
          <button
            class="ai-cta ai-cta--primary"
            :disabled="state.loadingSuggestions"
            @click="refresh"
          >
            <span v-if="!state.loadingSuggestions" class="ai-cta-icon"><IconSparkles :size="16" /></span>
            <span v-else class="ai-cta-spinner" aria-hidden="true"></span>
            <span>{{ state.loadingSuggestions ? 'AI 正在分析…' : '让 AI 看一眼盘里情况' }}</span>
          </button>
          <button class="ai-cta ai-cta--ghost" @click="emit('go-settings')">
            <span>AI 设置</span>
          </button>
          <button class="ai-cta ai-cta--ghost" @click="emit('go-privacy')">
            <span>隐私政策</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="!enabled" class="ai-empty ai-empty--cta">
      <svg class="ai-empty-art" viewBox="0 0 320 200" aria-hidden="true">
        <defs>
          <linearGradient id="ae-grad" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="#a855f7" />
            <stop offset="100%" stop-color="#14b8a6" />
          </linearGradient>
        </defs>
        <g transform="translate(160 100)">
          <circle r="68" fill="url(#ae-grad)" opacity="0.12" />
          <circle r="42" fill="url(#ae-grad)" opacity="0.2" />
          <path d="M0 -32 L9 -10 L32 -2 L9 6 L0 28 L-9 6 L-32 -2 L-9 -10 Z" fill="url(#ae-grad)" opacity="0.85" />
          <circle cx="32" cy="20" r="6" fill="#a855f7" opacity="0.8" />
          <circle cx="-28" cy="26" r="4" fill="#14b8a6" opacity="0.75" />
          <circle cx="22" cy="-30" r="3" fill="#f472b6" opacity="0.7" />
        </g>
      </svg>
      <h3>AI 服务尚未启用</h3>
      <p>到「设置 → AI 服务」打开开关，再勾选至少一个数据类别。所有发送数据会先经过本地脱敏。</p>
      <button class="ai-cta ai-cta--primary" @click="emit('go-settings')">
        <span class="ai-cta-icon"><IconSparkles :size="16" /></span>
        <span>去开启 AI</span>
      </button>
    </div>

    <div v-else-if="suggestions.length === 0" class="ai-empty">
      <svg class="ai-empty-art" viewBox="0 0 320 200" aria-hidden="true">
        <defs>
          <linearGradient id="ae-idle-grad" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stop-color="#a855f7" stop-opacity="0.45" />
            <stop offset="100%" stop-color="#14b8a6" stop-opacity="0.55" />
          </linearGradient>
        </defs>
        <g transform="translate(160 100)">
          <circle r="60" fill="none" stroke="url(#ae-idle-grad)" stroke-width="1" stroke-dasharray="3 6" opacity="0.55">
            <animateTransform attributeName="transform" type="rotate" from="0" to="360" dur="20s" repeatCount="indefinite" />
          </circle>
          <circle r="40" fill="none" stroke="url(#ae-idle-grad)" stroke-width="1" stroke-dasharray="2 4" opacity="0.5">
            <animateTransform attributeName="transform" type="rotate" from="360" to="0" dur="12s" repeatCount="indefinite" />
          </circle>
          <circle r="20" fill="url(#ae-idle-grad)" opacity="0.6">
            <animate attributeName="opacity" values="0.4;0.8;0.4" dur="2.5s" repeatCount="indefinite" />
          </circle>
        </g>
      </svg>
      <h3>暂无 AI 建议</h3>
      <p>点击上方「让 AI 看一眼盘里情况」开始第一次分析；或等待下一次定时扫描完成。</p>
    </div>

    <div v-else class="ai-list">
      <header class="ai-list-head">
        <h3>{{ suggestions.length }} 条建议待看</h3>
        <button class="ai-cta ai-cta--ghost ai-cta--sm" :disabled="state.loadingSuggestions" @click="refresh">
          <IconRefresh :size="14" :spinning="state.loadingSuggestions" />
          <span>{{ state.loadingSuggestions ? '分析中…' : '再来一次' }}</span>
        </button>
      </header>
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

.ai-hero {
  position: relative;
  border-radius: 22px;
  overflow: hidden;
  border: 1px solid rgba(168, 85, 247, 0.25);
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.08) 0%, rgba(20, 184, 166, 0.06) 100%), var(--surface, #ffffff);
  box-shadow: 0 18px 42px -28px rgba(168, 85, 247, 0.45);
  isolation: isolate;
}

.ai-hero-bg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  opacity: 0.85;
  pointer-events: none;
}

.ai-hero-orbit {
  position: absolute;
  right: -36px;
  top: 50%;
  transform: translateY(-50%);
  width: 280px;
  height: 280px;
  z-index: 1;
  pointer-events: none;
  filter: drop-shadow(0 12px 36px rgba(168, 85, 247, 0.4));
}

.ai-hero-copy {
  position: relative;
  z-index: 2;
  padding: 28px 32px;
  max-width: 60ch;
  color: #ffffff;
}

.ai-hero-eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 5px 12px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: #ffffff;
  margin-bottom: 14px;
}

.ai-hero-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #d8b4fe;
  box-shadow: 0 0 10px rgba(168, 85, 247, 0.85);
  animation: ai-dot-pulse 2.5s ease-in-out infinite;
}

@keyframes ai-dot-pulse {
  0%, 100% { opacity: 0.7; transform: scale(0.9); }
  50% { opacity: 1; transform: scale(1.1); }
}

.ai-hero-title {
  margin: 0 0 8px;
  font-size: 26px;
  font-weight: 800;
  letter-spacing: -0.01em;
  color: #ffffff;
  text-shadow: 0 6px 24px rgba(15, 23, 42, 0.45);
}

.ai-hero-desc {
  margin: 0 0 18px;
  font-size: 13.5px;
  line-height: 1.65;
  color: rgba(255, 255, 255, 0.88);
  text-shadow: 0 2px 12px rgba(15, 23, 42, 0.35);
  max-width: 56ch;
}

.ai-hero-desc strong {
  color: #ffffff;
  font-weight: 700;
}

.ai-hero-stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, max-content));
  gap: 24px;
  margin-bottom: 20px;
}

.ai-hero-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.ai-hero-stat-value {
  font-size: 22px;
  font-weight: 800;
  color: #ffffff;
  line-height: 1.15;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
  text-shadow: 0 2px 12px rgba(15, 23, 42, 0.4);
}

.ai-hero-stat-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.7);
}

.ai-hero-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.ai-cta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 9px 16px;
  border-radius: 10px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: transform 0.18s ease, background 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
  background: rgba(255, 255, 255, 0.14);
  color: #ffffff;
  border-color: rgba(255, 255, 255, 0.2);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
}

.ai-cta:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.22);
  transform: translateY(-1px);
}

.ai-cta:active:not(:disabled) {
  transform: translateY(0);
}

.ai-cta:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.ai-cta:focus-visible {
  outline: 2px solid #ffffff;
  outline-offset: 2px;
}

.ai-cta--primary {
  background: linear-gradient(135deg, #ffffff 0%, #f5d0fe 100%);
  color: #6b21a8;
  border-color: rgba(255, 255, 255, 0.6);
  box-shadow: 0 10px 26px -10px rgba(255, 255, 255, 0.55);
}

.ai-cta--primary:hover:not(:disabled) {
  background: linear-gradient(135deg, #ffffff 0%, #f0abfc 100%);
  box-shadow: 0 14px 30px -10px rgba(255, 255, 255, 0.7);
}

.ai-cta--ghost {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.2);
}

.ai-cta--sm {
  padding: 6px 12px;
  font-size: 12px;
  background: var(--surface, #ffffff);
  color: var(--text-primary, #111827);
  border-color: var(--border, rgba(0, 0, 0, 0.12));
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
}

[data-theme="dark"] .ai-cta--sm {
  background: rgba(255, 255, 255, 0.05);
  color: #f5f5f7;
  border-color: rgba(255, 255, 255, 0.14);
}

.ai-cta-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.ai-cta-spinner {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid rgba(107, 33, 168, 0.25);
  border-top-color: #6b21a8;
  animation: ai-spin 0.8s linear infinite;
}

@keyframes ai-spin {
  to { transform: rotate(360deg); }
}

.ai-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.ai-list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 2px;
}

.ai-list-head h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary, #111827);
  letter-spacing: 0.01em;
}

.ai-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 32px 24px 36px;
  border: 1px dashed var(--border, rgba(0, 0, 0, 0.12));
  border-radius: 20px;
  text-align: center;
  color: var(--text-secondary, #6b7280);
  background:
    radial-gradient(circle at 50% 0%, rgba(168, 85, 247, 0.08), transparent 60%),
    var(--surface, #ffffff);
}

.ai-empty--cta {
  border-style: solid;
  border-color: rgba(168, 85, 247, 0.28);
  background:
    radial-gradient(circle at 50% 0%, rgba(168, 85, 247, 0.16), transparent 65%),
    var(--surface, #ffffff);
}

.ai-empty-art {
  width: 200px;
  height: auto;
  margin-bottom: 4px;
}

.ai-empty h3 {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  color: var(--text-primary, #111827);
}

.ai-empty p {
  margin: 0;
  max-width: 48ch;
  line-height: 1.65;
  font-size: 13px;
}

.ai-empty .ai-cta--primary {
  margin-top: 6px;
  background: linear-gradient(135deg, #a855f7 0%, #14b8a6 100%);
  color: #ffffff;
  border-color: transparent;
  box-shadow: 0 14px 32px -14px rgba(168, 85, 247, 0.55);
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
}

.ai-empty .ai-cta--primary:hover:not(:disabled) {
  background: linear-gradient(135deg, #9333ea 0%, #0d9488 100%);
  box-shadow: 0 18px 36px -14px rgba(168, 85, 247, 0.7);
}

.ai-empty .ai-cta-spinner {
  border-color: rgba(255, 255, 255, 0.35);
  border-top-color: #ffffff;
}

[data-theme="dark"] .ai-hero {
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.14) 0%, rgba(20, 184, 166, 0.1) 100%), rgba(15, 23, 42, 0.5);
  border-color: rgba(168, 85, 247, 0.32);
}

[data-theme="dark"] .ai-empty {
  background:
    radial-gradient(circle at 50% 0%, rgba(168, 85, 247, 0.14), transparent 60%),
    rgba(255, 255, 255, 0.03);
  border-color: rgba(255, 255, 255, 0.14);
  color: #b3bac8;
}

[data-theme="dark"] .ai-empty h3 {
  color: #f5f5f7;
}

[data-theme="dark"] .ai-list-head h3 {
  color: #f5f5f7;
}

@media (max-width: 720px) {
  .ai-hero-orbit { display: none; }
  .ai-hero-copy { padding: 22px 22px 24px; }
  .ai-hero-title { font-size: 22px; }
  .ai-hero-stats { gap: 18px; }
}
</style>
