export type JunkCategory =
  | 'system_temp' | 'browser_cache' | 'windows_update'
  | 'thumbnail_cache' | 'recycle_bin' | 'crash_dump'
  | 'app_logs' | 'font_cache';

export type JunkRiskLevel = 'safe' | 'caution' | 'risky';

export interface JunkItem {
  rule_id: string;
  category: JunkCategory;
  category_name: string;
  rule_name: string;
  path: string;
  size: number;
  file_count: number;
  is_directory: boolean;
  risk_level: JunkRiskLevel;
  default_selected: boolean;
  why_safe: string | null;
}

export interface DryRunInput {
  path: string;
  rule_id: string;
  size_bytes: number;
}

export interface FileItemPreview {
  path: string;
  size_mb: number;
  mtime_iso: string;
  rule_id: string;
  is_symlink: boolean;
}

export type DrySkipReasonCode =
  | 'parent_child_conflict'
  | 'locked_by_process'
  | 'too_recent'
  | 'permission_denied'
  | 'not_found';

export interface DrySkipReason {
  path: string;
  rule_id: string;
  reason: DrySkipReasonCode;
  detail: string | null;
}

export interface DryRunReport {
  will_delete: FileItemPreview[];
  will_skip: DrySkipReason[];
  estimated_freed_mb: number;
  estimated_seconds: number;
}

export interface JunkFeedback {
  path: string;
  rule_id: string;
  rule_name: string;
  user_note: string;
  reported_at_iso: string;
}

export interface JunkScanResult {
  items: JunkItem[];
  total_size: number;
  total_count: number;
  scanned_rules: number;
  skipped_rules: string[];
  scan_duration_ms: number;
}

export interface JunkCleanError {
  path: string;
  error: string;
  reason?: string;
  suggestion?: string;
}

export interface JunkCleanResult {
  success: boolean;
  cleaned_size: number;
  cleaned_count: number;
  failed_count: number;
  errors: JunkCleanError[];
  duration_ms: number;
}

export const CATEGORY_LABELS: Record<JunkCategory, string> = {
  system_temp: '系统临时文件',
  browser_cache: '浏览器缓存',
  windows_update: 'Windows 更新残留',
  thumbnail_cache: '缩略图缓存',
  recycle_bin: '回收站',
  crash_dump: '崩溃转储',
  app_logs: '应用日志',
  font_cache: '字体缓存',
};

export const CMD_SCAN_JUNK = 'scan_junk_files' as const;
export const CMD_CLEAN_JUNK = 'clean_junk_files' as const;
export const CMD_DRY_RUN_JUNK = 'junk_dry_run' as const;
export const CMD_REPORT_JUNK_FEEDBACK = 'junk_report_feedback' as const;

export const SKIP_REASON_LABEL: Record<DrySkipReasonCode, string> = {
  parent_child_conflict: '父子规则冲突',
  locked_by_process: '被进程占用',
  too_recent: '文件最近还在使用',
  permission_denied: '权限不足',
  not_found: '路径已不存在',
};
