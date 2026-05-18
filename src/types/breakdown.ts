export interface BreakdownItem {
  path: string;
  name: string;
  size: number;
  item_type: 'directory' | 'file';
  can_migrate: boolean;
  can_delete: boolean;
  explanation: string;
}

export interface BreakdownCategory {
  id: string;
  label: string;
  description: string;
  size: number;
  percentage: number;
  item_count: number;
  actionable: 'full' | 'partial' | 'none' | 'unknown';
  color: string;
  top_items: BreakdownItem[];
}

export interface SpaceBreakdown {
  disk_path: string;
  disk_total: number;
  disk_used: number;
  disk_free: number;
  categories: BreakdownCategory[];
  actionable_total: number;
  non_actionable_total: number;
  analysis_duration_ms: number;
}

export interface BalanceItem {
  path: string;
  name: string;
  size: number;
  category: string;
  action: 'migrate' | 'redirect';
  priority: number;
}

export interface BalanceSuggestion {
  source_disk: string;
  target_disk: string;
  current_source_used: number;
  current_target_used: number;
  projected_source_used: number;
  projected_target_used: number;
  total_movable: number;
  suggested_items: BalanceItem[];
}

export interface ReclaimOpportunity {
  id: string;
  label: string;
  description: string;
  current_size: number;
  reclaimable_size: number;
  requires_admin: boolean;
  requires_reboot: boolean;
  reversible: boolean;
  risk_level: 'safe' | 'caution' | 'irreversible';
}

export interface ReclaimResult {
  success: boolean;
  freed_bytes: number;
  requires_reboot: boolean;
  message: string;
}

export interface KnownFolderInfo {
  id: string;
  display_name: string;
  current_path: string;
  default_path: string;
  size_bytes: number;
  is_on_system_drive: boolean;
  is_default_location: boolean;
}

export interface RedirectResult {
  success: boolean;
  moved_files: number;
  moved_bytes: number;
  requires_reboot: boolean;
  message: string;
}
