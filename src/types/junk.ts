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
}

export interface JunkScanResult {
  items: JunkItem[];
  total_size: number;
  total_count: number;
  scanned_rules: number;
  skipped_rules: string[];
  scan_duration_ms: number;
}

export interface JunkCleanResult {
  success: boolean;
  cleaned_size: number;
  cleaned_count: number;
  failed_count: number;
  errors: { path: string; error: string }[];
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
