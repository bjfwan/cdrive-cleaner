import { reactive, readonly } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export const CURRENT_CONSENT_VERSION = 1;

export const BUILTIN_MODELS = ['deepseek-v4-pro', 'glm-5.1'] as const;
export type BuiltinModel = (typeof BUILTIN_MODELS)[number];

export type AiMode = 'builtin' | 'byok';

export interface AiCategoriesState {
  disks: boolean;
  largeFiles: boolean;
  categories: boolean;
  duplicates: boolean;
}

export interface AiByokConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface AiSettings {
  enabled: boolean;
  mode: AiMode;
  byok: AiByokConfig;
  builtinModel: string;
  categories: AiCategoriesState;
  consentVersion: number;
}

export interface AiSuggestion {
  id: string;
  target_label: string;
  target_token: string;
  action: string;
  reason: string;
  estimated_savings_mb: number;
}

export interface AiSnapshotDisk {
  drive_letter?: string;
  total_mb?: number;
  free_mb?: number;
  [key: string]: unknown;
}

export interface AiSnapshot {
  disks?: AiSnapshotDisk[];
  large_items?: Array<Record<string, unknown>>;
  categories?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

const LOCAL_STORAGE_KEY = 'cdrive-cleaner-ai-settings';
const LOCAL_SUGGESTIONS_KEY = 'cdrive-cleaner-ai-suggestions';

export const DEFAULT_AI_SETTINGS: AiSettings = {
  enabled: false,
  mode: 'builtin',
  byok: { baseUrl: '', apiKey: '', model: '' },
  builtinModel: 'deepseek-v4-pro',
  categories: {
    disks: false,
    largeFiles: false,
    categories: false,
    duplicates: false,
  },
  consentVersion: 0,
};

function normalizeSettings(value: Partial<AiSettings> | null | undefined): AiSettings {
  const safe = (value ?? {}) as Partial<AiSettings>;
  const cats = (safe.categories ?? {}) as Partial<AiCategoriesState>;
  const byok = (safe.byok ?? {}) as Partial<AiByokConfig>;
  return {
    enabled: typeof safe.enabled === 'boolean' ? safe.enabled : DEFAULT_AI_SETTINGS.enabled,
    mode: safe.mode === 'byok' ? 'byok' : 'builtin',
    byok: {
      baseUrl: typeof byok.baseUrl === 'string' ? byok.baseUrl : '',
      apiKey: typeof byok.apiKey === 'string' ? byok.apiKey : '',
      model: typeof byok.model === 'string' ? byok.model : '',
    },
    builtinModel: typeof safe.builtinModel === 'string' && safe.builtinModel
      ? safe.builtinModel
      : DEFAULT_AI_SETTINGS.builtinModel,
    categories: {
      disks: typeof cats.disks === 'boolean' ? cats.disks : false,
      largeFiles: typeof cats.largeFiles === 'boolean' ? cats.largeFiles : false,
      categories: typeof cats.categories === 'boolean' ? cats.categories : false,
      duplicates: typeof cats.duplicates === 'boolean' ? cats.duplicates : false,
    },
    consentVersion: typeof safe.consentVersion === 'number' ? safe.consentVersion : 0,
  };
}

function loadLocalSettings(): AiSettings {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY);
    if (!raw) return normalizeSettings(null);
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return normalizeSettings(null);
  }
}

function saveLocalSettings(settings: AiSettings) {
  try {
    localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // ignore
  }
}

function loadLocalSuggestions(): AiSuggestion[] {
  try {
    const raw = localStorage.getItem(LOCAL_SUGGESTIONS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isSuggestion) : [];
  } catch {
    return [];
  }
}

function saveLocalSuggestions(items: AiSuggestion[]) {
  try {
    localStorage.setItem(LOCAL_SUGGESTIONS_KEY, JSON.stringify(items));
  } catch {
    // ignore
  }
}

function isSuggestion(value: unknown): value is AiSuggestion {
  if (!value || typeof value !== 'object') return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.id === 'string'
    && typeof v.target_label === 'string'
    && typeof v.target_token === 'string'
    && typeof v.action === 'string'
    && typeof v.reason === 'string'
    && typeof v.estimated_savings_mb === 'number'
  );
}

const state = reactive({
  settings: loadLocalSettings(),
  suggestions: loadLocalSuggestions() as AiSuggestion[],
  loadingSuggestions: false,
  lastError: '' as string,
  listenerInstalled: false,
});

let unlistenSuggestionsReady: UnlistenFn | null = null;

export const aiStore = {
  state: readonly(state),

  async pullFromBackend(): Promise<void> {
    try {
      const remote = await invoke<unknown>('cmd_settings_get', { path: 'ai' });
      if (remote !== undefined && remote !== null) {
        state.settings = normalizeSettings(remote as Partial<AiSettings>);
        saveLocalSettings(state.settings);
      }
    } catch {
      // Track B may not be ready yet; keep local snapshot.
    }
  },

  async pushToBackend(): Promise<void> {
    saveLocalSettings(state.settings);
    try {
      await invoke('cmd_settings_set', { path: 'ai', value: state.settings });
    } catch {
      // Backend not ready: localStorage is the temporary source of truth.
    }
  },

  async patchSettings(patch: Partial<AiSettings>): Promise<void> {
    state.settings = normalizeSettings({ ...state.settings, ...patch });
    await this.pushToBackend();
  },

  async patchCategories(patch: Partial<AiCategoriesState>): Promise<void> {
    state.settings.categories = { ...state.settings.categories, ...patch };
    await this.pushToBackend();
  },

  async patchByok(patch: Partial<AiByokConfig>): Promise<void> {
    state.settings.byok = { ...state.settings.byok, ...patch };
    await this.pushToBackend();
  },

  async setConsented(): Promise<void> {
    state.settings.consentVersion = CURRENT_CONSENT_VERSION;
    await this.pushToBackend();
  },

  hasConsent(): boolean {
    return state.settings.consentVersion >= CURRENT_CONSENT_VERSION;
  },

  setSuggestions(items: AiSuggestion[]): void {
    const valid = items.filter(isSuggestion);
    state.suggestions = valid;
    saveLocalSuggestions(valid);
  },

  dismiss(id: string): void {
    state.suggestions = state.suggestions.filter((s) => s.id !== id);
    saveLocalSuggestions(state.suggestions);
  },

  clear(): void {
    state.suggestions = [];
    saveLocalSuggestions([]);
  },

  async installSuggestionListener(): Promise<void> {
    if (state.listenerInstalled) return;
    try {
      unlistenSuggestionsReady = await listen<AiSuggestion[]>('ai-suggestions-ready', (event) => {
        const payload = Array.isArray(event.payload) ? event.payload : [];
        this.setSuggestions(payload);
      });
      state.listenerInstalled = true;
    } catch {
      // Listen API unavailable (e.g. browser preview). Ignore silently.
    }
  },

  uninstallSuggestionListener(): void {
    if (unlistenSuggestionsReady) {
      unlistenSuggestionsReady();
      unlistenSuggestionsReady = null;
    }
    state.listenerInstalled = false;
  },

  async previewSnapshot(): Promise<AiSnapshot> {
    try {
      const snapshot = await invoke<AiSnapshot>('cmd_ai_preview_snapshot');
      return snapshot ?? {};
    } catch (err) {
      state.lastError = String(err);
      return localPreviewSnapshot(state.settings);
    }
  },

  async requestAnalyze(): Promise<void> {
    state.loadingSuggestions = true;
    try {
      await invoke('cmd_ai_analyze');
    } catch (err) {
      state.lastError = String(err);
    } finally {
      state.loadingSuggestions = false;
    }
  },

  async resolveSuggestionPath(id: string): Promise<string | null> {
    try {
      const result = await invoke<string | { path?: string } | null>('cmd_ai_apply', { id });
      if (typeof result === 'string') return result;
      if (result && typeof result === 'object' && typeof (result as { path?: string }).path === 'string') {
        return (result as { path: string }).path;
      }
      return null;
    } catch (err) {
      state.lastError = String(err);
      return null;
    }
  },
};

function localPreviewSnapshot(settings: AiSettings): AiSnapshot {
  // Fallback preview when the Rust command isn't wired yet. We honor the
  // category toggles so an all-off snapshot reads as nearly empty — the same
  // contract the real command must hold.
  const snapshot: AiSnapshot = {};
  if (settings.categories.disks) snapshot.disks = [];
  if (settings.categories.largeFiles) snapshot.large_items = [];
  if (settings.categories.categories) snapshot.categories = [];
  if (settings.categories.duplicates) snapshot.duplicates = [];
  snapshot.mode = settings.mode;
  if (settings.mode === 'builtin') {
    snapshot.builtin_model = settings.builtinModel;
  } else {
    snapshot.byok_model = settings.byok.model;
  }
  return snapshot;
}
