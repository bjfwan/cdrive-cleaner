<script setup lang="ts">
import { computed } from 'vue';
import VChart from 'vue-echarts';

interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  children: DirectoryNode[];
}

interface Props {
  directories: DirectoryNode[];
  totalSize: number;
  deepScanning: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'navigate': [path: string];
}>();

const treemapOption = computed(() => {
  const colors = [
    '#007aff', '#5856d6', '#af52de', '#ff2d55', 
    '#ff3b30', '#ff9500', '#ffcc00', '#34c759',
    '#00c7be', '#30b0c7', '#32ade6'
  ];

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
      data: props.directories.map((dir, index) => ({
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
        height: 28,
        color: '#fff',
        fontSize: 12,
        fontWeight: 600
      },
      itemStyle: {
        borderColor: '#fff',
        borderWidth: 2,
        borderRadius: 8,
        gapWidth: 2,
        shadowBlur: 6,
        shadowColor: 'rgba(0, 0, 0, 0.08)'
      },
      emphasis: {
        itemStyle: {
          shadowBlur: 10,
          shadowColor: 'rgba(0, 0, 0, 0.15)',
          borderWidth: 2
        }
      },
      levels: [
        {
          itemStyle: {
            borderWidth: 0,
            borderRadius: 12,
            gapWidth: 4
          }
        }
      ]
    }]
  };
});

const sortedDirectories = computed(() => {
  return [...props.directories].sort((a, b) => b.size - a.size);
});

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function handleChartClick(params: any) {
  if (params.data && params.data.path) {
    if (props.deepScanning && params.data.hasChildren === false) {
      return;
    }
    emit('navigate', params.data.path);
  }
}

function handleItemClick(dir: DirectoryNode) {
  if (props.deepScanning && (!dir.children || dir.children.length === 0)) {
    return;
  }
  emit('navigate', dir.path);
}
</script>

<template>
  <div class="treemap-view">
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
    
    <div class="list-preview">
      <h3 class="preview-title">最大的目录</h3>
      <div class="preview-items">
        <div 
          v-for="(dir, index) in sortedDirectories.slice(0, 10)" 
          :key="dir.path"
          class="preview-item"
          @click="handleItemClick(dir)"
        >
          <div class="preview-rank">{{ index + 1 }}</div>
          <div class="preview-info">
            <div class="preview-name">{{ dir.name }}</div>
            <div class="preview-bar-container">
              <div 
                class="preview-bar" 
                :style="{ width: `${(dir.size / totalSize) * 100}%` }"
              ></div>
            </div>
          </div>
          <div class="preview-size">{{ formatBytes(dir.size) }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.treemap-view {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 360px;
  gap: 1.5rem;
  overflow: hidden;
}

.treemap-card {
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 14px;
  overflow: hidden;
  box-shadow: 0 1px 6px rgba(0, 0, 0, 0.04);
  min-height: 320px;
}

.treemap-container {
  width: 100%;
  height: 100%;
  position: relative;
  padding: 1rem;
}

.chart {
  position: absolute;
  top: 1rem;
  left: 1rem;
  right: 1rem;
  bottom: 1rem;
  width: calc(100% - 2rem);
  height: calc(100% - 2rem);
}

.list-preview {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.preview-title {
  font-size: 0.9375rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 0.875rem;
  letter-spacing: -0.01em;
}

.preview-items {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  padding-right: 0.25rem;
}

.preview-items::-webkit-scrollbar {
  width: 6px;
}

.preview-items::-webkit-scrollbar-track {
  background: transparent;
}

.preview-items::-webkit-scrollbar-thumb {
  background: #d4d4d8;
  border-radius: 3px;
}

.preview-items::-webkit-scrollbar-thumb:hover {
  background: #a1a1aa;
}

.preview-item {
  display: flex;
  align-items: center;
  gap: 0.875rem;
  padding: 0.75rem;
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.preview-item:hover {
  border-color: #007aff;
  box-shadow: 0 2px 6px rgba(0, 122, 255, 0.08);
  transform: translateX(3px);
}

.preview-rank {
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border-radius: 6px;
  font-size: 0.8125rem;
  font-weight: 600;
  color: #78716c;
  flex-shrink: 0;
}

.preview-info {
  flex: 1;
  min-width: 0;
}

.preview-name {
  font-size: 0.8125rem;
  font-weight: 500;
  color: #2c2c2c;
  margin-bottom: 0.3125rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-bar-container {
  height: 3px;
  background: #f5f5f4;
  border-radius: 1.5px;
  overflow: hidden;
}

.preview-bar {
  height: 100%;
  background: linear-gradient(90deg, #007aff 0%, #5856d6 100%);
  border-radius: 1.5px;
  transition: width 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.preview-size {
  font-size: 0.8125rem;
  font-weight: 600;
  color: #2c2c2c;
  flex-shrink: 0;
}

@media (max-width: 900px) {
  .treemap-view {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr auto;
    gap: 1.25rem;
  }

  .treemap-card {
    min-height: 280px;
  }

  .list-preview {
    min-height: auto;
  }

  .preview-items {
    flex-direction: row;
    overflow-x: auto;
    overflow-y: hidden;
    padding-right: 0;
    padding-bottom: 0.5rem;
    gap: 0.75rem;
  }

  .preview-items::-webkit-scrollbar {
    height: 6px;
    width: auto;
  }

  .preview-item {
    flex-direction: column;
    min-width: 140px;
    max-width: 160px;
    flex-shrink: 0;
    padding: 0.875rem;
    gap: 0.625rem;
  }

  .preview-rank {
    width: 100%;
    height: auto;
    padding: 0.375rem;
  }

  .preview-info {
    width: 100%;
    text-align: center;
  }

  .preview-name {
    text-align: center;
    margin-bottom: 0.5rem;
  }

  .preview-size {
    width: 100%;
    text-align: center;
    font-size: 0.875rem;
  }

  .preview-bar-container {
    width: 100%;
  }

  .treemap-container {
    padding: 0.875rem;
  }

  .chart {
    top: 0.875rem;
    left: 0.875rem;
    right: 0.875rem;
    bottom: 0.875rem;
    width: calc(100% - 1.75rem);
    height: calc(100% - 1.75rem);
  }
}

@media (max-width: 600px) {
  .treemap-view {
    grid-template-rows: 1fr auto;
    gap: 1rem;
  }

  .treemap-card {
    min-height: 220px;
  }

  .preview-item {
    min-width: 120px;
    max-width: 140px;
    padding: 0.75rem;
  }

  .preview-rank {
    font-size: 0.75rem;
  }

  .preview-name {
    font-size: 0.75rem;
  }

  .preview-size {
    font-size: 0.75rem;
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
}
</style>
