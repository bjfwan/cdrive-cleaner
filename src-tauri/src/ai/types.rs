use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSummary {
    pub drive: String,
    pub total_gb: u64,
    pub free_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeItemSummary {
    pub token: String,
    pub label: String,
    pub size_mb: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub kind: String,
    pub total_mb: u64,
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSnapshot {
    pub disks: Vec<DiskSummary>,
    pub large_items: Vec<LargeItemSummary>,
    pub categories: Vec<CategorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSuggestion {
    pub id: String,
    pub target_label: String,
    pub target_token: String,
    pub action: String,
    pub reason: String,
    #[serde(default)]
    pub estimated_savings_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMode {
    Builtin,
    Byok,
}

impl Default for AiMode {
    fn default() -> Self {
        AiMode::Builtin
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiProvider {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: AiMode,
    #[serde(default)]
    pub provider: AiProvider,
    #[serde(default)]
    pub categories: AiCategoryToggles,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
}

fn default_max_items() -> usize {
    20
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AiMode::default(),
            provider: AiProvider::default(),
            categories: AiCategoryToggles::default(),
            max_items: default_max_items(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCategoryToggles {
    #[serde(default = "default_true")]
    pub temp_files: bool,
    #[serde(default = "default_true")]
    pub dev_tools: bool,
    #[serde(default = "default_true")]
    pub app_cache: bool,
    #[serde(default = "default_true")]
    pub large_files: bool,
    #[serde(default = "default_true")]
    pub large_dirs: bool,
    #[serde(default = "default_true")]
    pub system_files: bool,
    #[serde(default = "default_true")]
    pub model_files: bool,
    #[serde(default = "default_true")]
    pub game_files: bool,
    #[serde(default = "default_true")]
    pub media_files: bool,
    #[serde(default = "default_true")]
    pub disk_images: bool,
    #[serde(default = "default_true")]
    pub installer_files: bool,
    #[serde(default = "default_true")]
    pub downloads: bool,
    #[serde(default = "default_true")]
    pub log_files: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AiCategoryToggles {
    fn default() -> Self {
        Self {
            temp_files: true,
            dev_tools: true,
            app_cache: true,
            large_files: true,
            large_dirs: true,
            system_files: true,
            model_files: true,
            game_files: true,
            media_files: true,
            disk_images: true,
            installer_files: true,
            downloads: true,
            log_files: true,
        }
    }
}

impl AiCategoryToggles {
    pub fn any_enabled(&self) -> bool {
        self.temp_files || self.dev_tools || self.app_cache
            || self.large_files || self.large_dirs
            || self.system_files || self.model_files
            || self.game_files || self.media_files
            || self.disk_images || self.installer_files
            || self.downloads || self.log_files
    }

    pub fn allows(&self, kind: &str) -> bool {
        match kind {
            "temp_files" => self.temp_files,
            "dev_tools" => self.dev_tools,
            "app_cache" => self.app_cache,
            "large_files" => self.large_files,
            "large_dirs" => self.large_dirs,
            "system_files" => self.system_files,
            "model_files" => self.model_files,
            "game_files" => self.game_files,
            "media_files" => self.media_files,
            "disk_images" => self.disk_images,
            "installer_files" => self.installer_files,
            "downloads" => self.downloads,
            "log_files" => self.log_files,
            _ => false,
        }
    }
}
