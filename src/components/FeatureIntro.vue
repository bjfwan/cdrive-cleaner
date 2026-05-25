<script setup lang="ts">
import { computed, ref, onMounted } from 'vue';

interface Props {
  storageKey: string;
  what: string;
  when: string;
  outcome: string;
  reversibility?: 'reversible' | 'partial' | 'irreversible';
  reversibilityNote?: string;
  defaultCollapsed?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  reversibility: 'reversible',
  reversibilityNote: '',
  defaultCollapsed: false,
});

const STORAGE_PREFIX = 'cdrive-cleaner-feature-intro:';

function readVisitState(): boolean {
  try {
    return localStorage.getItem(STORAGE_PREFIX + props.storageKey) === 'seen';
  } catch {
    return false;
  }
}

function persistVisitState() {
  try {
    localStorage.setItem(STORAGE_PREFIX + props.storageKey, 'seen');
  } catch {
    // localStorage unavailable; non-fatal — keeps the panel open this session
  }
}

const collapsed = ref(false);
const visited = ref(false);

onMounted(() => {
  visited.value = readVisitState();
  collapsed.value = visited.value || props.defaultCollapsed;
  if (!visited.value) {
    persistVisitState();
  }
});

function toggle() {
  collapsed.value = !collapsed.value;
}

const reversibilityLabel = computed(() => {
  if (props.reversibility === 'irreversible') return '不可撤销';
  if (props.reversibility === 'partial') return '部分可撤销';
  return '可撤销';
});

const reversibilityTone = computed(() => {
  if (props.reversibility === 'irreversible') return 'danger';
  if (props.reversibility === 'partial') return 'warning';
  return 'safe';
});
</script>

<template>
  <section class="feature-intro" :class="{ 'feature-intro--collapsed': collapsed }">
    <button class="feature-intro-toggle" @click="toggle" type="button" :aria-expanded="!collapsed">
      <span class="feature-intro-chevron" :class="{ open: !collapsed }">▾</span>
      <span class="feature-intro-toggle-label">{{ collapsed ? '为什么用这一栏' : '收起说明' }}</span>
    </button>

    <div v-if="!collapsed" class="feature-intro-body">
      <div class="feature-intro-row">
        <span class="feature-intro-tag feature-intro-tag--what">这是什么</span>
        <p class="feature-intro-text">{{ what }}</p>
      </div>
      <div class="feature-intro-row">
        <span class="feature-intro-tag feature-intro-tag--when">什么时候用</span>
        <p class="feature-intro-text">{{ when }}</p>
      </div>
      <div class="feature-intro-row">
        <span class="feature-intro-tag feature-intro-tag--outcome">会怎么样</span>
        <p class="feature-intro-text">
          {{ outcome }}
          <span class="feature-intro-badge" :data-tone="reversibilityTone">{{ reversibilityLabel }}</span>
          <span v-if="reversibilityNote" class="feature-intro-note">{{ reversibilityNote }}</span>
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.feature-intro {
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  padding: 0.6rem 0.9rem;
  margin-bottom: 0.85rem;
  font-family: var(--font-sans, system-ui);
}

.feature-intro--collapsed {
  padding: 0.45rem 0.9rem;
  background: transparent;
}

.feature-intro-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  background: none;
  border: none;
  padding: 0.15rem 0.2rem;
  cursor: pointer;
  color: var(--color-text-tertiary);
  font-size: 0.78rem;
  font-weight: 600;
  transition: color 0.15s;
}

.feature-intro-toggle:hover {
  color: var(--color-text-primary);
}

.feature-intro-chevron {
  display: inline-block;
  transition: transform 0.18s ease;
  font-size: 0.78rem;
}

.feature-intro-chevron.open {
  transform: rotate(0deg);
}

.feature-intro-chevron:not(.open) {
  transform: rotate(-90deg);
}

.feature-intro-body {
  margin-top: 0.45rem;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.feature-intro-row {
  display: grid;
  grid-template-columns: 5.4rem 1fr;
  gap: 0.7rem;
  align-items: baseline;
}

.feature-intro-tag {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.04em;
  padding: 0.18rem 0.45rem;
  border-radius: var(--radius-pill, 999px);
  text-align: center;
  white-space: nowrap;
}

.feature-intro-tag--what {
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight, #0f766e);
}

.feature-intro-tag--when {
  background: rgba(37, 99, 235, 0.1);
  color: var(--color-info, #2563eb);
}

.feature-intro-tag--outcome {
  background: rgba(245, 158, 11, 0.1);
  color: var(--color-warning, #b45309);
}

.feature-intro-text {
  font-size: 0.84rem;
  line-height: 1.55;
  color: var(--color-text-secondary, #43505c);
  margin: 0;
}

.feature-intro-badge {
  display: inline-block;
  margin-left: 0.4rem;
  padding: 0.1rem 0.42rem;
  border-radius: var(--radius-pill, 999px);
  font-size: 0.7rem;
  font-weight: 700;
  vertical-align: 1px;
}

.feature-intro-badge[data-tone='safe'] {
  background: rgba(15, 159, 110, 0.1);
  color: #0d8a5f;
}

.feature-intro-badge[data-tone='warning'] {
  background: rgba(245, 158, 11, 0.14);
  color: #b45309;
}

.feature-intro-badge[data-tone='danger'] {
  background: rgba(220, 38, 38, 0.12);
  color: #b91c1c;
}

.feature-intro-note {
  display: block;
  margin-top: 0.18rem;
  font-size: 0.74rem;
  color: var(--color-text-tertiary, #778391);
}

[data-theme='dark'] .feature-intro {
  background: rgba(255, 255, 255, 0.03);
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme='dark'] .feature-intro-badge[data-tone='safe'] {
  background: rgba(52, 211, 153, 0.14);
  color: #34d399;
}

[data-theme='dark'] .feature-intro-badge[data-tone='warning'] {
  background: rgba(251, 191, 36, 0.14);
  color: #fbbf24;
}

[data-theme='dark'] .feature-intro-badge[data-tone='danger'] {
  background: rgba(248, 113, 113, 0.14);
  color: #fca5a5;
}

@media (max-width: 640px) {
  .feature-intro-row {
    grid-template-columns: 1fr;
    gap: 0.15rem;
  }
  .feature-intro-tag {
    justify-self: flex-start;
  }
}
</style>
