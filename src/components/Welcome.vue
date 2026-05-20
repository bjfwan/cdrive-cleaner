<script setup lang="ts">
import appIcon from '../assets/app-icon.png';
import { IconScan, IconTruck, IconRollback, IconClose } from './icons';

const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <div class="welcome-overlay" @click.self="emit('close')">
    <div class="welcome-card" role="dialog" aria-modal="true" aria-label="关于 CDrive Cleaner">
      <button class="welcome-close" @click="emit('close')" aria-label="关闭"><IconClose :size="16" /></button>

      <img :src="appIcon" alt="" class="welcome-logo" />
      <h2 class="welcome-title">CDrive Cleaner</h2>
      <p class="welcome-version">v0.1.6</p>

      <ul class="welcome-features">
        <li>
          <span class="feature-icon"><IconScan :size="18" /></span>
          <div>
            <strong>智能扫描</strong>
            <small>找出能搬走和能删掉的</small>
          </div>
        </li>
        <li>
          <span class="feature-icon"><IconTruck :size="18" /></span>
          <div>
            <strong>搬到其他盘还能用</strong>
            <small>用 Junction 链接，应用照常运行</small>
          </div>
        </li>
        <li>
          <span class="feature-icon"><IconRollback :size="18" /></span>
          <div>
            <strong>每一步都可回滚</strong>
            <small>历史里随时撤销</small>
          </div>
        </li>
      </ul>

      <button class="welcome-btn" @click="emit('close')">关闭</button>
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
  background: var(--modal-backdrop, rgba(15, 23, 32, 0.4));
  z-index: 2000;
  animation: welcome-fade 0.2s ease;
}

@keyframes welcome-fade {
  from { opacity: 0; }
  to { opacity: 1; }
}

.welcome-card {
  position: relative;
  width: min(400px, 90vw);
  padding: 2.2rem 2rem 1.8rem;
  border-radius: var(--radius-lg, 20px);
  background: var(--color-bg-secondary, #fff);
  border: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  box-shadow: var(--shadow-xl, 0 24px 48px rgba(0, 0, 0, 0.16));
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  animation: welcome-card-in 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}

@keyframes welcome-card-in {
  from { opacity: 0; transform: translateY(8px) scale(0.96); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}

.welcome-close {
  position: absolute;
  top: 14px;
  right: 14px;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--color-border-light);
  border-radius: 10px;
  background: var(--color-surface, #f9fafb);
  color: var(--color-text-tertiary);
  cursor: pointer;
  font-size: 0.9rem;
  transition: background 0.15s, color 0.15s;
}

.welcome-close:hover {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
}

.welcome-logo {
  width: 64px;
  height: 64px;
  border-radius: 16px;
  box-shadow: var(--shadow-sm);
  margin-bottom: 0.6rem;
}

.welcome-title {
  font-family: var(--font-display, system-ui);
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.welcome-version {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  margin-top: 0.1rem;
  margin-bottom: 1rem;
}

.welcome-features {
  list-style: none;
  padding: 0;
  margin: 0;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.welcome-features li {
  display: grid;
  grid-template-columns: auto 1fr;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 0.9rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm, 10px);
  text-align: left;
}

.feature-icon {
  width: 32px;
  height: 32px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.1));
  font-size: 1rem;
}

.welcome-features strong {
  font-size: 0.9rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.welcome-features small {
  display: block;
  margin-top: 0.12rem;
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}

.welcome-btn {
  margin-top: 1.2rem;
  padding: 0.65rem 2rem;
  border: none;
  border-radius: 10px;
  background: var(--color-highlight, #0f766e);
  color: var(--color-text-inverse, #fff);
  font-size: 0.88rem;
  font-weight: 700;
  cursor: pointer;
  transition: transform 0.15s, box-shadow 0.15s;
  box-shadow: 0 8px 18px rgba(15, 118, 110, 0.2);
}

.welcome-btn:hover {
  transform: translateY(-1px);
  box-shadow: 0 12px 24px rgba(15, 118, 110, 0.28);
}
</style>
