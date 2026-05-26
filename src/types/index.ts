export interface DiskInfo {
  drive_letter: string;
  label: string;
  file_system: string;
  total_space: number;
  free_space: number;
  used_space: number;
  usage_percent: number;
}

export interface DirectoryNode {
  path: string;
  name: string;
  size: number;
  file_count: number;
  dir_count: number;
  children: DirectoryNode[];
  has_children: boolean;
  is_symlink: boolean;
  link_target?: string;
  safety?: MigrationSafety;
  modified_time?: number;
}

export interface MigrationProgress {
  copied_bytes: number;
  total_bytes: number;
  copied_files: number;
  total_files: number;
  current_file: string;
  progress_percent: number;
}

export type DeleteMode = 'recycle' | 'permanent';

export interface DeleteError {
  path: string;
  error: string;
}

export interface DeleteResult {
  success: boolean;
  source_path: string;
  mode: DeleteMode;
  deleted_size: number;
  deleted_files: number;
  errors: DeleteError[];
  duration_ms: number;
}

export interface DeleteProgress {
  current_file: string;
  deleted_files: number;
  total_files: number;
  deleted_size: number;
  total_size: number;
  progress_percent: number;
  error_count: number;
}

export interface FileInfo {
  path: string;
  name: string;
  size: number;
  extension: string;
  modified_at: string;
  is_readonly: boolean;
  is_symlink: boolean;
  link_target?: string;
}

export type FilePageSort = 'size_desc' | 'size_asc' | 'name_asc' | 'name_desc' | 'modified_desc' | 'modified_asc';

export interface LargeFilesPage {
  files: FileInfo[];
  total: number;
  offset: number;
  limit: number;
  total_size: number;
  filtered_total_size: number;
  has_more?: boolean;
}

export interface DirectoryFilesPage {
  files: FileInfo[];
  total: number;
  offset: number;
  limit: number;
  total_size?: number;
  has_more?: boolean;
}

export interface ScanResult {
  root_path: string;
  total_size: number;
  system_reserved_bytes?: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: DirectoryNode[];
  large_files: FileInfo[];
  inaccessible_count: number;
  scan_backend?: string;
}

export interface DirectoryChildrenSnapshot {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  directories: DirectoryNode[];
}

export interface ScanCapabilities {
  is_elevated: boolean;
  file_system: string;
  mft_available: boolean;
  preferred_backend: string;
  admin_recommended: boolean;
  reason: string;
}

export interface MigrationSafety {
  verdict: 'safe' | 'safe_after_action' | 'blocked' | 'system_critical';
  can_migrate: boolean;
  findings: SafetyFinding[];
  required_actions: string[];
  app_type: string;
  analysis_duration_ms: number;
}

export interface SafetyFinding {
  gate: string;
  severity: 'info' | 'warning' | 'blocker';
  message: string;
  detail?: string;
}

export interface MigrationResult {
  success: boolean;
  source_path: string;
  target_path: string;
  link_type: 'auto' | 'symlink' | 'junction' | 'hardlink' | 'none';
  file_size: number;
  duration_ms: number;
  migration_id: number;
  error?: string | null;
  warnings: string[];
}

export interface MigrationRecord {
  id: number;
  source_path: string;
  target_path: string;
  link_type: string;
  file_size: number;
  created_at: string;
  status: string;
}

export interface MigrationStats {
  total_count: number;
  total_size: number;
  active_count: number;
  rolled_back_count: number;
}

export type CloseBehavior = 'exit' | 'tray';

export type DownloadMirror = 'none' | 'ghproxy' | 'custom';

export interface AppSettings {
  defaultTargetDisk: string;
  largeFileThreshold: number;
  createSymlink: boolean;
  defaultDeleteMode: DeleteMode;
  theme?: 'light' | 'dark' | 'auto';
  autoCheckUpdate?: boolean;
  downloadMirror?: DownloadMirror;
  downloadMirrorUrl?: string;
  // Track A (v0.1.9 后台扫描 + 托盘)
  closeBehavior?: CloseBehavior;
  schedulerEnabled?: boolean;
  schedulerIdleMinutes?: number;
}

export interface UpdateInfo {
  has_update: boolean;
  latest_version: string;
  current_version: string;
  release_notes: string;
  download_url: string;
  installer_url: string;
  published_at: string;
}

export interface CacheEntry {
  disk_path: string;
  scan_type: string;
  file_count: number;
  total_size: number;
  created_at: string;
  cache_size: number;
}

export interface CacheInfo {
  cache_path: string;
  total_size: number;
  caches: CacheEntry[];
}

export type ToastType = 'success' | 'info' | 'warning' | 'error';

export type SmartCategory = 'app_cache' | 'dev_tools' | 'temp_files' | 'large_files';
export type SmartAction = 'migrate' | 'delete' | 'review';
export type SmartRisk = 'safe' | 'caution' | 'blocked' | 'unknown';

export interface SmartItem {
  path: string;
  name: string;
  size: number;
  file_count: number;
  category: SmartCategory;
  recommendation: SmartAction;
  risk: SmartRisk;
  rule: string;
  default_selected: boolean;
}

export interface SmartGroup {
  category: SmartCategory;
  recommendation: SmartAction;
  total_size: number;
  item_count: number;
  selected_size: number;
  items: SmartItem[];
}

export interface SmartScanReport {
  root_path: string;
  generated_at_ms: number;
  potential_savings: number;
  default_savings: number;
  groups: SmartGroup[];
  analysis_duration_ms: number;
}

export type GamePlatform = 'steam' | 'epic' | 'game_pass' | 'microsoft_store';

export interface GameInfo {
  platform: GamePlatform;
  app_id: string;
  name: string;
  install_path: string;
  install_size: number;
  drive_letter: string;
  last_played?: string | null;
  can_migrate: boolean;
  migration_hint: string;
}

export interface GameLibraryInfo {
  platform: GamePlatform;
  library_paths: string[];
  games: GameInfo[];
  installed: boolean;
}
