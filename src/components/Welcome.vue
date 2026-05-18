<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import appIcon from '../assets/app-icon.png';

const emit = defineEmits<{ close: [] }>();

const step = ref(1);
const direction = ref<'forward' | 'backward'>('forward');
const totalSteps = 4;
const isElevated = ref(false);
const cardRef = ref<HTMLDivElement | null>(null);
const primaryButtonRef = ref<HTMLButtonElement | null>(null);

const canBack = computed(() => step.value > 1);
const canForward = computed(() => step.value < totalSteps);
const isLast = computed(() => step.value === totalSteps);

onMounted(async () => {
  document.addEventListener('keydown', onKey);
  try {
    isElevated.value = await invoke<boolean>('is_elevated');
  } catch {
    isElevated.value = false;
  }
  focusPrimary();
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKey);
});

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    skip();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    if (isLast.value) {
      finish();
    } else {
      next();
    }
  }
}

function next() {
  if (canForward.value) {
    direction.value = 'forward';
    step.value += 1;
    focusPrimary();
  }
}

function prev() {
  if (canBack.value) {
    direction.value = 'backward';
    step.value -= 1;
    focusPrimary();
  }
}

function skip() {
  finish();
}

function finish() {
  localStorage.setItem('cdrive-cleaner-onboarding-completed', 'true');
  localStorage.setItem('cdrive-cleaner-welcome-shown', 'true');
  emit('close');
}

async function enableAdmin() {
  try {
    await invoke('request_admin_rescan', { disk: 'C:\\' });
  } catch {
    next();
  }
}

function focusPrimary() {
  requestAnimationFrame(() => {
    primaryButtonRef.value?.focus();
  });
}
</script>

<template>
  <div class="onboarding-overlay" @click.self="skip">
    <div class="onboarding-card" ref="cardRef" role="dialog" aria-modal="true" aria-label="欢迎引导">
      <button class="onboarding-skip" @click="skip" aria-label="跳过">跳过</button>

      <div class="onboarding-stage" :data-direction="direction">
        <transition :name="direction === 'forward' ? 'slide-forward' : 'slide-backward'" mode="out-in">
          <section v-if="step === 1" :key="1" class="step">
            <img :src="appIcon" alt="" class="step-logo" />
            <h2 class="step-title">为 C 盘瘦身</h2>
            <p class="step-sub">三件事，一个工具</p>
            <ul class="value-list">
              <li>
                <span class="value-icon">🔍</span>
                <div>
                  <strong>智能扫描</strong>
                  <small>找出能搬走和能删掉的</small>
                </div>
              </li>
              <li>
                <span class="value-icon">🚚</span>
                <div>
                  <strong>搬到其他盘还能用</strong>
                  <small>用 Junction 链接，应用照常运行</small>
                </div>
              </li>
              <li>
                <span class="value-icon">↩️</span>
                <div>
                  <strong>每一步都可回滚</strong>
                  <small>历史里随时撤销</small>
                </div>
              </li>
            </ul>
          </section>

          <section v-else-if="step === 2" :key="2" class="step">
            <div class="junction-illustration" aria-hidden="true">
              <svg viewBox="0 0 360 140" preserveAspectRatio="xMidYMid meet">
                <defs>
                  <linearGradient id="junction-c" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0%" stop-color="#1f5658" />
                    <stop offset="100%" stop-color="#2b7b73" />
                  </linearGradient>
                  <linearGradient id="junction-d" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0%" stop-color="#dbe8ef" />
                    <stop offset="100%" stop-color="#bcd0db" />
                  </linearGradient>
                </defs>
                <rect x="20" y="30" width="120" height="80" rx="14" fill="url(#junction-c)" />
                <text x="80" y="58" text-anchor="middle" font-family="Segoe UI Variable Display,sans-serif" font-weight="700" font-size="14" fill="#f8fafc">C 盘</text>
                <text x="80" y="80" text-anchor="middle" font-family="Segoe UI Variable Text,sans-serif" font-size="11" fill="rgba(248,250,252,0.85)">Chrome</text>
                <text x="80" y="96" text-anchor="middle" font-family="Segoe UI Variable Text,sans-serif" font-size="11" fill="rgba(248,250,252,0.85)">缓存</text>

                <line x1="148" y1="70" x2="212" y2="70" stroke="#0f766e" stroke-width="2" stroke-dasharray="4 4" />
                <polygon points="208,64 220,70 208,76" fill="#0f766e" />
                <text x="180" y="58" text-anchor="middle" font-family="Segoe UI Variable Text,sans-serif" font-size="11" font-weight="700" fill="#0f766e">junction</text>

                <rect x="220" y="30" width="120" height="80" rx="14" fill="url(#junction-d)" stroke="rgba(15,118,110,0.2)" />
                <text x="280" y="58" text-anchor="middle" font-family="Segoe UI Variable Display,sans-serif" font-weight="700" font-size="14" fill="#121923">D 盘</text>
                <text x="280" y="80" text-anchor="middle" font-family="Segoe UI Variable Text,sans-serif" font-size="11" fill="#43505c">Chrome</text>
                <text x="280" y="96" text-anchor="middle" font-family="Segoe UI Variable Text,sans-serif" font-size="11" fill="#43505c">缓存</text>
              </svg>
            </div>
            <h2 class="step-title">什么是 Junction</h2>
            <ol class="explain-list">
              <li>你的应用以为文件还在 C 盘</li>
              <li>实际上它们已经搬到 D 盘了</li>
              <li>应用照常运行，只是 C 盘空间被释放</li>
            </ol>
            <p class="step-stress">零修改，零配置</p>
          </section>

          <section v-else-if="step === 3" :key="3" class="step">
            <h2 class="step-title">为什么建议管理员模式</h2>
            <p class="step-sub">权限决定扫描的速度和能力</p>

            <div class="perm-table">
              <div class="perm-col">
                <span class="perm-tag">普通权限</span>
                <strong>~30 秒</strong>
                <small>基础目录枚举</small>
                <ul>
                  <li>能扫描全部文件</li>
                  <li>每次都全量扫</li>
                </ul>
              </div>
              <div class="perm-col perm-col--strong">
                <span class="perm-tag perm-tag--primary">管理员</span>
                <strong>5 秒内</strong>
                <small>MFT 直读 + USN 增量</small>
                <ul>
                  <li>千万级文件秒级扫</li>
                  <li>只读变化，热数据复用</li>
                </ul>
              </div>
            </div>

            <div class="perm-actions">
              <button v-if="!isElevated" class="step-btn step-btn--primary" @click="enableAdmin">开启管理员</button>
              <button class="step-btn step-btn--ghost" @click="next">{{ isElevated ? '已是管理员，下一步' : '先用普通模式' }}</button>
            </div>
          </section>

          <section v-else :key="4" class="step">
            <div class="ready-mark" aria-hidden="true">✓</div>
            <h2 class="step-title">准备好了</h2>
            <p class="step-sub">建议先扫描 C 盘看看现状</p>
          </section>
        </transition>
      </div>

      <footer class="onboarding-foot">
        <button class="step-btn step-btn--ghost" :disabled="!canBack" @click="prev">上一步</button>
        <div class="step-dots">
          <span
            v-for="i in totalSteps"
            :key="i"
            class="step-dot"
            :class="{ active: step === i }"
            :aria-label="`第 ${i} 步${step === i ? '（当前）' : ''}`"
          ></span>
        </div>
        <button
          v-if="!isLast"
          ref="primaryButtonRef"
          class="step-btn step-btn--primary"
          @click="next"
        >
          下一步
        </button>
        <button
          v-else
          ref="primaryButtonRef"
          class="step-btn step-btn--primary"
          @click="finish"
        >
          开始扫描
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.onboarding-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.2rem;
  background: var(--modal-backdrop);
  z-index: 2000;
  font-family: var(--font-sans);
  animation: overlay-in var(--transition-base);
}

@keyframes overlay-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

.onboarding-card {
  position: relative;
  width: min(560px, 92vw);
  max-height: 640px;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-secondary);
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border-light);
  box-shadow: var(--shadow-xl);
  overflow: hidden;
  animation: card-in var(--transition-base);
}

@keyframes card-in {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.onboarding-skip {
  position: absolute;
  top: 16px;
  right: 16px;
  height: 36px;
  padding: 0 0.85rem;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  background: var(--color-surface);
  color: var(--color-text-tertiary);
  cursor: pointer;
  font-size: 0.78rem;
  font-weight: 600;
  z-index: 2;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.onboarding-skip:hover {
  color: var(--color-text-primary);
  background: var(--color-surface-hover);
}

.onboarding-stage {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 3rem 2.4rem 1.6rem;
  display: flex;
  flex-direction: column;
}

.step {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 1rem;
  flex: 1;
}

.step-logo {
  width: 76px;
  height: 76px;
  margin-bottom: 0.4rem;
  border-radius: 20px;
  object-fit: cover;
  box-shadow:
    0 12px 28px rgba(15, 23, 32, 0.22),
    0 1px 0 rgba(255, 255, 255, 0.6) inset,
    0 0 0 1px rgba(15, 23, 32, 0.06);
}

.step-title {
  font-family: var(--font-display);
  font-size: 1.7rem;
  font-weight: 700;
  color: var(--color-text-primary);
  letter-spacing: -0.01em;
  line-height: 1.05;
}

.step-sub {
  font-size: 0.95rem;
  color: var(--color-text-secondary);
  margin-top: -0.25rem;
}

.value-list {
  list-style: none;
  padding: 0;
  margin: 0.5rem 0 0;
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  width: 100%;
  max-width: 380px;
}

.value-list li {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 0.85rem;
  padding: 0.85rem 1rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  text-align: left;
}

.value-icon {
  font-size: 1.25rem;
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  background: var(--color-highlight-soft);
}

.value-list strong {
  font-size: 0.95rem;
  color: var(--color-text-primary);
  font-weight: 700;
}

.value-list small {
  display: block;
  margin-top: 0.18rem;
  font-size: 0.8rem;
  color: var(--color-text-tertiary);
  line-height: 1.4;
}

.junction-illustration {
  width: 100%;
  max-width: 360px;
  margin-bottom: 0.4rem;
}

.junction-illustration svg {
  width: 100%;
  height: auto;
}

.explain-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  width: 100%;
  max-width: 380px;
  text-align: left;
  counter-reset: explain;
}

.explain-list li {
  position: relative;
  padding: 0.7rem 0.95rem 0.7rem 2.4rem;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  color: var(--color-text-primary);
  font-size: 0.9rem;
  line-height: 1.5;
  counter-increment: explain;
}

.explain-list li::before {
  content: counter(explain);
  position: absolute;
  left: 0.85rem;
  top: 50%;
  transform: translateY(-50%);
  width: 22px;
  height: 22px;
  border-radius: var(--radius-pill);
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  font-size: 0.72rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.step-stress {
  margin-top: 0.4rem;
  padding: 0.5rem 1rem;
  border-radius: var(--radius-pill);
  background: var(--color-highlight-soft);
  color: var(--color-highlight);
  font-weight: 700;
  font-size: 0.92rem;
  letter-spacing: 0.02em;
}

.perm-table {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.7rem;
  width: 100%;
  margin-top: 0.5rem;
}

.perm-col {
  padding: 1rem;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  text-align: left;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

.perm-col--strong {
  background: linear-gradient(180deg, var(--color-highlight-soft), var(--color-surface-strong));
  border-color: rgba(15, 118, 110, 0.24);
}

.perm-tag {
  align-self: flex-start;
  padding: 0.18rem 0.55rem;
  border-radius: var(--radius-pill);
  background: var(--color-bg-tertiary);
  color: var(--color-text-secondary);
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.perm-tag--primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
}

.perm-col strong {
  font-family: var(--font-display);
  font-size: 1.45rem;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  line-height: 1;
}

.perm-col small {
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
}

.perm-col ul {
  list-style: none;
  padding: 0;
  margin: 0.3rem 0 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.perm-col ul li {
  position: relative;
  padding-left: 1rem;
  font-size: 0.82rem;
  color: var(--color-text-secondary);
}

.perm-col ul li::before {
  content: '·';
  position: absolute;
  left: 0.2rem;
  top: 0;
  font-weight: 700;
  color: var(--color-highlight);
}

.perm-actions {
  display: flex;
  gap: 0.55rem;
  margin-top: 0.6rem;
}

.ready-mark {
  width: 64px;
  height: 64px;
  border-radius: var(--radius-pill);
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 1.6rem;
  font-weight: 700;
  box-shadow: 0 14px 28px rgba(15, 118, 110, 0.32);
}

.onboarding-foot {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  gap: 0.85rem;
  padding: 1rem 1.6rem 1.2rem;
  border-top: 1px solid var(--color-border-light);
  background: var(--color-surface);
}

.step-dots {
  display: flex;
  gap: 0.35rem;
  align-items: center;
  justify-content: center;
}

.step-dot {
  width: 4px;
  height: 4px;
  border-radius: var(--radius-pill);
  background: var(--color-border-medium);
  transition: width var(--transition-base), height var(--transition-base), background var(--transition-base);
}

.step-dot.active {
  width: 8px;
  height: 8px;
  background: var(--color-highlight);
}

.step-btn {
  padding: 0.62rem 1.1rem;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  font-size: 0.88rem;
  font-weight: 600;
  cursor: pointer;
  transition: transform var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
}

.step-btn:focus-visible {
  outline: 2px solid var(--color-highlight);
  outline-offset: 2px;
}

.step-btn--primary {
  background: var(--color-highlight);
  color: var(--color-text-inverse);
  box-shadow: 0 10px 22px rgba(15, 118, 110, 0.24);
}

.step-btn--primary:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 14px 26px rgba(15, 118, 110, 0.3);
}

.step-btn--primary:active:not(:disabled) {
  transform: scale(0.97);
}

.step-btn--ghost {
  background: var(--color-surface-strong);
  color: var(--color-text-secondary);
  border-color: var(--color-border-medium);
}

.step-btn--ghost:hover:not(:disabled) {
  background: var(--color-surface-hover);
  color: var(--color-text-primary);
  transform: translateY(-1px);
}

.step-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.slide-forward-enter-active,
.slide-forward-leave-active,
.slide-backward-enter-active,
.slide-backward-leave-active {
  transition: transform var(--transition-base), opacity var(--transition-base);
}

.slide-forward-enter-from {
  opacity: 0;
  transform: translateX(24px);
}

.slide-forward-leave-to {
  opacity: 0;
  transform: translateX(-24px);
}

.slide-backward-enter-from {
  opacity: 0;
  transform: translateX(-24px);
}

.slide-backward-leave-to {
  opacity: 0;
  transform: translateX(24px);
}
</style>
