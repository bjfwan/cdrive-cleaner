<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from 'vue';

const emit = defineEmits<{ close: [] }>();

interface NodeConfig {
  id: string;
  badge: string;
  title: string;
  lead: string;
  bullets: string[];
  footnote?: string;
}

const NODES: NodeConfig[] = [
  {
    id: 'welcome',
    badge: '欢迎',
    title: '把 C 盘从红条救回来',
    lead: '在你按任何按钮之前，先用 30 秒看完 CSD 能帮你做什么、不会做什么。',
    bullets: [
      '能做：找出占空间的大文件、内容重复的副本、Windows 和应用产生的临时垃圾，并帮你搬走或清掉。',
      '能做：把 Steam / Epic / Game Pass 游戏整库搬到其他盘，搬完启动器还认得到。',
      '不会做：在没有你确认的情况下，删除或移动你的任何文件。所有动作都要你点一下「执行」。',
    ],
    footnote: '所有操作都会写进历史，绝大多数都能从「迁移历史」里一键还原。',
  },
  {
    id: 'scan',
    badge: '第一步',
    title: '选一个盘，扫一次',
    lead: '左侧栏选磁盘，右上角点「开始扫描」。一次扫描会做这些事：',
    bullets: [
      '读取目录树并缓存：第一次 30 秒到几分钟（取决于盘大小），之后只读变化部分会快很多。',
      '扫描期间不会动你任何文件——只读不写。',
      '扫完之后，「大文件 / 重复文件 / 系统占用」这些视图才会有数据。',
    ],
    footnote: 'CSD 优先尝试 MFT + USN 高速扫描（需要管理员）；不行就走普通目录遍历，结果一样准。',
  },
  {
    id: 'results',
    badge: '结果',
    title: '扫完会出现几个视图，各管一件事',
    lead: '顶部 Tab 切换。第一次看可能眼花，记住一句话每条：',
    bullets: [
      '「磁盘」：看每个目录占了多少，可下钻、可一键挪。',
      '「游戏库」：列出 Steam / Epic / Game Pass 的所有游戏，勾选 → 加入搬运车 → 搬到其他盘。',
      '「垃圾清理」：扫 Windows 临时目录、浏览器缓存、应用日志等，建议清的项默认已勾。',
    ],
    footnote: '每个视图顶部都有「为什么用这一栏」的灰色按钮，展开后讲清楚什么时候用、会怎么样。',
  },
  {
    id: 'clean',
    badge: '清理',
    title: '清理 / 迁移 / 忽略 — 怎么选',
    lead: '每一项都贴了风险标签，意思如下：',
    bullets: [
      '清理：删除，默认走 Windows 回收站，可还原；勾「永久删除」才会绕过回收站，不可还原。',
      '迁移：文件搬到其他盘，原位置留链接，软件无感知，可在「迁移历史」里回滚。',
      '忽略：CSD 不处理，但下次扫描仍会出现。需要永久跳过请在「设置 → 扫描」里加入跳过路径。',
    ],
    footnote: '风险标签：安全 = 删了无所谓；注意 = 看一眼再删；风险 = 删了可能影响功能；禁清 = 系统保护，CSD 不会让你动。',
  },
  {
    id: 'migrate',
    badge: '迁移',
    title: '链接迁移 = 文件搬家，游戏/软件不用重装',
    lead: 'CSD 默认用「符号链接」做迁移。这意味着：',
    bullets: [
      '文件被真的搬到目标盘，源盘空出空间。',
      '原路径留一个透明的「快捷方式」，程序还是认得到，配置不用改、启动器不用动。',
      '随时可以从「迁移历史」一键还原回原位置。',
    ],
    footnote: '迁移过程中会预先做安全检测：游戏正在运行、文件正被进程占用、目标盘空间不够，CSD 都会拦住。',
  },
];

const STORAGE_KEY_DONE = 'cdrive-cleaner-onboarding-completed';
const STORAGE_KEY_NODE_PREFIX = 'cdrive-cleaner-onboarding-node:';

const currentIndex = ref(0);
const dontShowAgain = ref(false);

const currentNode = computed(() => NODES[currentIndex.value]);
const isLast = computed(() => currentIndex.value === NODES.length - 1);
const isFirst = computed(() => currentIndex.value === 0);

const primaryLabel = computed(() => (isLast.value ? '我准备好了，开始用' : '继续，看下一节'));
const secondaryLabel = computed(() => '跳过整套引导');

function persistNodeFlag(id: string, skipped: boolean) {
  try {
    localStorage.setItem(STORAGE_KEY_NODE_PREFIX + id, skipped ? 'skipped' : 'seen');
  } catch {
    // localStorage unavailable
  }
}

function close(markComplete = true) {
  if (markComplete) {
    try {
      localStorage.setItem(STORAGE_KEY_DONE, 'true');
      localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
    } catch {
      // localStorage unavailable
    }
  }
  emit('close');
}

function next() {
  persistNodeFlag(currentNode.value.id, dontShowAgain.value);
  dontShowAgain.value = false;
  if (isLast.value) {
    close(true);
    return;
  }
  currentIndex.value += 1;
}

function prev() {
  if (isFirst.value) return;
  currentIndex.value -= 1;
}

function skipAll() {
  for (const node of NODES) {
    persistNodeFlag(node.id, true);
  }
  close(true);
}

function jumpTo(index: number) {
  currentIndex.value = index;
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    skipAll();
  } else if (e.key === 'Enter' || e.key === 'ArrowRight') {
    next();
  } else if (e.key === 'ArrowLeft') {
    prev();
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div class="onboard-overlay" @click.self="skipAll">
      <div class="onboard-panel" role="dialog" aria-modal="true">
        <header class="onboard-header">
          <div class="onboard-progress">
            <button
              v-for="(node, i) in NODES"
              :key="node.id"
              class="onboard-step"
              :class="{ active: i === currentIndex, done: i < currentIndex }"
              @click="jumpTo(i)"
              :aria-label="`第 ${i + 1} 节：${node.badge}`"
              type="button"
            >
              <span class="onboard-step-dot">{{ i + 1 }}</span>
              <span class="onboard-step-text">{{ node.badge }}</span>
            </button>
          </div>
          <button class="onboard-skip-x" @click="skipAll" type="button" aria-label="跳过整套引导">✕</button>
        </header>

        <section class="onboard-body">
          <div class="onboard-badge">{{ currentNode.badge }} · {{ currentIndex + 1 }} / {{ NODES.length }}</div>
          <h2 class="onboard-title">{{ currentNode.title }}</h2>
          <p class="onboard-lead">{{ currentNode.lead }}</p>

          <ul class="onboard-list">
            <li v-for="(line, i) in currentNode.bullets" :key="i">
              <span class="onboard-bullet">·</span>
              <span>{{ line }}</span>
            </li>
          </ul>

          <p v-if="currentNode.footnote" class="onboard-footnote">{{ currentNode.footnote }}</p>
        </section>

        <footer class="onboard-footer">
          <label class="onboard-dontshow">
            <input type="checkbox" v-model="dontShowAgain" />
            <span>这一节以后不再提示</span>
          </label>
          <div class="onboard-actions">
            <button
              class="onboard-btn onboard-btn--ghost"
              :disabled="isFirst"
              @click="prev"
              type="button"
            >上一节</button>
            <button
              class="onboard-btn onboard-btn--secondary"
              @click="skipAll"
              type="button"
            >{{ secondaryLabel }}</button>
            <button
              class="onboard-btn onboard-btn--primary"
              @click="next"
              type="button"
            >{{ primaryLabel }}</button>
          </div>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.onboard-overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 32, 0.55);
  backdrop-filter: blur(6px);
  z-index: 9000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  animation: onboard-fade 0.25s ease;
}

@keyframes onboard-fade {
  from { opacity: 0; }
  to { opacity: 1; }
}

.onboard-panel {
  width: min(640px, 100%);
  max-height: 88vh;
  background: var(--color-surface-strong, #fff);
  border: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.08));
  border-radius: 20px;
  box-shadow: var(--shadow-xl, 0 24px 80px rgba(15, 23, 32, 0.4));
  display: flex;
  flex-direction: column;
  overflow: hidden;
  font-family: var(--font-sans, system-ui);
  animation: onboard-rise 0.35s cubic-bezier(0.16, 1, 0.3, 1);
}

@keyframes onboard-rise {
  from {
    opacity: 0;
    transform: translateY(12px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.onboard-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.8rem;
  padding: 0.9rem 1.1rem;
  border-bottom: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.06));
}

.onboard-progress {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex-wrap: wrap;
}

.onboard-step {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.25rem 0.55rem;
  border: 1px solid transparent;
  background: none;
  border-radius: 999px;
  color: var(--color-text-tertiary, #778391);
  font-size: 0.72rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s;
}

.onboard-step:hover {
  color: var(--color-text-primary, #121923);
}

.onboard-step.active {
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.1));
  color: var(--color-highlight, #0f766e);
}

.onboard-step.done {
  color: var(--color-text-secondary, #43505c);
}

.onboard-step-dot {
  display: inline-flex;
  width: 1.1rem;
  height: 1.1rem;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: currentColor;
  color: var(--color-surface-strong, #fff);
  font-size: 0.66rem;
  font-weight: 700;
}

.onboard-step.done .onboard-step-dot::before {
  content: '✓';
  position: absolute;
}

.onboard-step.done .onboard-step-dot {
  font-size: 0;
}

.onboard-skip-x {
  border: none;
  background: none;
  color: var(--color-text-tertiary, #778391);
  cursor: pointer;
  padding: 0.3rem 0.5rem;
  font-size: 0.95rem;
  border-radius: 6px;
  transition: color 0.15s;
}

.onboard-skip-x:hover {
  color: var(--color-text-primary, #121923);
}

.onboard-body {
  padding: 1.4rem 1.6rem 1.2rem;
  overflow-y: auto;
}

.onboard-badge {
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  padding: 0.18rem 0.55rem;
  border-radius: 999px;
  background: var(--color-highlight-soft, rgba(15, 118, 110, 0.1));
  color: var(--color-highlight, #0f766e);
  display: inline-block;
  margin-bottom: 0.75rem;
}

.onboard-title {
  font-size: 1.4rem;
  font-weight: 700;
  color: var(--color-text-primary, #121923);
  line-height: 1.25;
  margin: 0 0 0.6rem;
}

.onboard-lead {
  font-size: 0.92rem;
  color: var(--color-text-secondary, #43505c);
  line-height: 1.55;
  margin: 0 0 1rem;
}

.onboard-list {
  list-style: none;
  padding: 0;
  margin: 0 0 1rem;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.onboard-list li {
  display: grid;
  grid-template-columns: 1rem 1fr;
  gap: 0.4rem;
  font-size: 0.88rem;
  color: var(--color-text-secondary, #43505c);
  line-height: 1.55;
}

.onboard-bullet {
  color: var(--color-highlight, #0f766e);
  font-weight: 700;
}

.onboard-footnote {
  font-size: 0.78rem;
  color: var(--color-text-tertiary, #778391);
  background: var(--color-bg-tertiary, rgba(0, 0, 0, 0.03));
  border-radius: 10px;
  padding: 0.6rem 0.85rem;
  margin: 0;
  line-height: 1.5;
}

.onboard-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.8rem;
  padding: 0.95rem 1.1rem;
  border-top: 1px solid var(--color-border-light, rgba(0, 0, 0, 0.06));
  background: var(--color-bg-tertiary, rgba(0, 0, 0, 0.02));
}

.onboard-dontshow {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.78rem;
  color: var(--color-text-tertiary, #778391);
  cursor: pointer;
  user-select: none;
}

.onboard-actions {
  display: flex;
  gap: 0.45rem;
  align-items: center;
}

.onboard-btn {
  padding: 0.5rem 0.95rem;
  border-radius: 10px;
  border: 1px solid var(--color-border-medium, rgba(0, 0, 0, 0.12));
  background: var(--color-surface-strong, #fff);
  color: var(--color-text-secondary, #43505c);
  font-size: 0.84rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s;
}

.onboard-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  background: var(--color-surface-hover, rgba(0, 0, 0, 0.04));
  color: var(--color-text-primary, #121923);
}

.onboard-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.onboard-btn--ghost {
  border-color: transparent;
}

.onboard-btn--secondary {
  background: transparent;
  border-color: transparent;
  color: var(--color-text-tertiary, #778391);
}

.onboard-btn--primary {
  background: var(--color-highlight, #0f766e);
  color: var(--color-text-inverse, #fff);
  border-color: transparent;
  box-shadow: 0 6px 18px rgba(15, 118, 110, 0.25);
}

.onboard-btn--primary:hover {
  background: var(--color-highlight, #0f766e);
  color: var(--color-text-inverse, #fff);
  box-shadow: 0 10px 24px rgba(15, 118, 110, 0.32);
}

@media (max-width: 640px) {
  .onboard-progress {
    overflow-x: auto;
    flex-wrap: nowrap;
  }
  .onboard-step-text {
    display: none;
  }
  .onboard-footer {
    flex-direction: column;
    align-items: stretch;
    gap: 0.6rem;
  }
  .onboard-actions {
    justify-content: flex-end;
  }
}

[data-theme='dark'] .onboard-panel {
  background: var(--color-surface-strong, #1c1f29);
}

[data-theme='dark'] .onboard-footer {
  background: rgba(255, 255, 255, 0.03);
}

[data-theme='dark'] .onboard-footnote {
  background: rgba(255, 255, 255, 0.04);
}
</style>
