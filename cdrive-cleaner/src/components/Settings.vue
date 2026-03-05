<script setup lang="ts">
import { ref, onMounted } from 'vue';
import ConfirmDialog from './ConfirmDialog.vue';
import { IconClose, IconRefresh } from './icons';

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
}

const settings = ref<Settings>({
  defaultTargetDisk: '',
  largeFileThreshold: 100
});

const showResetConfirm = ref(false);

onMounted(() => {
  loadSettings();
});

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
    largeFileThreshold: 100
  };
  showResetConfirm.value = false;
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
  </div>
</template>

<style scoped src="./Settings.css"></style>
