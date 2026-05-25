pub mod client;
pub mod commands;
pub mod sanitizer;
pub mod settings;
pub mod state;
pub mod types;

pub use client::analyze;
pub use settings::SettingsStore;
pub use state::AiState;
pub use types::{
    AiCategoryToggles, AiMode, AiProvider, AiSettings, AiSnapshot, AiSuggestion, CategorySummary,
    DiskSummary, LargeItemSummary,
};
