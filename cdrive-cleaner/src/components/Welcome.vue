<script setup lang="ts">
import { ref, onMounted } from 'vue';
import appIcon from '../assets/app-icon.svg';

const emit = defineEmits<{
  'close': [];
}>();

const dontShowAgain = ref(false);

function handleStart() {
  if (dontShowAgain.value) {
    localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
  }
  emit('close');
}

function handleSkip() {
  localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
  emit('close');
}
</script>

<template>
  <div class="welcome-overlay">
    <div class="welcome-container">
      <!-- Logo 区域 -->
      <div class="logo-section">
        <img :src="appIcon" alt="C盘清理工具" class="app-logo" width="140" height="140" />
        <h1 class="app-title">C 盘清理工具</h1>
        <p class="app-subtitle">快速释放空间，智能迁移文件</p>
      </div>

      <!-- 分隔线 -->
      <div class="divider"></div>

      <!-- 功能介绍 -->
      <div class="features-section">
        <h2 class="section-title">核心功能</h2>
        <div class="features-grid">
          <div class="feature-item">
            <div class="feature-icon scan">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                <circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="2" opacity="0.3"/>
                <path d="M12 8v8M8 12h8" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
              </svg>
            </div>
            <div class="feature-content">
              <h3>快速扫描</h3>
              <p>分析磁盘空间占用，找出大文件和目录</p>
            </div>
          </div>

          <div class="feature-item">
            <div class="feature-icon migrate">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                <path d="M5 12h14M12 5l7 7-7 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-content">
              <h3>智能迁移</h3>
              <p>安全移动文件到其他磁盘，释放 C 盘空间</p>
            </div>
          </div>

          <div class="feature-item">
            <div class="feature-icon link">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-content">
              <h3>符号链接</h3>
              <p>保持原路径可访问，应用程序无需修改配置</p>
            </div>
          </div>

          <div class="feature-item">
            <div class="feature-icon history">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                <path d="M12 8v4l3 3" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                <path d="M3.05 11a9 9 0 1 1 .5 4m-.5 5v-5h5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </div>
            <div class="feature-content">
              <h3>历史记录</h3>
              <p>随时查看迁移历史，支持一键回滚操作</p>
            </div>
          </div>
        </div>
      </div>

      <!-- 底部操作 -->
      <div class="actions-section">
        <label class="checkbox-label">
          <input type="checkbox" v-model="dontShowAgain" class="checkbox" />
          <span class="checkbox-text">不再显示此页面</span>
        </label>
        
        <div class="button-group">
          <button class="btn btn-secondary" @click="handleSkip">
            跳过
          </button>
          <button class="btn btn-primary" @click="handleStart">
            开始使用
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
@import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;500;600;700&display=swap');

.welcome-overlay {
  position: fixed;
  inset: 0;
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.6) 0%, rgba(0, 0, 0, 0.4) 100%);
  backdrop-filter: blur(24px) saturate(120%);
  -webkit-backdrop-filter: blur(24px) saturate(120%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  animation: fadeIn 0.5s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: 'DM Sans', -apple-system, BlinkMacSystemFont, sans-serif;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    backdrop-filter: blur(0px);
  }
  to {
    opacity: 1;
    backdrop-filter: blur(24px) saturate(120%);
  }
}

.welcome-container {
  width: 90%;
  max-width: 680px;
  max-height: 90vh;
  background: linear-gradient(to bottom, #ffffff 0%, #fefefe 100%);
  border-radius: 28px;
  box-shadow: 
    0 0 0 1px rgba(0, 0, 0, 0.04),
    0 4px 12px rgba(0, 0, 0, 0.04),
    0 16px 48px rgba(0, 0, 0, 0.12),
    0 32px 80px rgba(0, 0, 0, 0.16);
  overflow: hidden;
  animation: slideUp 0.6s cubic-bezier(0.16, 1, 0.3, 1);
  display: flex;
  flex-direction: column;
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(40px) scale(0.92);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

/* Logo 区域 */
.logo-section {
  padding: 4rem 3rem 3rem;
  text-align: center;
  background: linear-gradient(135deg, rgba(0, 122, 255, 0.02) 0%, transparent 100%);
}

.app-logo {
  animation: logoFloat 0.8s cubic-bezier(0.16, 1, 0.3, 1) 0.2s backwards;
}

@keyframes logoFloat {
  from {
    opacity: 0;
    transform: translateY(-20px) scale(0.8);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.app-title {
  font-size: 2.25rem;
  font-weight: 700;
  color: #0a0a0a;
  margin: 1.5rem 0 0.75rem;
  letter-spacing: -0.03em;
  animation: fadeInUp 0.6s cubic-bezier(0.16, 1, 0.3, 1) 0.3s backwards;
}

.app-subtitle {
  font-size: 1.125rem;
  color: #666;
  margin: 0;
  font-weight: 500;
  animation: fadeInUp 0.6s cubic-bezier(0.16, 1, 0.3, 1) 0.4s backwards;
}

@keyframes fadeInUp {
  from {
    opacity: 0;
    transform: translateY(12px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* 分隔线 */
.divider {
  height: 1px;
  background: linear-gradient(to right, transparent 0%, rgba(0, 0, 0, 0.08) 50%, transparent 100%);
  margin: 0 3rem;
}

/* 功能区域 */
.features-section {
  padding: 3rem;
  flex: 1;
  overflow-y: auto;
}

.features-section::-webkit-scrollbar {
  width: 6px;
}

.features-section::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 3px;
}

.section-title {
  font-size: 1.125rem;
  font-weight: 600;
  color: #0a0a0a;
  margin: 0 0 1.75rem;
  letter-spacing: -0.01em;
}

.features-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 1.5rem;
}

.feature-item {
  display: flex;
  gap: 1rem;
  padding: 1.25rem;
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.01) 0%, rgba(0, 0, 0, 0.02) 100%);
  border: 1px solid rgba(0, 0, 0, 0.06);
  border-radius: 16px;
  transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  animation: fadeInUp 0.5s cubic-bezier(0.16, 1, 0.3, 1) backwards;
}

.feature-item:nth-child(1) { animation-delay: 0.5s; }
.feature-item:nth-child(2) { animation-delay: 0.6s; }
.feature-item:nth-child(3) { animation-delay: 0.7s; }
.feature-item:nth-child(4) { animation-delay: 0.8s; }

.feature-item:hover {
  background: linear-gradient(135deg, rgba(0, 122, 255, 0.04) 0%, rgba(0, 122, 255, 0.02) 100%);
  border-color: rgba(0, 122, 255, 0.2);
  transform: translateY(-2px);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
}

.feature-icon {
  width: 48px;
  height: 48px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}

.feature-icon.scan {
  background: linear-gradient(135deg, rgba(0, 122, 255, 0.12) 0%, rgba(0, 122, 255, 0.08) 100%);
  color: #007aff;
}

.feature-icon.migrate {
  background: linear-gradient(135deg, rgba(16, 185, 129, 0.12) 0%, rgba(16, 185, 129, 0.08) 100%);
  color: #10b981;
}

.feature-icon.link {
  background: linear-gradient(135deg, rgba(139, 92, 246, 0.12) 0%, rgba(139, 92, 246, 0.08) 100%);
  color: #8b5cf6;
}

.feature-icon.history {
  background: linear-gradient(135deg, rgba(245, 158, 11, 0.12) 0%, rgba(245, 158, 11, 0.08) 100%);
  color: #f59e0b;
}

.feature-content {
  flex: 1;
}

.feature-content h3 {
  font-size: 1rem;
  font-weight: 600;
  color: #0a0a0a;
  margin: 0 0 0.375rem;
  letter-spacing: -0.01em;
}

.feature-content p {
  font-size: 0.875rem;
  color: #666;
  margin: 0;
  line-height: 1.5;
}

/* 底部操作 */
.actions-section {
  padding: 2rem 3rem 2.5rem;
  border-top: 1px solid rgba(0, 0, 0, 0.06);
  background: linear-gradient(to top, rgba(250, 250, 250, 0.6) 0%, transparent 100%);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1.5rem;
  animation: fadeInUp 0.5s cubic-bezier(0.16, 1, 0.3, 1) 0.9s backwards;
}

.checkbox-label {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  cursor: pointer;
  user-select: none;
}

.checkbox {
  width: 20px;
  height: 20px;
  cursor: pointer;
  accent-color: #007aff;
}

.checkbox-text {
  font-size: 0.9375rem;
  color: #666;
  font-weight: 500;
}

.button-group {
  display: flex;
  gap: 0.875rem;
}

.btn {
  padding: 0.875rem 2rem;
  font-size: 1rem;
  font-weight: 600;
  border: none;
  border-radius: 12px;
  cursor: pointer;
  transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  letter-spacing: -0.01em;
  white-space: nowrap;
}

.btn-secondary {
  color: #666;
  background: linear-gradient(to bottom, rgba(0, 0, 0, 0.04) 0%, rgba(0, 0, 0, 0.06) 100%);
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.btn-secondary:hover {
  background: linear-gradient(to bottom, rgba(0, 0, 0, 0.06) 0%, rgba(0, 0, 0, 0.08) 100%);
  transform: translateY(-2px);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
}

.btn-primary {
  color: #ffffff;
  background: linear-gradient(135deg, #007aff 0%, #0051d5 100%);
  box-shadow: 
    0 0 0 1px rgba(0, 122, 255, 0.2),
    0 4px 16px rgba(0, 122, 255, 0.3);
}

.btn-primary:hover {
  transform: translateY(-3px);
  box-shadow: 
    0 0 0 1px rgba(0, 122, 255, 0.3),
    0 8px 24px rgba(0, 122, 255, 0.4);
}

/* 响应式 */
@media (max-width: 768px) {
  .welcome-container {
    width: 95%;
    max-height: 95vh;
  }

  .logo-section {
    padding: 3rem 2rem 2rem;
  }

  .app-title {
    font-size: 1.875rem;
  }

  .app-subtitle {
    font-size: 1rem;
  }

  .features-section {
    padding: 2rem;
  }

  .features-grid {
    grid-template-columns: 1fr;
    gap: 1rem;
  }

  .actions-section {
    flex-direction: column;
    align-items: stretch;
    padding: 1.5rem 2rem 2rem;
  }

  .button-group {
    width: 100%;
  }

  .btn {
    flex: 1;
  }
}
</style>
