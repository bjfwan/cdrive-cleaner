<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { DiskInfo, GameInfo, GameLibraryInfo, GamePlatform } from '../types';
import { formatBytes } from '../utils/format';
import { useCart } from '../composables/useCart';
import { useToast } from '../composables/useToast';
import { fetchGameLibraries, getCachedGameLibraries } from '../composables/useGameDetection';
import VirtualList from './VirtualList.vue';
import { IconScanEmpty } from './icons';

interface Props {
  availableDisks: DiskInfo[];
  currentDrive: string;
}

defineProps<Props>();

const cart = useCart();
const showToast = useToast();

const platforms: Array<{ key: GamePlatform; label: string; sub: string; mark: string }> = [
  { key: 'steam', label: 'Steam', sub: 'Valve · 库索引透明，搬完即可启动', mark: 'STM' },
  { key: 'epic', label: 'Epic Games', sub: 'Epic 启动器 · 搬后会做一次完整性校验', mark: 'EPC' },
  { key: 'game_pass', label: 'Game Pass', sub: 'Xbox / Microsoft Store · 部分游戏走系统设置', mark: 'XBX' },
];

const libraries = ref<GameLibraryInfo[]>([]);
const loading = ref(false);
const activePlatform = ref<GamePlatform>('steam');
const checked = ref<Record<string, boolean>>({});

const activeLibrary = computed(() =>
  libraries.value.find((lib) => lib.platform === activePlatform.value),
);

const installedPlatforms = computed(() =>
  libraries.value.filter((lib) => lib.installed).map((lib) => lib.platform),
);

const migratableGames = computed(() => activeLibrary.value?.games.filter((g) => g.can_migrate) ?? []);
const blockedGames = computed(() => activeLibrary.value?.games.filter((g) => !g.can_migrate) ?? []);

// Incremental checked count/size tracking (avoids full traversal on each toggle)
let _checkedCount = 0;
let _checkedSize = 0;
const checkedCount = ref(0);
const checkedSize = ref(0);

function bumpChecked(countDelta: number, sizeDelta: number) {
  _checkedCount += countDelta;
  _checkedSize += sizeDelta;
  if (_checkedCount < 0) _checkedCount = 0;
  if (_checkedSize < 0) _checkedSize = 0;
  checkedCount.value = _checkedCount;
  checkedSize.value = _checkedSize;
}

function resetCheckedCounters() {
  _checkedCount = 0;
  _checkedSize = 0;
  checkedCount.value = 0;
  checkedSize.value = 0;
}

let cachedLibraries: GameLibraryInfo[] | null = null;

onMounted(() => {
  // Prefer the module-scoped cache so that switching away from the games tab
  // and coming back doesn't trigger a fresh 1~2s Steam/Epic/Game Pass scan.
  const cached = cachedLibraries ?? getCachedGameLibraries();
  if (cached) {
    cachedLibraries = cached;
    libraries.value = cached;
    const firstInstalled = cached.find((lib) => lib.installed);
    if (firstInstalled) {
      activePlatform.value = firstInstalled.platform;
    }
    return;
  }
  void load();
});

async function load(forceRefresh = false) {
  loading.value = true;
  try {
    const result = await fetchGameLibraries({ forceRefresh });
    libraries.value = result;
    cachedLibraries = result;
    const firstInstalled = result.find((lib) => lib.installed);
    if (firstInstalled) {
      activePlatform.value = firstInstalled.platform;
    }
  } catch (err) {
    showToast('游戏库检测失败', String(err), 'error');
  } finally {
    loading.value = false;
  }
}

function toggleGame(game: GameInfo) {
  const wasChecked = !!checked.value[game.install_path];
  checked.value = {
    ...checked.value,
    [game.install_path]: !wasChecked,
  };
  if (wasChecked) {
    bumpChecked(-1, -game.install_size);
  } else {
    bumpChecked(1, game.install_size);
  }
}

function selectAll() {
  const next = { ...checked.value };
  let countDelta = 0;
  let sizeDelta = 0;
  for (const game of migratableGames.value) {
    if (!next[game.install_path]) {
      next[game.install_path] = true;
      countDelta++;
      sizeDelta += game.install_size;
    }
  }
  checked.value = next;
  bumpChecked(countDelta, sizeDelta);
}

function deselectAll() {
  const next = { ...checked.value };
  for (const game of migratableGames.value) {
    delete next[game.install_path];
  }
  checked.value = next;
  resetCheckedCounters();
}

function platformLabel(p: GamePlatform) {
  switch (p) {
    case 'steam':
      return 'Steam';
    case 'epic':
      return 'Epic Games';
    case 'game_pass':
      return 'Game Pass';
    case 'microsoft_store':
      return 'Microsoft Store';
  }
}

function addToCart(game: GameInfo) {
  cart.add({
    path: game.install_path,
    name: game.name,
    size: game.install_size,
    file_count: 1,
    recommendation: 'migrate',
    source: 'game',
    game: { platform: game.platform, app_id: game.app_id },
  });
  showToast(
    '已加入搬运车',
    `${game.name} · ${formatBytes(game.install_size)}`,
    'info',
  );
}

function addCheckedToCart() {
  const items = migratableGames.value.filter((g) => checked.value[g.install_path]);
  if (items.length === 0) return;
  let totalSize = 0;
  const batch = items.map((game) => {
    totalSize += game.install_size;
    return {
      path: game.install_path,
      name: game.name,
      size: game.install_size,
      file_count: 1,
      recommendation: 'migrate' as const,
      source: 'game' as const,
      game: { platform: game.platform, app_id: game.app_id },
    };
  });
  cart.addBatch(batch);
  showToast(
    '已加入搬运车',
    `${items.length} 项游戏 · 共 ${formatBytes(totalSize)}`,
    'info',
  );
  deselectAll();
}

async function openNativeMigrationUi(platform: GamePlatform) {
  try {
    await invoke('open_native_migration_ui', { platform });
  } catch (err) {
    showToast('打开系统设置失败', String(err), 'error');
  }
}

defineExpose({ reload: load });
</script>

<template>
  <div class="games-view">
    <header class="games-head">
      <div class="games-title">
        <span class="kicker">游戏库</span>
        <h2>把 C 盘里的游戏搬到其他盘</h2>
        <p>Steam / Epic 直接 junction，启动照旧。Game Pass 部分游戏需要走系统设置。</p>
      </div>
      <div class="games-actions">
        <button class="ghost-btn" :disabled="loading" @click="load(true)">{{ loading ? '检测中…' : '重新检测' }}</button>
      </div>
    </header>

    <nav class="platform-tabs">
      <button
        v-for="p in platforms"
        :key="p.key"
        class="platform-tab"
        :class="{ active: activePlatform === p.key, off: !installedPlatforms.includes(p.key) }"
        @click="activePlatform = p.key"
      >
        <span class="platform-mark">{{ p.mark }}</span>
        <span class="platform-text">
          <strong>{{ p.label }}</strong>
          <small>{{ installedPlatforms.includes(p.key) ? p.sub : '未检测到安装' }}</small>
        </span>
      </button>
    </nav>

    <section v-if="loading" class="placeholder">
      <div class="spinner"></div>
      <p>正在扫描三家平台…</p>
    </section>

    <section v-else-if="!activeLibrary?.installed" class="placeholder">
      <div class="placeholder-icon"><IconScanEmpty :size="28" /></div>
      <h3>未检测到 {{ platformLabel(activePlatform) }}</h3>
      <p>如果确实安装了，请确保启动器至少完成过一次登录，然后点上面的“重新检测”。</p>
    </section>

    <section v-else class="library-body">
      <div v-if="(activeLibrary?.library_paths.length ?? 0) > 0" class="library-paths">
        <span>库路径</span>
        <code v-for="path in activeLibrary?.library_paths" :key="path">{{ path }}</code>
      </div>

      <div v-if="migratableGames.length > 0" class="batch-bar">
        <div class="batch-summary">
          <strong>{{ checkedCount }}</strong>
          <small>已选 · {{ formatBytes(checkedSize) }}</small>
        </div>
        <button class="ghost-btn" @click="checkedCount === migratableGames.length ? deselectAll() : selectAll()">
          {{ checkedCount === migratableGames.length ? '全部取消' : '全选' }}
        </button>
        <button class="primary-btn" :disabled="checkedCount === 0" @click="addCheckedToCart">
          加入搬运车
        </button>
      </div>

      <div v-if="migratableGames.length > 0" class="game-list-shell">
        <VirtualList
          :items="migratableGames"
          :item-size="82"
          :buffer="4"
          :overscan="2"
          v-slot="{ item: game }"
        >
          <div
            :key="(game as GameInfo).install_path"
            class="game-item"
            :class="{ checked: checked[(game as GameInfo).install_path] }"
          >
            <button class="game-check" @click="toggleGame(game as GameInfo)" aria-label="选择">
              <svg v-if="checked[(game as GameInfo).install_path]" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
                <path
                  d="M3 8.5L6.5 12L13 4.5"
                  stroke="currentColor"
                  stroke-width="2.4"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  fill="none"
                />
              </svg>
            </button>
            <div class="game-cover" :data-platform="(game as GameInfo).platform">{{ platforms.find((p) => p.key === (game as GameInfo).platform)?.mark ?? 'GME' }}</div>
            <div class="game-main">
              <div class="game-name-row">
                <span class="game-name" :title="(game as GameInfo).install_path">{{ (game as GameInfo).name }}</span>
                <span class="game-tag">{{ platformLabel((game as GameInfo).platform) }}</span>
                <span class="game-tag" v-if="(game as GameInfo).drive_letter">{{ (game as GameInfo).drive_letter }}</span>
              </div>
              <div class="game-path" :title="(game as GameInfo).install_path">{{ (game as GameInfo).install_path }}</div>
              <div class="game-hint">{{ (game as GameInfo).migration_hint }}</div>
            </div>
            <div class="game-size">{{ formatBytes((game as GameInfo).install_size) }}</div>
            <button class="single-btn" @click="addToCart(game as GameInfo)">加入搬运车</button>
          </div>
        </VirtualList>
      </div>

      <div v-else-if="blockedGames.length === 0" class="placeholder-soft">
        当前平台未发现已安装游戏。
      </div>

      <section v-if="blockedGames.length > 0" class="blocked-section">
        <header>
          <strong>需要在系统设置中迁移</strong>
          <small>{{ blockedGames.length }} 项 · WindowsApps 受系统保护，本工具不直接动它们</small>
        </header>
        <div class="blocked-list-shell">
          <VirtualList
            :items="blockedGames"
            :item-size="82"
            :buffer="4"
            :overscan="2"
            v-slot="{ item: game }"
          >
            <div :key="(game as GameInfo).install_path" class="blocked-item">
              <div class="game-cover blocked">MS</div>
              <div class="game-main">
                <div class="game-name-row">
                  <span class="game-name">{{ (game as GameInfo).name }}</span>
                  <span class="game-tag warn">{{ platformLabel((game as GameInfo).platform) }}</span>
                </div>
                <div class="game-path">{{ (game as GameInfo).install_path }}</div>
                <div class="game-hint">{{ (game as GameInfo).migration_hint }}</div>
              </div>
              <button class="primary-btn" @click="openNativeMigrationUi((game as GameInfo).platform)">在系统设置中迁移</button>
            </div>
          </VirtualList>
        </div>
      </section>
    </section>
  </div>
</template>

<style scoped>
.games-view {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 0.85rem 1rem 5rem;
}

.games-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.85rem 1.1rem 1rem;
  border-radius: 18px;
  background: linear-gradient(140deg, var(--color-bg-secondary), var(--color-bg-tertiary));
  border: 1px solid var(--color-border-light);
}
.games-title h2 {
  font-size: 1.4rem;
  font-weight: 700;
  color: var(--color-text-primary);
  margin-top: 0.25rem;
}
.games-title p {
  margin-top: 0.35rem;
  font-size: 0.84rem;
  color: var(--color-text-tertiary);
}
.kicker {
  display: inline-block;
  padding: 0.18rem 0.5rem;
  border-radius: 999px;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.platform-tabs {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.6rem;
}
.platform-tab {
  display: grid;
  grid-template-columns: 56px 1fr;
  align-items: center;
  gap: 0.7rem;
  padding: 0.85rem 1rem;
  border-radius: 16px;
  border: 1px solid var(--color-border-light);
  background: var(--color-surface-strong);
  text-align: left;
  cursor: pointer;
  transition: border-color var(--transition-fast), background var(--transition-fast),
    transform var(--transition-fast);
}
.platform-tab:hover { transform: translateY(-1px); }
.platform-tab.active {
  border-color: var(--color-text-primary);
  background: var(--color-bg-secondary);
}
.platform-tab.off { opacity: 0.55; }

.platform-mark {
  width: 56px;
  height: 56px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 14px;
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  font-family: var(--font-display);
  font-size: 1rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.platform-text strong {
  display: block;
  font-size: 1rem;
  color: var(--color-text-primary);
}
.platform-text small {
  display: block;
  margin-top: 0.18rem;
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
}

.placeholder {
  padding: 3rem 2rem;
  border-radius: 20px;
  border: 1px dashed var(--color-border-medium);
  background: var(--color-surface);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.6rem;
  text-align: center;
}
.placeholder h3 { font-size: 1.15rem; font-weight: 700; }
.placeholder p { color: var(--color-text-tertiary); font-size: 0.86rem; max-width: 32rem; }
.placeholder-icon {
  width: 44px;
  height: 44px;
  border-radius: 14px;
  background: rgba(15, 118, 110, 0.1);
  color: var(--color-highlight);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 1.4rem;
}

.placeholder-soft {
  padding: 1.5rem;
  border-radius: 16px;
  background: var(--color-surface);
  color: var(--color-text-tertiary);
  text-align: center;
  font-size: 0.9rem;
}

.spinner {
  width: 28px;
  height: 28px;
  border: 2.5px solid var(--color-border-medium);
  border-top-color: var(--color-highlight);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.library-body {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}

.library-paths {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.78rem;
  color: var(--color-text-tertiary);
  padding: 0.6rem 0.9rem;
  border-radius: 12px;
  background: var(--color-surface);
  border: 1px solid var(--color-border-light);
}
.library-paths code {
  background: rgba(32, 45, 58, 0.06);
  border-radius: 8px;
  padding: 0.18rem 0.45rem;
  font-family: var(--font-mono);
  font-size: 0.74rem;
  color: var(--color-text-secondary);
}

.batch-bar {
  display: flex;
  align-items: center;
  gap: 0.85rem;
  padding: 0.7rem 1rem;
  border-radius: 14px;
  background: var(--color-surface-strong);
  border: 1px solid var(--color-border-light);
}
.batch-summary {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
}
.batch-summary strong {
  font-size: 1.4rem;
  font-weight: 700;
  font-feature-settings: 'tnum';
  color: var(--color-text-primary);
}
.batch-summary small { color: var(--color-text-tertiary); font-size: 0.78rem; }

.ghost-btn,
.primary-btn,
.single-btn {
  padding: 0.55rem 0.9rem;
  border-radius: 12px;
  border: 1px solid var(--color-border-medium);
  font-weight: 600;
  font-size: 0.84rem;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast),
    border-color var(--transition-fast), transform var(--transition-fast);
}
.ghost-btn { background: var(--color-surface); color: var(--color-text-secondary); }
.ghost-btn:hover:not(:disabled) { background: var(--color-surface-hover); color: var(--color-text-primary); }
.ghost-btn:disabled { opacity: 0.5; cursor: not-allowed; }

.primary-btn {
  background: var(--color-text-primary);
  color: var(--color-text-inverse);
  border-color: transparent;
  margin-left: auto;
}
.primary-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.primary-btn:hover:not(:disabled) { transform: translateY(-1px); }

.single-btn { background: transparent; color: var(--color-text-secondary); }
.single-btn:hover { background: var(--color-surface-hover); color: var(--color-text-primary); }

.game-list-shell,
.blocked-list-shell {
  height: min(520px, 60vh);
  min-height: 164px;
  border-radius: 14px;
  overflow: hidden;
}

.game-list-shell :deep(.vlist),
.blocked-list-shell :deep(.vlist) {
  padding: 0.25rem 0;
}

.game-item,
.blocked-item {
  display: grid;
  grid-template-columns: 26px 56px minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 0.85rem;
  padding: 0.7rem 0.9rem;
  border-radius: 14px;
  background: var(--color-surface-strong);
  border: 1px solid var(--color-border-light);
  height: 82px;
  box-sizing: border-box;
  transition: border-color var(--transition-fast), background var(--transition-fast);
}
.game-item:hover { border-color: var(--color-border-medium); }
.game-item.checked { background: rgba(15, 118, 110, 0.05); border-color: var(--color-highlight); }

.blocked-item {
  grid-template-columns: 56px minmax(0, 1fr) auto;
  background: rgba(220, 38, 38, 0.04);
  border-color: rgba(220, 38, 38, 0.18);
}

.game-check {
  width: 22px;
  height: 22px;
  border-radius: 7px;
  border: 1.5px solid var(--color-border-medium);
  background: var(--color-surface);
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: white;
  transition: background var(--transition-fast), border-color var(--transition-fast);
}
.game-check:hover { border-color: var(--color-text-secondary); }
.game-item.checked .game-check {
  background: var(--color-highlight);
  border-color: var(--color-highlight);
}

.game-cover {
  width: 56px;
  height: 56px;
  border-radius: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(140deg, #1f2937, #0f172a);
  color: white;
  font-family: var(--font-display);
  font-size: 0.9rem;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.game-cover[data-platform='steam'] { background: linear-gradient(140deg, #1b2838, #2a475e); }
.game-cover[data-platform='epic'] { background: linear-gradient(140deg, #313131, #5d5d5d); }
.game-cover[data-platform='game_pass'],
.game-cover[data-platform='microsoft_store'] { background: linear-gradient(140deg, #107c10, #0e6b0e); }
.game-cover.blocked { background: linear-gradient(140deg, #b91c1c, #7f1d1d); }

.game-main { min-width: 0; }
.game-name-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
.game-name {
  font-weight: 600;
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 24rem;
}
.game-tag {
  padding: 0.12rem 0.45rem;
  border-radius: 999px;
  background: rgba(32, 45, 58, 0.08);
  color: var(--color-text-secondary);
  font-size: 0.7rem;
  font-weight: 600;
}
.game-tag.warn { background: rgba(220, 38, 38, 0.1); color: #b91c1c; }

.game-path {
  font-family: var(--font-mono);
  font-size: 0.74rem;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 0.18rem;
}
.game-hint {
  margin-top: 0.18rem;
  font-size: 0.78rem;
  color: var(--color-text-secondary);
}

.game-size {
  font-size: 0.96rem;
  font-weight: 700;
  color: var(--color-text-primary);
  font-feature-settings: 'tnum';
  text-align: right;
  min-width: 6rem;
}

.blocked-section {
  margin-top: 0.4rem;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}
.blocked-section header {
  display: flex;
  align-items: baseline;
  gap: 0.6rem;
  padding: 0.5rem 0.2rem;
}
.blocked-section header strong { font-size: 0.95rem; color: var(--color-text-primary); }
.blocked-section header small { font-size: 0.78rem; color: var(--color-text-tertiary); }

/* ===== Dark mode overrides ===== */
[data-theme="dark"] .game-tag {
  background: rgba(255, 255, 255, 0.06);
}
[data-theme="dark"] .game-tag.warn {
  background: rgba(248, 113, 113, 0.14);
  color: #fca5a5;
}
</style>
