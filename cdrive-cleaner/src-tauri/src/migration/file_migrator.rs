use super::link_creator::{LinkCreator, LinkType};
use anyhow::Result;
use std::path::Path;

pub struct FileMigrator {
    link_creator: LinkCreator,
}

impl FileMigrator {
    pub fn new() -> Self {
        Self {
            link_creator: LinkCreator::new(),
        }
    }

    pub async fn migrate<P: AsRef<Path>>(
        &self,
        source: P,
        target_disk: P,
        link_type: LinkType,
    ) -> Result<MigrationResult> {
        Ok(MigrationResult {
            success: true,
            source_path: source.as_ref().to_string_lossy().to_string(),
            target_path: target_disk.as_ref().to_string_lossy().to_string(),
            link_type,
            file_size: 0,
            duration_ms: 0,
            migration_id: 0,
            error: None,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MigrationResult {
    pub success: bool,
    pub source_path: String,
    pub target_path: String,
    pub link_type: LinkType,
    pub file_size: u64,
    pub duration_ms: u64,
    pub migration_id: i64,
    pub error: Option<String>,
}
