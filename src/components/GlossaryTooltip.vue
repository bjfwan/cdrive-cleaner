<script setup lang="ts">
import { computed } from 'vue';
import { lookupGlossary, lookupByTerm } from '../utils/glossary';

interface Props {
  termKey?: string;
  term?: string;
  showQuestionMark?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  termKey: '',
  term: '',
  showQuestionMark: true,
});

const entry = computed(() => {
  if (props.termKey) return lookupGlossary(props.termKey);
  if (props.term) return lookupByTerm(props.term);
  return null;
});

const displayTerm = computed(() => entry.value?.term ?? props.term ?? props.termKey);

const tooltipText = computed(() => {
  if (!entry.value) return '';
  const lines = [entry.value.plain];
  if (entry.value.detail) lines.push(entry.value.detail);
  return lines.join('\n');
});
</script>

<template>
  <span class="glossary-tooltip" :title="tooltipText" :data-has-entry="!!entry">
    <slot>{{ displayTerm }}</slot><span v-if="showQuestionMark && entry" class="glossary-hint" aria-hidden="true">?</span>
  </span>
</template>

<style scoped>
.glossary-tooltip {
  display: inline-flex;
  align-items: baseline;
  gap: 0.18rem;
  border-bottom: 1px dotted var(--color-border-medium, rgba(0, 0, 0, 0.25));
  cursor: help;
}

.glossary-tooltip[data-has-entry='false'] {
  border-bottom: none;
  cursor: inherit;
}

.glossary-hint {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 0.95rem;
  height: 0.95rem;
  margin-left: 0.18rem;
  border-radius: 50%;
  background: var(--color-border-light, rgba(0, 0, 0, 0.08));
  color: var(--color-text-tertiary, #778391);
  font-size: 0.66rem;
  font-weight: 700;
  line-height: 1;
}

.glossary-tooltip:hover .glossary-hint {
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.12));
  color: var(--color-highlight, #0f766e);
}
</style>
