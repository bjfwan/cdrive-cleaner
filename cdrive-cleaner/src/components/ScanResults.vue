<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
  is_symlink: boolean;
  link_target?: string;
}

interface ScanResult {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: DirectoryNode[];
  inaccessible_count: number;
}

interface Props {
  result: ScanResult;
  viewMode: 'treemap' | 'list';
  canGoBack: boolean;
  currentPath: string;
  deepScanning: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'update:viewMode': [mode: 'treemap' | 'list'];
  'navigate': [path: string];
  'goBack': [];
}>();

console.log('ScanResults 接收到的数据:', props.result);
console.log('directories 数量:', props.result?.directories?.length || 0);
if (props.result?.directories?.length > 0) {
  console.log('第一个目录:', props.result.directories[0]);
}

function handleItemClick(dir: DirectoryNode) {
  if (props.deepScanning && (!dir.children || dir.children.length === 0)) {
    return;
  }
  emit('navigate', dir.path);
}

function handleChartClick(params: any) {
  if (params.data && params.data.path) {
    if (props.deepScanning && params.data.hasChildren === false) {
      return;
    }
    emit('navigate', params.data.path);
  }
}

const treemapOption = computed(() => {
  const colors = [
    '#007aff', '#5856d6', '#af52de', '#ff2d55', 
    '#ff3b30', '#ff9500', '#ffcc00', '#34c759',
    '#00c7be', '#30b0c7', '#32ade6'
  ];

  const directories = props.result?.directories || [];
  console.log('treemapOption: directories 数量', directories.length);

  return {
    tooltip: {
      formatter: (info: any) => {
        const status = props.deepScanning && !info.data.hasChildren ? '<br/>(扫描中...)' : '';
        return `${info.name}<br/>${formatBytes(info.value)}${status}`;
      },
      backgroundColor: 'rgba(255, 255, 255, 0.95)',
      borderColor: '#e7e5e4',
      borderWidth: 1,
      textStyle: {
        color: '#2c2c2c',
        fontSize: 13
      }
    },
    series: [{
      type: 'treemap',
      left: 0,
      right: 0,
      top: 0,
      bottom: 0,
      squareRatio: 0.6,
      leafDepth: 1,
      visibleMin: 100,
      data: directories.map((dir, index) => ({
        name: dir.name,
        value: dir.size,
        path: dir.path,
        hasChildren: dir.children && dir.children.length > 0,
        itemStyle: {
          color: colors[index % colors.length],
          opacity: props.deepScanning && (!dir.children || dir.children.length === 0) ? 0.5 : 1
        }
      })),
      roam: false,
      nodeClick: 'link',
      breadcrumb: { show: false },
      label: {
        show: true,
        formatter: (params: any) => {
          const area = params.width * params.height;
          if (area < 1500) return '';
          if (area < 4000) return params.name.substring(0, 8);
          return params.name;
        },
        fontSize: 12,
        color: '#fff',
        fontWeight: 500,
        overflow: 'truncate'
      },
      upperLabel: {
        show: true,
        height: 32,
        color: '#fff',
        fontSize: 13,
        fontWeight: 600
      },
      itemStyle: {
        borderColor: '#fff',
        borderWidth: 3,
        gapWidth: 3,
        shadowBlur: 8,
        shadowColor: 'rgba(0, 0, 0, 0.1)'
      },
      emphasis: {
        itemStyle: {
          shadowBlur: 12,
          shadowColor: 'rgba(0, 0, 0, 0.2)'
        }
      },
      levels: [
        {
          itemStyle: {
            borderWidth: 0,
            gapWidth: 5
          }
        }
      ]
    }]
  };
});

const sortedDirectories = computed(() => {
  if (!props.result?.directories || props.result.directories.length === 0) {
    console.warn('sortedDirectories: directories 为空或未定义');
    return [];
  }
  return [...props.result.directories].sort((a, b) => b.size - a.size);
});

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function formatNumber(num: number): string {
  return num.toLocaleString('zh-CN');
}
</script>

<template>
  <div class="results">
    <header class="header">
      <div class="header-info">
        <div class="breadcrumb">
          <button 
            v-if="canGoBack"
            @click="emit('goBack')"
            class="back-btn"
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M12.5 15L7.5 10L12.5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <h2>{{ currentPath }}</h2>
        </div>
        <div class="stats">
          <span class="stat-item">{{ formatBytes(result.total_size) }}</span>
          <span class="separator">·</span>
          <span class="stat-item">{{ formatNumber(result.total_files) }} 个文件</span>
          <span class="separator">·</span>
          <span class="stat-item">{{ formatNumber(result.total_dirs) }} 个目录</span>
        </div>
      </div>
      <div class="view-switcher">
        <button 
          class="view-btn"
          :class="{ active: viewMode === 'treemap' }"
          @click="$emit('update:viewMode', 'treemap')"
        >
          树状图
        </button>
        <button 
          class="view-btn"
          :class="{ active: viewMode === 'list' }"
          @click="$emit('update:viewMode', 'list')"
        >
          列表
        </button>
      </div>
    </header>

    <div class="body">
      <div v-if="viewMode === 'treemap'" class="view-treemap">
        <div class="treemap-card">
          <div class="treemap-container">
            <v-chart 
              :option="treemapOption" 
              class="chart" 
              autoresize 
              @click="handleChartClick"
            />
          </div>
        </div>
      </div>

      <div v-if="viewMode === 'list'" class="view-list">
        <div class="table-header">
          <div class="th th-name">名称</div>
          <div class="th th-size">大小</div>
          <div class="th th-files">文件数</div>
        </div>
        <div class="table-body">
          <div 
            v-for="dir in sortedDirectories" 
            :key="dir.path"
            class="table-row"
            :class="{ 'row-disabled': deepScanning && (!dir.children || dir.children.length === 0) }"
            @click="handleItemClick(dir)"
          >
            <div class="td td-name">
              <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                <path d="M2 4.5C2 3.67157 2.67157 3 3.5 3H6L7.5 4.5H14.5C15.3284 4.5 16 5.17157 16 6V13.5C16 14.3284 15.3284 15 14.5 15H3.5C2.67157 15 2 14.3284 2 13.5V4.5Z" fill="#2c2c2c"/>
              </svg>
              <span>{{ dir.name }}</span>
              <span v-if="deepScanning && (!dir.children || dir.children.length === 0)" class="scanning-badge">扫描中</span>
            </div>
            <div class="td td-size">{{ formatBytes(dir.size) }}</div>
            <div class="td td-files">{{ formatNumber(dir.file_count) }}</div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="result.inaccessible_count > 0" class="notice">
      {{ result.inaccessible_count }} 个项目无法访问
    </div>
  </div>
</template>

<style scoped>
.results {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
}

.header {
  padding: 1.5rem 2rem;
  border-bottom: 1px solid #e7e5e4;
  background: linear-gradient(to bottom, #ffffff 0%, #fafaf9 100%);
  flex-shrink: 0;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1.5rem;
  flex-wrap: wrap;
}

.header-info h2 {
  font-size: 1.25rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.5rem;
  letter-spacing: -0.01em;
}

.breadcrumb {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-bottom: 0.5rem;
}

.back-btn {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border: none;
  border-radius: 8px;
  color: #2c2c2c;
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.back-btn:hover {
  background: #e7e5e4;
  color: #007aff;
}

.stats {
  font-size: 0.875rem;
  color: #78716c;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.separator {
  color: #d6d3d1;
}

.view-switcher {
  display: flex;
  gap: 0.5rem;
  background: #f5f5f4;
  padding: 0.25rem;
  border-radius: 10px;
  flex-shrink: 0;
}

.view-btn {
  padding: 0.5rem 1.25rem;
  font-size: 0.875rem;
  font-weight: 500;
  background: transparent;
  color: #78716c;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.view-btn:hover {
  color: #007aff;
  background: rgba(0, 122, 255, 0.05);
}

.view-btn.active {
  background: white;
  color: #007aff;
  box-shadow: 0 2px 8px rgba(0, 122, 255, 0.15);
}

.body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 2rem;
  overflow: hidden;
}

.view-treemap {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.treemap-card {
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 16px;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  height: 60%;
  max-height: 500px;
  min-height: 300px;
}

.treemap-container {
  width: 100%;
  height: 100%;
  position: relative;
  padding: 1.5rem;
}

.chart {
  position: absolute;
  top: 1.5rem;
  left: 1.5rem;
  right: 1.5rem;
  bottom: 1.5rem;
  width: calc(100% - 3rem);
  height: calc(100% - 3rem);
}

.view-list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.table-header {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) minmax(100px, 140px) minmax(80px, 120px);
  gap: 1.5rem;
  padding: 0 0 1rem;
  border-bottom: 1px solid #e7e5e4;
  flex-shrink: 0;
}

.th {
  font-size: 0.75rem;
  font-weight: 600;
  color: #78716c;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.table-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.table-row {
  display: grid;
  grid-template-columns: minmax(200px, 1fr) minmax(100px, 140px) minmax(80px, 120px);
  gap: 1.5rem;
  padding: 1rem 0;
  border-bottom: 1px solid #f5f5f4;
  transition: all 0.2s ease;
  cursor: pointer;
}

.table-row:hover {
  background: linear-gradient(to right, rgba(0, 122, 255, 0.03) 0%, transparent 100%);
  margin: 0 -2rem;
  padding-left: 2rem;
  padding-right: 2rem;
  border-left: 2px solid #007aff;
}

.table-row.row-disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.table-row.row-disabled:hover {
  background: none;
  margin: 0;
  padding: 1rem 0;
  border-left: none;
}

.scanning-badge {
  padding: 0.125rem 0.5rem;
  background: #fef3c7;
  color: #92400e;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
}

.td {
  display: flex;
  align-items: center;
  font-size: 0.9375rem;
}

.td-name {
  gap: 0.75rem;
  font-weight: 500;
  color: #2c2c2c;
  overflow: hidden;
}

.td-name span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.td-size {
  color: #2c2c2c;
  font-weight: 500;
}

.td-files {
  color: #78716c;
}

.notice {
  position: absolute;
  bottom: 1rem;
  left: 50%;
  transform: translateX(-50%);
  padding: 0.75rem 1.25rem;
  background: rgba(255, 251, 235, 0.95);
  color: #92400e;
  border: 1px solid #fde68a;
  border-radius: 8px;
  font-size: 0.875rem;
  backdrop-filter: blur(8px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  z-index: 10;
}

@media (max-width: 900px) {
  .header {
    flex-direction: column;
    align-items: flex-start;
    padding: 1.25rem 1.5rem;
  }

  .view-switcher {
    width: 100%;
  }

  .view-btn {
    flex: 1;
  }

  .body {
    padding: 1.5rem;
  }

  .treemap-card {
    height: 50%;
    min-height: 250px;
  }

  .treemap-container {
    padding: 1rem;
  }

  .chart {
    top: 1rem;
    left: 1rem;
    right: 1rem;
    bottom: 1rem;
    width: calc(100% - 2rem);
    height: calc(100% - 2rem);
  }

  .table-header,
  .table-row {
    grid-template-columns: 1fr 100px 80px;
    gap: 1rem;
  }

  .table-row:hover {
    margin: 0 -1.5rem;
    padding-left: 1.5rem;
    padding-right: 1.5rem;
  }

  .notice {
    bottom: 0.75rem;
    font-size: 0.8125rem;
    padding: 0.625rem 1rem;
  }
}

@media (max-width: 600px) {
  .header-info h2 {
    font-size: 1.125rem;
  }

  .stats {
    font-size: 0.8125rem;
  }

  .body {
    padding: 1rem;
  }

  .treemap-card {
    height: 45%;
    min-height: 200px;
  }

  .treemap-container {
    padding: 0.75rem;
  }

  .chart {
    top: 0.75rem;
    left: 0.75rem;
    right: 0.75rem;
    bottom: 0.75rem;
    width: calc(100% - 1.5rem);
    height: calc(100% - 1.5rem);
  }

  .table-header,
  .table-row {
    grid-template-columns: 1fr 90px;
    gap: 0.75rem;
  }

  .th-files,
  .td-files {
    display: none;
  }
}
</style>
