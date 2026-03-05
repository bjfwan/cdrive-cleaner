<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import ConfirmDialog from './ConfirmDialog.vue';
import { IconClose, IconRefresh, IconLink, IconInfo, IconWarning, IconShield } from './icons';

interface DiskInfo {
  drive_letter: string;
  label: string;
  free_space: number;
}

interface Props {
  show: boolean;
  availableDisks: DiskInfo[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'close': [];
  'save': [settings: Settings];
}>();

interface Settings {
  defaultTargetDisk: string;
  largeFileThreshold: number;
  createSymlink: boolean;
}

const settings = ref<Settings>({
  defaultTargetDisk: '',
  largeFileThreshold: 100,
  createSymlink: true
});

const showResetConfirm = ref(false);
const showRestartConfirm = ref(false);
const isElevated = ref(false);
const isCheckingElevation = ref(true);

onMounted(() => {
  loadSettings();
  checkElevation();
});

async function checkElevation() {
  try {
    isElevated.value = await invoke<boolean>('is_elevated');
  } catch (e) {
    console.error('Failed to check elevation:', e);
  } finally {
    isCheckingElevation.value = false;
  }
}

function loadSettings() {
  const saved = localStorage.getItem('cdrive-cleaner-settings');
  if (saved) {
    try {
      settings.value = { ...settings.value, ...JSON.parse(saved) };
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
  }
}

function saveSettings() {
  try {
    localStorage.setItem('cdrive-cleaner-settings', JSON.stringify(settings.value));
    emit('save', settings.value);
    emit('close');
  } catch (e) {
    console.error('Failed to save settings:', e);
  }
}

function confirmReset() {
  settings.value = {
    defaultTargetDisk: '',
    largeFileThreshold: 100,
    createSymlink: true
  };
  showResetConfirm.value = false;
}

async function confirmRestartAsAdmin() {
  try {
    await invoke('restart_as_admin');
  } catch (e) {
    console.error('Failed to restart as admin:', e);
    alert('重启失败，请手动以管理员身份运行应用');
  }
  showRestartConfirm.value = false;
}

function close() {
  emit('close');
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}
</script>

<template>
  <div v-if="show" class="overlay" @click.self="close">
    <div class="settings-panel" @click.stop>
      <div class="panel-header">
        <h2>设置</h2>
        <button class="close-btn" @click="close">
          <IconClose :size="20" />
        </button>
      </div>

      <div class="panel-body">
        <div class="setting-section">
          <div class="section-header">
            <h3>迁移设置</h3>
            <p>配置文件迁移的默认行为</p>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label for="target-disk">默认目标磁盘</label>
              <span class="setting-description">迁移文件时的默认目标位置</span>
            </div>
            <select 
              id="target-disk" 
              v-model="settings.defaultTargetDisk" 
              class="setting-select"
              @click.stop
            >
              <option value="">每次选择</option>
              <option 
                v-for="disk in availableDisks" 
                :key="disk.drive_letter"
                :value="disk.drive_letter + '\\'"
              >
                {{ disk.drive_letter }} - {{ disk.label }} ({{ formatBytes(disk.free_space) }} 可用)
              </option>
            </select>
          </div>

          <div class="setting-item">
            <div class="setting-label">
              <label for="threshold">大文件阈值</label>
              <span class="setting-description">超过此大小的文件将在"大文件"视图中显示</span>
            </div>
            <div class="threshold-input-group">
              <input 
                id="threshold" 
                type="number" 
                v-model.number="settings.largeFileThreshold" 
                min="1" 
                max="10000"
                class="setting-input"
                @click.stop
              />
              <span class="input-suffix">MB</span>
            </div>
          </div>

          <div class="setting-item symlink-setting">
            <div class="setting-label">
              <div class="label-with-icon">
                <IconLink :size="20" />
                <label for="symlink">创建符号链接</label>
              </div>
              <span class="setting-description">迁移后在原位置创建符号链接，保持路径可访问</span>
            </div>
            <label class="toggle-switch">
              <input 
                id="symlink"
                type="checkbox" 
                v-model="settings.createSymlink"
                @click.stop
              />
              <span class="toggle-slider"></span>
            </label>
          </div>

          <div v-if="settings.createSymlink" class="info-card info-card-enabled">
            <div class="info-icon">
              <IconInfo :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">符号链接的作用</div>
              <ul class="info-list">
                <li>保持原路径可访问，应用程序无需修改配置</li>
                <li>透明重定向到新位置，用户体验无感知</li>
                <li>支持回滚操作，可随时恢复原状</li>
              </ul>
            </div>
          </div>

          <div v-if="!settings.createSymlink" class="info-card info-card-warning">
            <div class="info-icon warning">
              <IconWarning :size="20" />
            </div>
            <div class="info-content">
              <div class="info-title">不创建符号链接的影响</div>
              <ul class="info-list">
                <li>文件将被完全移动，原路径将不再存在</li>
                <li>依赖此路径的程序可能无法正常运行</li>
                <li>需要手动更新应用程序的配置路径</li>
              </ul>
            </div>
          </div>
        </div>

        <div class="setting-section">
          <div class="section-header">
            <h3>权限管理</h3>
            <p>某些操作可能需要管理员权限</p>
          </div>

          <div v-if="!isCheckingElevation" class="admin-status">
            <div v-if="isElevated" class="status-badge status-elevated">
              <IconShield :size="16" />
              <span>当前以管理员身份运行</span>
            </div>
            <div v-else class="status-badge status-normal">
              <IconInfo :size="16" />
              <span>当前以普通用户身份运行</span>
            </div>
          </div>

          <div v-if="!isElevated && !isCheckingElevation" class="admin-info">
            <p>以管理员身份运行可以：</p>
            <ul>
              <li>访问系统保护的文件和文件夹</li>
              <li>迁移需要特殊权限的文件</li>
              <li>执行某些高级操作</li>
            </ul>
          </div>

          <button 
            v-if="!isElevated && !isCheckingElevation"
            @click.stop="showRestartConfirm = true" 
            class="admin-restart-btn"
          >
            <IconRefresh :size="16" />
            以管理员身份重启
          </button>
        </div>

        <div class="setting-section">
          <div class="section-header">
            <h3>重置设置</h3>
            <p>恢复所有设置到默认值</p>
          </div>

          <button @click.stop="showResetConfirm = true" class="reset-btn">
            <IconRefresh :size="16" />
            重置所有设置
          </button>
        </div>
      </div>

      <div class="panel-footer">
        <button class="btn btn-secondary" @click="close">取消</button>
        <button class="btn btn-primary" @click="saveSettings">保存设置</button>
      </div>
    </div>

    <ConfirmDialog
      :show="showResetConfirm"
      title="确定要重置所有设置吗？"
      message="此操作将恢复所有设置到默认值，且无法撤销"
      confirm-text="确定重置"
      cancel-text="取消"
      type="danger"
      @confirm="confirmReset"
      @cancel="showResetConfirm = false"
    />

    <ConfirmDialog
      :show="showRestartConfirm"
      title="以管理员身份重启"
      message="应用将关闭并以管理员权限重新启动。请在弹出的 UAC 提示中点击"是"。"
      confirm-text="立即重启"
      cancel-text="取消"
      type="warning"
      @confirm="confirmRestartAsAdmin"
      @cancel="showRestartConfirm = false"
    />
  </div>
</template>

<style scoped src="./Settings.css"></style>
