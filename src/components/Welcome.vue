<script setup lang="ts">
import appIcon from '../assets/app-icon.svg';

const emit = defineEmits<{ close: [] }>();

function handleStart() {
  localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
  emit('close');
}
</script>

<template>
  <div class="welcome-overlay">
    <div class="welcome-card">
      <img :src="appIcon" alt="CSD" class="welcome-logo" width="80" height="80" />
      <h1 class="welcome-title">为 C 盘瘦身</h1>
      <p class="welcome-sub">扫描 → 找出能搬走的 → 一键搬到其它盘</p>
      <p class="welcome-hint">迁移走的目录会保留链接占位，应用照常运行；任何一步都能在"迁移历史"里回滚。</p>
      <button class="welcome-btn" @click="handleStart">开始</button>
    </div>
  </div>
</template>

<style scoped>
.welcome-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(15, 23, 32, 0.42);
  z-index: 2000;
  animation: fadeIn 0.32s ease;
}

.welcome-card {
  width: min(420px, 92vw);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.55rem;
  padding: 2.4rem 2rem 2.2rem;
  border-radius: 24px;
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border-light);
  box-shadow: 0 32px 80px rgba(15, 23, 32, 0.28);
  text-align: center;
  animation: rise 0.36s cubic-bezier(0.16, 1, 0.3, 1);
}

.welcome-logo {
  margin-bottom: 0.5rem;
  filter: drop-shadow(0 12px 24px rgba(15, 23, 32, 0.18));
}

.welcome-title {
  font-family: var(--font-display);
  font-size: 1.85rem;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--color-text-primary);
}

.welcome-sub {
  font-size: 1rem;
  color: var(--color-text-secondary);
  margin-top: 0.2rem;
}

.welcome-hint {
  font-size: 0.82rem;
  color: var(--color-text-tertiary);
  margin-top: 0.35rem;
  line-height: 1.6;
  max-width: 22rem;
}

.welcome-btn {
  margin-top: 1.4rem;
  padding: 0.78rem 1.6rem;
  border: none;
  border-radius: 14px;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-size: 1rem;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 14px 28px rgba(15, 23, 32, 0.24);
  transition: transform var(--transition-base), box-shadow var(--transition-base);
}

.welcome-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 20px 36px rgba(15, 23, 32, 0.32);
}

@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }
@keyframes rise {
  from { opacity: 0; transform: translateY(12px) scale(0.96); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>
