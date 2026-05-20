<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import type { DiskInfo } from '../types';
import { formatBytes } from '../utils/format';

type SelectSize = 'sm' | 'md';
type SelectPlacement = 'bottom' | 'top';

interface Props {
  id?: string;
  modelValue: string;
  disks: DiskInfo[];
  placeholder?: string;
  emptyLabel?: string;
  includeEmpty?: boolean;
  disabled?: boolean;
  size?: SelectSize;
  placement?: SelectPlacement;
}

const props = withDefaults(defineProps<Props>(), {
  placeholder: '选择磁盘',
  emptyLabel: '不预设',
  includeEmpty: false,
  disabled: false,
  size: 'md',
  placement: 'bottom',
});

const emit = defineEmits<{
  'update:modelValue': [value: string];
}>();

const rootEl = ref<HTMLElement | null>(null);
const open = ref(false);

const selectedDisk = computed(() =>
  props.disks.find((disk) => `${disk.drive_letter}\\` === props.modelValue) ?? null,
);

const selectedTitle = computed(() => {
  if (selectedDisk.value) {
    return `${selectedDisk.value.drive_letter} · ${selectedDisk.value.label || 'Local Disk'}`;
  }
  if (props.includeEmpty && props.modelValue === '') {
    return props.emptyLabel;
  }
  return props.placeholder;
});

const selectedMeta = computed(() => {
  if (!selectedDisk.value) return '';
  return `${formatBytes(selectedDisk.value.free_space)} 可用 · ${selectedDisk.value.usage_percent.toFixed(0)}% 已用`;
});

function valueFor(disk: DiskInfo) {
  return `${disk.drive_letter}\\`;
}

function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
}

function choose(value: string) {
  if (props.disabled) return;
  emit('update:modelValue', value);
  open.value = false;
}

function closeFromOutside(event: PointerEvent) {
  if (!rootEl.value || rootEl.value.contains(event.target as Node)) return;
  open.value = false;
}

function closeFromKeyboard(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    open.value = false;
  }
}

onMounted(() => {
  document.addEventListener('pointerdown', closeFromOutside);
  document.addEventListener('keydown', closeFromKeyboard);
});

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', closeFromOutside);
  document.removeEventListener('keydown', closeFromKeyboard);
});
</script>

<template>
  <div ref="rootEl" class="disk-select" :class="[`disk-select--${size}`, { open, disabled }]" :data-placement="placement">
    <button
      :id="id"
      type="button"
      class="disk-select-button"
      :disabled="disabled"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="toggle"
    >
      <span class="disk-select-drive">{{ selectedDisk?.drive_letter ?? '盘' }}</span>
      <span class="disk-select-copy">
        <span class="disk-select-title">{{ selectedTitle }}</span>
        <span v-if="selectedMeta" class="disk-select-meta">{{ selectedMeta }}</span>
      </span>
      <span class="disk-select-chevron" :class="{ open }">⌄</span>
    </button>

    <Transition name="disk-select-pop">
      <div v-if="open" class="disk-select-menu" role="listbox">
        <button
          v-if="includeEmpty"
          type="button"
          class="disk-select-option"
          :class="{ active: modelValue === '' }"
          role="option"
          :aria-selected="modelValue === ''"
          @click="choose('')"
        >
          <span class="disk-option-drive">—</span>
          <span class="disk-option-main">
            <span class="disk-option-name">{{ emptyLabel }}</span>
            <span class="disk-option-meta">需要迁移时再选择目标盘</span>
          </span>
        </button>

        <button
          v-for="disk in disks"
          :key="disk.drive_letter"
          type="button"
          class="disk-select-option"
          :class="{ active: modelValue === valueFor(disk) }"
          role="option"
          :aria-selected="modelValue === valueFor(disk)"
          @click="choose(valueFor(disk))"
        >
          <span class="disk-option-drive">{{ disk.drive_letter }}</span>
          <span class="disk-option-main">
            <span class="disk-option-name">{{ disk.label || 'Local Disk' }}</span>
            <span class="disk-option-meta">{{ disk.file_system }} · {{ formatBytes(disk.free_space) }} 可用</span>
          </span>
          <span class="disk-option-usage">{{ disk.usage_percent.toFixed(0) }}%</span>
        </button>

        <div v-if="!includeEmpty && disks.length === 0" class="disk-select-empty">没有可用磁盘</div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.disk-select {
  position: relative;
  width: 100%;
  min-width: 0;
}

.disk-select-button {
  width: 100%;
  min-height: 3.1rem;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.75rem;
  padding: 0.6rem 0.8rem;
  border: 1px solid var(--color-border-medium);
  border-radius: var(--radius-sm);
  background: linear-gradient(180deg, var(--color-surface-strong), var(--color-surface));
  color: var(--color-text-primary);
  box-shadow: var(--shadow-xs);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast), background var(--transition-fast), transform var(--transition-fast);
}

.disk-select--sm .disk-select-button {
  min-height: 2.45rem;
  gap: 0.55rem;
  padding: 0.42rem 0.65rem;
  border-radius: 12px;
}

.disk-select-button:hover:not(:disabled),
.disk-select.open .disk-select-button {
  border-color: var(--color-highlight-soft);
  background: var(--color-surface-hover);
  box-shadow: var(--shadow-sm);
}

.disk-select-button:disabled {
  cursor: not-allowed;
  opacity: 0.62;
}

.disk-select-drive,
.disk-option-drive {
  width: 2.2rem;
  height: 2.2rem;
  border-radius: 0.75rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
  font-size: 0.82rem;
  font-weight: 800;
  font-feature-settings: 'tnum';
  flex-shrink: 0;
}

.disk-select--sm .disk-select-drive {
  width: 1.75rem;
  height: 1.75rem;
  border-radius: 0.58rem;
  font-size: 0.74rem;
}

.disk-select-copy,
.disk-option-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.12rem;
}

.disk-select-title,
.disk-option-name {
  overflow: hidden;
  color: var(--color-text-primary);
  font-size: 0.9rem;
  font-weight: 700;
  line-height: 1.2;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.disk-select--sm .disk-select-title {
  font-size: 0.82rem;
}

.disk-select-meta,
.disk-option-meta {
  overflow: hidden;
  color: var(--color-text-tertiary);
  font-size: 0.74rem;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.disk-select--sm .disk-select-meta {
  display: none;
}

.disk-select-chevron {
  color: var(--color-text-tertiary);
  font-size: 1rem;
  line-height: 1;
  transition: transform var(--transition-fast), color var(--transition-fast);
}

.disk-select-chevron.open {
  color: var(--color-highlight);
  transform: rotate(180deg);
}

.disk-select-menu {
  position: absolute;
  inset-inline: 0;
  top: calc(100% + 0.45rem);
  z-index: 3200;
  max-height: 18rem;
  overflow-y: auto;
  padding: 0.35rem;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  background: var(--color-surface-strong);
  box-shadow: var(--shadow-lg);
}

.disk-select[data-placement='top'] .disk-select-menu {
  top: auto;
  bottom: calc(100% + 0.45rem);
}

@supports (backdrop-filter: blur(12px)) or (-webkit-backdrop-filter: blur(12px)) {
  .disk-select-menu {
    background: var(--color-surface);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
  }
}

.disk-select-option {
  width: 100%;
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.65rem;
  padding: 0.62rem;
  border: none;
  border-radius: 12px;
  background: transparent;
  color: var(--color-text-primary);
  cursor: pointer;
  text-align: left;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.disk-select-option:hover,
.disk-select-option.active {
  background: var(--color-highlight-soft);
}

.disk-select-option.active .disk-option-drive {
  background: var(--color-highlight);
  color: #ffffff;
}

.disk-option-usage {
  padding: 0.2rem 0.42rem;
  border-radius: 999px;
  background: var(--color-border-light);
  color: var(--color-text-secondary);
  font-size: 0.68rem;
  font-weight: 800;
  font-feature-settings: 'tnum';
}

.disk-select-empty {
  padding: 0.85rem;
  color: var(--color-text-tertiary);
  font-size: 0.82rem;
  text-align: center;
}

.disk-select-pop-enter-active,
.disk-select-pop-leave-active {
  transition: opacity var(--transition-fast), transform var(--transition-fast);
}

.disk-select-pop-enter-from,
.disk-select-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}

.disk-select[data-placement='top'] .disk-select-pop-enter-from,
.disk-select[data-placement='top'] .disk-select-pop-leave-to {
  transform: translateY(4px) scale(0.98);
}

[data-theme='dark'] .disk-select-menu {
  background: var(--color-surface-strong);
  border-color: var(--color-border-medium);
}
</style>
