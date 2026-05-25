<script setup lang="ts">
import { computed } from 'vue';
import { IconClose, IconShield, IconInfo, IconWarning } from './icons';
import { aiStore } from '../store/ai';

interface Props {
  show: boolean;
}

defineProps<Props>();
const emit = defineEmits<{ 'close': [] }>();

const state = aiStore.state;

const modeLabel = computed(() =>
  state.settings.mode === 'byok'
    ? `BYOK · ${state.settings.byok.baseUrl || '未填写'}`
    : `Builtin · ${state.settings.builtinModel}`,
);

const enabledCategories = computed(() => {
  const cats = state.settings.categories;
  const labels: string[] = [];
  if (cats.disks) labels.push('磁盘容量与剩余空间');
  if (cats.largeFiles) labels.push('大文件列表（已脱敏）');
  if (cats.categories) labels.push('文件类别统计');
  if (cats.duplicates) labels.push('重复文件');
  return labels;
});
</script>

<template>
  <div v-if="show" class="overlay" @click.self="emit('close')">
    <div class="privacy-panel" @click.stop>
      <header class="privacy-header">
        <div class="privacy-header-title">
          <IconShield :size="22" />
          <h2>隐私政策</h2>
        </div>
        <button class="close-btn" @click="emit('close')">
          <IconClose :size="20" />
        </button>
      </header>

      <div class="privacy-body">
        <section class="privacy-section">
          <h3>当前你的设置</h3>
          <ul class="privacy-state">
            <li>
              <span>AI 分析</span>
              <strong>{{ state.settings.enabled ? '已启用' : '未启用' }}</strong>
            </li>
            <li>
              <span>调用模式</span>
              <strong>{{ modeLabel }}</strong>
            </li>
            <li>
              <span>已勾选的数据类别</span>
              <strong>{{ enabledCategories.length === 0 ? '无' : enabledCategories.join('、') }}</strong>
            </li>
          </ul>
        </section>

        <section class="privacy-section">
          <h3>1. 哪些数据可能被发送</h3>
          <p>只有当你 <strong>同时</strong> 开启「启用 AI 分析」并在「数据类别」面板里勾选某一类时，该类数据才会被打包进发送给 AI 的请求。</p>
          <ul>
            <li><strong>磁盘容量与剩余空间</strong>：每个盘符的容量与可用空间（MB），不包含分区标签以外的文件信息。</li>
            <li><strong>大文件列表（已脱敏）</strong>：仅发送大小、扩展名以及脱敏 token（不会包含真实路径或文件名）。</li>
            <li><strong>文件类别统计</strong>：按类别聚合的总大小与文件数量，不包含具体文件。</li>
            <li><strong>重复文件</strong>：重复组的总大小与文件数量，路径已脱敏为内部 token。</li>
          </ul>
          <p>所有路径会在本地脱敏后再发出：<code>C:\Users\&lt;name&gt;\…</code> → <code>~user~/…</code>，真实路径仅保留在本地反查表中，<strong>不会</strong>发送到任何服务端。</p>
        </section>

        <section class="privacy-section">
          <h3>2. 数据会发给谁</h3>
          <ul>
            <li><strong>Builtin 模式（免费，每天 50 次）</strong>：请求经我们部署的 Cloudflare Worker 代理 <code>csd-api.likeyou.qzz.io</code>，再转发至上游模型服务商 <code>agentrouter.org</code>。每日额度按调用方 IP 计算。</li>
            <li><strong>BYOK 模式</strong>：请求 <strong>直连</strong>你在设置中填写的 Base URL（OpenAI 兼容服务），客户端 <strong>不会</strong>把请求经过我们的 Worker。</li>
          </ul>
          <div class="privacy-callout">
            <IconInfo :size="18" />
            <span>客户端二进制内不会内嵌任何上游服务商的 API Key；Builtin 模式所需的 Key 由 Worker 在服务端注入。</span>
          </div>
        </section>

        <section class="privacy-section">
          <h3>3. 数据保留策略</h3>
          <ul>
            <li>本地：扫描结果、AI 建议、设置项均存储在本机，<strong>不会</strong>上传到我们的服务器。</li>
            <li>Worker：仅记录调用次数（用于配额）。我们 <strong>不会</strong>记录 prompt 内容；Cloudflare 平台日志按其默认策略保留。</li>
            <li>上游服务商：请参考你选定的服务商（agentrouter 或 BYOK 提供方）的隐私政策。</li>
          </ul>
        </section>

        <section class="privacy-section">
          <h3>4. 「发送前预览」</h3>
          <p>在「设置 → AI 服务」中，你可以随时点击「发送前预览」查看即将随下一次请求发出的完整 JSON 数据。<strong>没有勾选的数据类别一定不会出现在这个预览里</strong>。</p>
        </section>

        <section class="privacy-section">
          <h3>5. 你的撤回权利</h3>
          <ul>
            <li>关掉「启用 AI 分析」总开关后，AI 模块立即停止任何外发请求。</li>
            <li>取消勾选任一数据类别后，该类别的数据将不再出现在后续请求中。</li>
            <li>切换为 BYOK 模式可让请求完全跳过我们的 Worker。</li>
            <li>所有 AI 建议都需要你 <strong>手动点击「采纳」</strong>才会触发迁移对话框 —— 我们 <strong>永远不会</strong>自动执行任何文件操作。</li>
          </ul>
        </section>

        <section class="privacy-section">
          <h3>6. 联系方式</h3>
          <p>如果你对本政策有疑问、想行使「停止处理」或「数据导出」的权利，请通过 GitHub Issues 联系我们：<code>cdrive-cleaner</code> 项目仓库的 Issue 区。</p>
        </section>

        <div class="privacy-callout privacy-callout--warning">
          <IconWarning :size="18" />
          <span>本政策可能随版本迭代更新；如重大变更，会在更新后第一次开启「启用 AI 分析」时再次请你确认。</span>
        </div>
      </div>

      <footer class="privacy-footer">
        <button class="btn btn-primary" @click="emit('close')">我已知悉</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 18, 28, 0.45);
  backdrop-filter: blur(6px);
  z-index: 1500;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.privacy-panel {
  width: min(720px, 100%);
  max-height: 90vh;
  background: var(--surface, #ffffff);
  border-radius: 18px;
  box-shadow: 0 24px 60px rgba(0, 0, 0, 0.28);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.privacy-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 18px 24px;
  border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.08));
}

.privacy-header-title {
  display: inline-flex;
  align-items: center;
  gap: 10px;
}

.privacy-header h2 {
  margin: 0;
  font-size: 18px;
  color: var(--text-primary, #111827);
}

.close-btn {
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 6px;
  border-radius: 8px;
  color: var(--text-secondary, #6b7280);
}

.close-btn:hover {
  background: rgba(0, 0, 0, 0.05);
}

.privacy-body {
  overflow-y: auto;
  padding: 20px 24px 8px;
  display: flex;
  flex-direction: column;
  gap: 22px;
}

.privacy-section h3 {
  margin: 0 0 8px;
  font-size: 15px;
  color: var(--text-primary, #111827);
}

.privacy-section p,
.privacy-section ul {
  margin: 0 0 6px;
  font-size: 13.5px;
  line-height: 1.7;
  color: var(--text-secondary, #4b5563);
}

.privacy-section ul {
  padding-left: 22px;
}

.privacy-section li {
  margin-bottom: 4px;
}

.privacy-section code {
  background: rgba(15, 23, 42, 0.06);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 12px;
}

.privacy-state {
  display: flex;
  flex-direction: column;
  gap: 6px;
  list-style: none;
  padding: 0;
  border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  overflow: hidden;
  margin-bottom: 4px;
}

.privacy-state li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 14px;
  font-size: 13px;
  border-bottom: 1px solid var(--border, rgba(0, 0, 0, 0.05));
}

.privacy-state li:last-child {
  border-bottom: none;
}

.privacy-state strong {
  color: var(--text-primary, #111827);
}

.privacy-callout {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  padding: 10px 12px;
  background: rgba(47, 109, 246, 0.06);
  border-radius: 10px;
  color: var(--text-secondary, #4b5563);
  font-size: 13px;
  line-height: 1.6;
}

.privacy-callout--warning {
  background: rgba(245, 158, 11, 0.08);
}

.privacy-footer {
  padding: 14px 24px 18px;
  display: flex;
  justify-content: flex-end;
  border-top: 1px solid var(--border, rgba(0, 0, 0, 0.06));
}

.btn {
  display: inline-flex;
  align-items: center;
  padding: 8px 18px;
  border-radius: 8px;
  border: 1px solid transparent;
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  transition: background 0.15s ease;
}

.btn-primary {
  background: #2f6df6;
  color: #ffffff;
}

.btn-primary:hover {
  background: #2257d8;
}

[data-theme="dark"] .privacy-panel {
  background: #1c1f29;
}

[data-theme="dark"] .privacy-header,
[data-theme="dark"] .privacy-footer {
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .privacy-header h2,
[data-theme="dark"] .privacy-section h3,
[data-theme="dark"] .privacy-state strong {
  color: #f5f5f7;
}

[data-theme="dark"] .privacy-section p,
[data-theme="dark"] .privacy-section ul,
[data-theme="dark"] .privacy-callout,
[data-theme="dark"] .privacy-state li {
  color: #aab2c0;
}

[data-theme="dark"] .privacy-state {
  border-color: rgba(255, 255, 255, 0.08);
}

[data-theme="dark"] .privacy-state li {
  border-color: rgba(255, 255, 255, 0.05);
}

[data-theme="dark"] .privacy-section code {
  background: rgba(255, 255, 255, 0.08);
  color: #f5f5f7;
}

[data-theme="dark"] .close-btn {
  color: #cdd3df;
}

[data-theme="dark"] .close-btn:hover {
  background: rgba(255, 255, 255, 0.06);
}

[data-theme="dark"] .privacy-callout {
  background: rgba(47, 109, 246, 0.16);
}

[data-theme="dark"] .privacy-callout--warning {
  background: rgba(245, 158, 11, 0.18);
}
</style>
