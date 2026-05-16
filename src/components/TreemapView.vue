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
  navigate: [path: string];
}>();

const colors = [
  '#18242f',
  '#28566a',
  '#0f766e',
  '#2563eb',
  '#b7791f',
  '#37515f',
  '#5d9c8f',
  '#5c6f9e',
  '#bf8a42',
  '#48525d',
];

const sortedDirectories = computed(() => [...props.directories].sort((a, b) => b.size - a.size));
const treemapData = computed(() =>
  sortedDirectories.value.map((dir, index) => ({
    name: dir.name,
    value: dir.size,
    path: dir.path,
    hasChildren: dir.has_children,
    itemStyle: {
      color: colors[index % colors.length],
      opacity: props.deepScanning && !dir.has_children ? 0.62 : 0.94,
    },
  })),
);

const treemapOption = computed(() => {
  return {
    tooltip: {
      formatter: (info: { name: string; value: number; data: { hasChildren: boolean } }) => {
        const status = props.deepScanning && !info.data.hasChildren ? '<br/>(扫描更新中)' : '';
        return `${info.name}<br/>${formatBytes(info.value)}${status}`;
      },
      backgroundColor: 'rgba(255, 255, 255, 0.96)',
      borderColor: 'rgba(46, 33, 18, 0.08)',
      borderWidth: 1,
      textStyle: {
        color: '#161616',
        fontSize: 13,
        fontFamily: 'Manrope, Microsoft YaHei UI, sans-serif',
      },
      padding: [10, 14],
      borderRadius: 14,
      shadowBlur: 18,
      shadowColor: 'rgba(18, 18, 18, 0.12)',
    },
    series: [
      {
        type: 'treemap',
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
        squareRatio: 0.82,
        visibleMin: 120,
        data: treemapData.value,
        roam: false,
        nodeClick: 'link',
        breadcrumb: { show: false },
        label: {
          show: true,
          formatter: (params: { name: string; width?: number; height?: number }) => {
            const w = params.width ?? 0;
            const h = params.height ?? 0;
            const area = w * h;
            // 块太小不渲染文字，避免视觉噪声
            if (area < 1800 || w < 36 || h < 22) return '';
            return params.name;
          },
          fontSize: 12,
          color: 'rgba(255, 255, 255, 0.96)',
          fontWeight: 700,
          fontFamily: 'Manrope, Microsoft YaHei UI, sans-serif',
          overflow: 'truncate',
          ellipsis: '…',
          padding: [4, 8],
          backgroundColor: 'rgba(23, 23, 23, 0.18)',
          borderRadius: 10,
        },
        upperLabel: {
          show: true,
          height: 28,
          color: 'rgba(255, 255, 255, 0.98)',
          fontSize: 12,
          fontWeight: 700,
          fontFamily: 'Manrope, Microsoft YaHei UI, sans-serif',
        },
        itemStyle: {
          borderColor: 'rgba(255, 255, 255, 0.32)',
          borderWidth: 2,
          borderRadius: 16,
          gapWidth: 4,
          shadowBlur: 10,
          shadowColor: 'rgba(18, 18, 18, 0.08)',
        },
        emphasis: {
          itemStyle: {
            shadowBlur: 18,
            shadowColor: 'rgba(18, 18, 18, 0.2)',
            borderWidth: 3,
            borderColor: 'rgba(255, 255, 255, 0.52)',
          },
        },
        levels: [
          {
            itemStyle: {
              borderWidth: 0,
              borderRadius: 18,
              gapWidth: 4,
            },
          },
        ],
      },
    ],
  };
});

function handleChartClick(params: Record<string, unknown>) {
  const data = params.data as { path?: string } | undefined;
  if (!data?.path || !props.hasDeepScanned) {
    return;
  }

  emit('navigate', data.path);
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
    <section class="treemap-card">
      <div class="panel-header">
        <div>
          <h3>空间热区</h3>
          <p>用面积直接比较目录体积，快速找出最值得处理的区域。</p>
        </div>
        <span class="panel-chip">{{ hasDeepScanned ? '可深入导航' : '概览模式' }}</span>
      </div>

      <div v-if="treemapData.length > 0" class="treemap-container">
        <v-chart :option="treemapOption" class="chart" autoresize @click="handleChartClick" />
      </div>
      <div v-else class="treemap-empty">
        <h4>当前层级没有子目录</h4>
        <p>切换到列表或大文件视图查看直接文件。</p>
      </div>
    </section>

    <aside class="list-preview">
      <div class="panel-header panel-header-side">
        <div>
          <h3>目录排行</h3>
          <p>右侧保留最重要的可读性视图，方便快速判断。</p>
        </div>
      </div>

      <div v-if="sortedDirectories.length > 0" class="preview-items">
        <div
          v-for="(dir, index) in sortedDirectories.slice(0, 8)"
          :key="dir.path"
          class="preview-item"
          :class="{ disabled: !hasDeepScanned }"
          @click="handleItemClick(dir)"
        >
          <div class="preview-rank">{{ index + 1 }}</div>

          <div class="preview-info">
            <div class="preview-name-row">
              <div class="preview-name">{{ dir.name }}</div>
              <div class="preview-percent">{{ totalSize ? ((dir.size / totalSize) * 100).toFixed(1) : '0.0' }}%</div>
            </div>

            <div class="preview-bar-container">
              <div class="preview-bar" :style="{ width: `${totalSize ? (dir.size / totalSize) * 100 : 0}%` }"></div>
            </div>
          </div>

          <div class="preview-size">{{ formatBytes(dir.size) }}</div>
        </div>
      </div>
      <div v-else class="preview-empty">暂无可排行目录</div>
    </aside>
  </div>
</template>

<style scoped>
.treemap-view {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 21rem;
  gap: 1rem;
  flex: 1;
  min-height: 0;
}

.treemap-card,
.list-preview {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  border-radius: var(--radius-lg);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.76), rgba(247, 241, 232, 0.88));
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xs);
}

.panel-header {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 1rem;
  padding: 1rem 1rem 0;
}

.panel-header h3 {
  font-size: 1.2rem;
  margin-bottom: 0.22rem;
}

.panel-header p {
  font-size: 0.82rem;
  line-height: 1.6;
  color: var(--color-text-tertiary);
}

.panel-chip {
  padding: 0.45rem 0.7rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.06);
  color: var(--color-text-secondary);
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.treemap-container {
  position: relative;
  flex: 1;
  min-height: 20rem;
  padding: 1rem;
}

.chart {
  width: 100%;
  height: 100%;
}

.treemap-empty,
.preview-empty {
  flex: 1;
  min-height: 16rem;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 0.35rem;
  padding: 1rem;
  color: var(--color-text-tertiary);
}

.treemap-empty h4 {
  font-size: 1rem;
  color: var(--color-text-primary);
}

.treemap-empty p,
.preview-empty {
  font-size: 0.84rem;
  line-height: 1.55;
}

.panel-header-side {
  padding-bottom: 0.4rem;
}

.preview-items {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  overflow-y: auto;
  padding: 0 1rem 1rem;
}

.preview-item {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 0.8rem;
  padding: 0.9rem;
  border-radius: var(--radius-md);
  background: rgba(255, 255, 255, 0.64);
  border: 1px solid rgba(46, 33, 18, 0.08);
  cursor: pointer;
  transition: transform var(--transition-base), box-shadow var(--transition-base), background var(--transition-base);
}

.preview-item:hover {
  transform: translateX(2px);
  background: var(--color-surface-hover);
  box-shadow: var(--shadow-sm);
}

.preview-item.disabled {
  cursor: not-allowed;
}

.preview-item.disabled:hover {
  transform: none;
}

.preview-rank {
  width: 2rem;
  height: 2rem;
  border-radius: 0.8rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(23, 23, 23, 0.06);
  font-weight: 800;
  color: var(--color-text-secondary);
}

.preview-info {
  min-width: 0;
}

.preview-name-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.7rem;
  margin-bottom: 0.45rem;
}

.preview-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.88rem;
  font-weight: 700;
  color: var(--color-text-primary);
}

.preview-percent {
  font-size: 0.74rem;
  font-weight: 800;
  color: var(--color-text-tertiary);
}

.preview-bar-container {
  height: 0.38rem;
  border-radius: var(--radius-pill);
  background: rgba(23, 23, 23, 0.08);
  overflow: hidden;
}

.preview-bar {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, rgba(15, 118, 110, 0.9), rgba(37, 99, 235, 0.72));
  transition: width var(--transition-slow);
}

.preview-size {
  font-size: 0.8rem;
  font-weight: 800;
  color: var(--color-text-secondary);
}

@media (max-width: 960px) {
  .treemap-view {
    grid-template-columns: 1fr;
  }

  .list-preview {
    min-height: 20rem;
  }
}

@media (max-width: 640px) {
  .panel-header,
  .preview-items,
  .treemap-container {
    padding-left: 0.8rem;
    padding-right: 0.8rem;
  }

  .preview-item {
    grid-template-columns: auto minmax(0, 1fr);
  }

  .preview-size {
    grid-column: 2;
  }
}
</style>
