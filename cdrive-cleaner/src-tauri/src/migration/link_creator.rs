use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    Auto,
    Symlink,
    Junction,
    Hardlink,
}

pub struct LinkCreator;

impl LinkCreator {
    pub fn new() -> Self {
        Self
    }

    pub fn create_link<P: AsRef<Path>>(
        &self,
        source: P,
        target: P,
        link_type: LinkType,
    ) -> Result<LinkType> {
        Ok(link_type)
    }

    pub fn verify_link<P: AsRef<Path>>(&self, link_path: P) -> Result<bool> {
        Ok(true)
    }
}
