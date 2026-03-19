<script setup lang="ts">
import { computed } from 'vue';
import { use } from 'echarts/core';
import { TreemapChart } from 'echarts/charts';
import { TitleComponent, TooltipComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import VChart from 'vue-echarts';
import type { DirectoryNode } from '../types';
import { formatBytes } from '../utils/format';

use([CanvasRenderer, TreemapChart, TitleComponent, TooltipComponent]);

interface Props {
  directories: DirectoryNode[];
  totalSize: number;
  deepScanning: boolean;
  hasDeepScanned: boolean;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  'navigate': [path: string];
}>();

const treemapOption = computed(() => {
  // 温暖、优雅的色调，与整体设计一致
  const colors = [
    '#8b7355', '#a88d5f', '#c9a66b', '#d4a574', '#e8b86d',
    '#b8956a', '#9d8264', '#c4a57b', '#dbb98a', '#f0d5a8',
    '#8b6f47', '#a68a5c'
  ];

  return {
    tooltip: {
      formatter: (info: { name: string; value: number; data: { hasChildren: boolean } }) => {
        const status = props.deepScanning && !info.data.hasChildren ? '<br/>(扫描中...)' : '';
        return `${info.name}<br/>${formatBytes(info.value)}${status}`;
      },
      backgroundColor: 'rgba(255, 252, 245, 0.98)',
      borderColor: 'rgba(139, 92, 46, 0.15)',
      borderWidth: 1,
      textStyle: {
        color: '#2d2d2d',
        fontSize: 13,
        fontFamily: 'Inter, -apple-system, sans-serif'
      },
      padding: [10, 14],
      borderRadius: 10,
      shadowBlur: 12,
      shadowColor: 'rgba(139, 92, 46, 0.12)'
    },
    series: [{
      type: 'treemap',
      left: 0,
      right: 0,
      top: 0,
      bottom: 0,
      squareRatio: 0.7,
      leafDepth: 1,
      visibleMin: 120,
      data: props.directories.map((dir, index) => ({
        name: dir.name,
        value: dir.size,
        path: dir.path,
        hasChildren: dir.has_children,
        itemStyle: {
          color: colors[index % colors.length],
          opacity: props.deepScanning && !dir.has_children ? 0.6 : 0.92
        }
      })),
      roam: false,
      nodeClick: 'link',
      breadcrumb: { show: false },
      label: {
        show: true,
        formatter: (params: { name: string; width: number; height: number }) => {
          const area = params.width * params.height;
          if (area < 2000) return '';
          if (area < 5000) return params.name.substring(0, 10);
          return params.name;
        },
        fontSize: 13,
        color: 'rgba(255, 255, 255, 0.95)',
        fontWeight: 600,
        fontFamily: 'Inter, -apple-system, sans-serif',
        overflow: 'truncate',
        padding: [4, 8],
        backgroundColor: 'rgba(45, 35, 25, 0.15)',
        borderRadius: 6
      },
      upperLabel: {
        show: true,
        height: 32,
        color: 'rgba(255, 255, 255, 0.98)',
        fontSize: 13,
        fontWeight: 600,
        fontFamily: 'Inter, -apple-system, sans-serif'
      },
      itemStyle: {
        borderColor: 'rgba(255, 252, 245, 0.4)',
        borderWidth: 2,
        borderRadius: 12,
        gapWidth: 3,
        shadowBlur: 8,
        shadowColor: 'rgba(139, 92, 46, 0.1)'
      },
      emphasis: {
        itemStyle: {
          shadowBlur: 16,
          shadowColor: 'rgba(139, 92, 46, 0.25)',
          borderWidth: 3,
          borderColor: 'rgba(255, 252, 245, 0.6)'
        }
      },
      levels: [
        {
          itemStyle: {
            borderWidth: 0,
            borderRadius: 14,
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

function handleChartClick(params: Record<string, unknown>) {
  const data = params.data as { path?: string } | undefined;
  if (data?.path) {
    if (!props.hasDeepScanned) {
      return;
    }
    emit('navigate', data.path);
  }
}

function handleItemClick(dir: DirectoryNode) {
  if (!props.hasDeepScanned) {
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
  grid-template-columns: 1fr 340px;
  gap: 1.25rem;
  overflow: hidden;
}

.treemap-card {
  background: linear-gradient(to bottom, var(--color-bg-primary) 0%, var(--color-bg-secondary) 100%);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-lg);
  overflow: hidden;
  box-shadow: var(--shadow-sm);
  min-height: 280px;
  transition: all var(--transition-base);
}

.treemap-card:hover {
  box-shadow: var(--shadow-md);
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
  font-family: var(--font-serif);
  font-size: 1rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 1rem;
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
  background: rgba(139, 92, 46, 0.15);
  border-radius: 3px;
}

.preview-items::-webkit-scrollbar-thumb:hover {
  background: rgba(139, 92, 46, 0.25);
}

.preview-item {
  display: flex;
  align-items: center;
  gap: 0.875rem;
  padding: 0.875rem 1rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-base);
}

.preview-item:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-medium);
  box-shadow: var(--shadow-md);
  transform: translateX(4px);
}

.preview-rank {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, rgba(139, 115, 85, 0.08) 0%, rgba(139, 115, 85, 0.12) 100%);
  border-radius: 8px;
  font-size: 0.8125rem;
  font-weight: 700;
  color: var(--color-accent-primary);
  flex-shrink: 0;
  font-family: var(--font-serif);
}

.preview-info {
  flex: 1;
  min-width: 0;
}

.preview-name {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--color-text-primary);
  margin-bottom: 0.375rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  letter-spacing: -0.01em;
}

.preview-bar-container {
  height: 4px;
  background: rgba(139, 92, 46, 0.08);
  border-radius: 2px;
  overflow: hidden;
}

.preview-bar {
  height: 100%;
  background: linear-gradient(90deg, var(--color-accent-primary) 0%, var(--color-accent-secondary) 100%);
  border-radius: 2px;
  transition: width 0.4s cubic-bezier(0.16, 1, 0.3, 1);
  box-shadow: 0 0 8px rgba(139, 115, 85, 0.3);
}

.preview-size {
  font-size: 0.8125rem;
  font-weight: 700;
  color: var(--color-text-primary);
  flex-shrink: 0;
  font-family: var(--font-serif);
}

@media (max-width: 900px) {
  .treemap-view {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr;
    gap: 1rem;
  }

  .treemap-card {
    min-height: 260px;
  }

  .list-preview {
    display: none;
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
    gap: 0.875rem;
  }

  .treemap-card {
    min-height: 220px;
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
