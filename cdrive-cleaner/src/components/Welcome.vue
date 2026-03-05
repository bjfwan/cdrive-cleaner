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

<style scoped src="../styles/welcome.css"></style>
