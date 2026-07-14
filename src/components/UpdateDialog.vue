<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { IconClose, IconInfo, IconRefresh } from './icons';
import type { UpdateInfo } from '../types';

interface Props {
  show: boolean;
  updateInfo: UpdateInfo | null;
  checking?: boolean;
  checkError?: string;
}

const props = withDefaults(defineProps<Props>(), {
  checking: false,
  checkError: '',
});

const emit = defineEmits<{
  close: [];
  retry: [];
}>();

const openError = ref('');

async function openInBrowser() {
  if (!props.updateInfo) return;
  openError.value = '';
  try {
    await invoke('open_url', { url: props.updateInfo.download_url });
  } catch (err) {
    openError.value = String(err);
  }
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '';
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString('zh-CN', { year: 'numeric', month: 'long', day: 'numeric' });
  } catch {
    return dateStr;
  }
}

</script>

<template>
  <div v-if="show" class="update-overlay" @click.self="emit('close')">
    <div class="update-dialog" @click.stop>
      <button class="close-btn" type="button" aria-label="关闭更新窗口" @click="emit('close')">
        <IconClose :size="18" />
      </button>

      <!-- Checking state -->
      <div v-if="checking" class="update-checking">
        <div class="checking-spinner"></div>
        <h3>正在检查更新</h3>
        <p>正在连接 GitHub 获取最新版本信息…</p>
      </div>

      <!-- Check failed -->
      <div v-else-if="checkError && !checking" class="update-content">
        <h3 class="update-title error-title">检查失败</h3>
        <p class="update-current">{{ checkError }}</p>
        <div class="update-actions">
          <button class="btn btn-secondary" @click="emit('close')">关闭</button>
          <button class="btn btn-primary" @click="emit('retry')">
            <IconRefresh :size="16" />
            重试
          </button>
        </div>
      </div>

      <!-- Has update -->
      <div v-else-if="updateInfo?.has_update" class="update-content">
        <div class="update-badge">
          <span class="badge-new">新版本</span>
        </div>

        <h3 class="update-title">
          v{{ updateInfo.latest_version }} 已发布
        </h3>
        <p class="update-current">
          当前版本：v{{ updateInfo.current_version }}
          <template v-if="updateInfo.published_at">
            · 发布于 {{ formatDate(updateInfo.published_at) }}
          </template>
        </p>

        <div v-if="updateInfo.release_notes" class="update-notes">
          <h4>更新日志</h4>
          <div class="notes-content">{{ updateInfo.release_notes }}</div>
        </div>

        <p class="update-hint">为确保安装包安全，应用将打开官方 GitHub Releases 页面供您下载。</p>
        <p v-if="openError" class="update-current error-title">{{ openError }}</p>
        <div class="update-actions">
          <button class="btn btn-secondary" @click="emit('close')">稍后再说</button>
          <button class="btn btn-primary" @click="openInBrowser">
            <IconRefresh :size="16" />
            前往 GitHub 下载
          </button>
        </div>
      </div>

      <!-- No update -->
      <div v-else-if="updateInfo && !updateInfo.has_update" class="update-content update-latest">
        <div class="update-icon-ok">
          <IconInfo :size="48" />
        </div>
        <h3>已是最新版本</h3>
        <p class="update-current">
          当前版本：v{{ updateInfo.current_version }}
        </p>
        <div class="update-actions">
          <button class="btn btn-secondary" @click="emit('close')">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.update-overlay {
  position: fixed;
  inset: 0;
  background: var(--modal-backdrop);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2500;
  animation: updateFadeIn 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans);
}

@keyframes updateFadeIn {
  from { opacity: 0; }
  to { opacity: 1; }
}

.update-dialog {
  width: 90%;
  max-width: 480px;
  max-height: 80vh;
  padding: 2rem;
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border-radius: 24px;
  box-shadow:
    0 0 0 1px var(--color-border-light),
    var(--shadow-lg),
    var(--shadow-xl);
  animation: updateSlideUp 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  position: relative;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

@keyframes updateSlideUp {
  from {
    opacity: 0;
    transform: translateY(20px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.close-btn {
  position: absolute;
  top: 1rem;
  right: 1rem;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(139, 92, 46, 0.06);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-base);
  z-index: 1;
}

.close-btn:hover {
  background: rgba(139, 92, 46, 0.12);
  border-color: var(--color-border-medium);
  color: var(--color-text-primary);
}

/* Checking state */
.update-checking {
  text-align: center;
  padding: 2rem 0;
}

.checking-spinner {
  width: 40px;
  height: 40px;
  margin: 0 auto 1.5rem;
  border: 3px solid var(--color-border-light);
  border-top-color: var(--color-accent-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.update-checking h3 {
  font-family: var(--font-serif);
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.5rem;
}

.update-checking p {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
}

/* Update content */
.update-content {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.update-badge {
  display: flex;
  justify-content: center;
}

.badge-new {
  display: inline-flex;
  align-items: center;
  padding: 0.375rem 1rem;
  font-size: 0.8125rem;
  font-weight: 700;
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary, #b8860b) 100%);
  border-radius: 20px;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  box-shadow: 0 2px 8px rgba(139, 115, 85, 0.3);
}

.update-title {
  font-family: var(--font-serif);
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--color-text-primary);
  text-align: center;
  margin: 0;
}

.update-current {
  font-size: 0.875rem;
  color: var(--color-text-tertiary);
  text-align: center;
  margin: 0;
}

.update-notes {
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  padding: 1rem 1.25rem;
  max-height: 240px;
  overflow-y: auto;
}

.update-notes h4 {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--color-text-secondary);
  margin-bottom: 0.75rem;
}

.notes-content {
  font-size: 0.8125rem;
  line-height: 1.7;
  color: var(--color-text-primary);
  white-space: pre-wrap;
  word-break: break-word;
}

.update-notes::-webkit-scrollbar {
  width: 5px;
}

.update-notes::-webkit-scrollbar-track {
  background: transparent;
}

.update-notes::-webkit-scrollbar-thumb {
  background: rgba(139, 92, 46, 0.15);
  border-radius: 3px;
}

.update-actions {
  display: flex;
  gap: 0.875rem;
  justify-content: center;
  padding-top: 0.5rem;
}

.btn {
  min-width: 120px;
  padding: 0.75rem 1.75rem;
  font-size: 0.9375rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all var(--transition-base);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
}

.btn-secondary {
  color: var(--color-text-primary);
  background: rgba(139, 92, 46, 0.12);
  border: 1px solid var(--color-border-strong);
}

.btn-secondary:hover {
  background: rgba(139, 92, 46, 0.18);
  transform: translateY(-2px);
  box-shadow: var(--shadow-md);
}

[data-theme="dark"] .btn-secondary {
  background: rgba(255, 255, 255, 0.06);
  border-color: var(--color-border-medium);
}

[data-theme="dark"] .btn-secondary:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: var(--color-border-strong);
}

.btn-primary {
  color: #ffffff;
  background: linear-gradient(135deg, var(--color-accent-primary) 0%, var(--color-accent-secondary, #b8860b) 100%);
  box-shadow:
    0 0 0 1px rgba(139, 115, 85, 0.2),
    0 4px 16px rgba(139, 115, 85, 0.3);
}

.btn-primary:hover {
  transform: translateY(-3px);
  box-shadow:
    0 0 0 1px rgba(139, 115, 85, 0.3),
    0 8px 24px rgba(139, 115, 85, 0.4);
}

/* Progress bar */
.progress-bar-container {
  width: 100%;
  height: 8px;
  background: var(--color-border-light);
  border-radius: 4px;
  overflow: hidden;
}

.progress-bar {
  height: 100%;
  background: linear-gradient(90deg, var(--color-accent-primary) 0%, var(--color-accent-secondary, #b8860b) 100%);
  border-radius: 4px;
  transition: width 0.2s ease;
}

.progress-text {
  font-size: 0.8125rem;
  color: var(--color-text-tertiary);
  text-align: center;
  margin: 0;
}

.error-title {
  color: var(--color-error, #ef4444);
}

.update-hint {
  font-size: 0.75rem;
  color: var(--color-text-tertiary);
  text-align: center;
  margin: 0;
  padding-top: 0.25rem;
  opacity: 0.7;
}

/* Latest version state */
.update-latest {
  text-align: center;
  padding: 1rem 0;
}

.update-icon-ok {
  display: flex;
  justify-content: center;
  color: var(--color-success, #22c55e);
  margin-bottom: 0.5rem;
}

.update-latest h3 {
  font-family: var(--font-serif);
  font-size: 1.375rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0;
}

@media (max-width: 480px) {
  .update-dialog {
    width: 95%;
    padding: 1.5rem;
  }

  .update-actions {
    flex-direction: column;
  }

  .btn {
    width: 100%;
  }
}
</style>
