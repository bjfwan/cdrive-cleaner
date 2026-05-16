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

export interface ScanResult {
  root_path: string;
  total_size: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  directories: DirectoryNode[];
  large_files: FileInfo[];
  inaccessible_count: number;
  scan_backend?: string;
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

export interface AppSettings {
  defaultTargetDisk: string;
  largeFileThreshold: number;
  createSymlink: boolean;
  theme?: 'light' | 'dark' | 'auto';
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
