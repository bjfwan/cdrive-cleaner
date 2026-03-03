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
  grid-template-columns: 1fr 380px;
  gap: 2rem;
  overflow: hidden;
}

.treemap-card {
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 16px;
  overflow: hidden;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  min-height: 400px;
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

.list-preview {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.preview-title {
  font-size: 1rem;
  font-weight: 600;
  color: #2c2c2c;
  margin-bottom: 1rem;
  letter-spacing: -0.01em;
}

.preview-items {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.preview-item {
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 0.875rem;
  background: white;
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.preview-item:hover {
  border-color: #007aff;
  box-shadow: 0 2px 8px rgba(0, 122, 255, 0.1);
  transform: translateX(4px);
}

.preview-rank {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f5f5f4;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 600;
  color: #78716c;
  flex-shrink: 0;
}

.preview-info {
  flex: 1;
  min-width: 0;
}

.preview-name {
  font-size: 0.875rem;
  font-weight: 500;
  color: #2c2c2c;
  margin-bottom: 0.375rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-bar-container {
  height: 4px;
  background: #f5f5f4;
  border-radius: 2px;
  overflow: hidden;
}

.preview-bar {
  height: 100%;
  background: linear-gradient(90deg, #007aff 0%, #5856d6 100%);
  border-radius: 2px;
  transition: width 0.3s ease;
}

.preview-size {
  font-size: 0.875rem;
  font-weight: 600;
  color: #2c2c2c;
  flex-shrink: 0;
}

@media (max-width: 900px) {
  .treemap-view {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr auto;
    gap: 1.5rem;
  }

  .treemap-card {
    min-height: 350px;
  }

  .list-preview {
    max-height: 300px;
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
}

@media (max-width: 600px) {
  .treemap-view {
    gap: 1rem;
  }

  .treemap-card {
    min-height: 250px;
  }

  .list-preview {
    max-height: 250px;
  }

  .preview-item {
    padding: 0.75rem;
    gap: 0.75rem;
  }

  .preview-rank {
    width: 24px;
    height: 24px;
    font-size: 0.8125rem;
  }

  .preview-name {
    font-size: 0.8125rem;
  }

  .preview-size {
    font-size: 0.8125rem;
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
