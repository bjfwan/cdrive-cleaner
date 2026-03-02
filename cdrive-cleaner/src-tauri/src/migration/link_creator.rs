use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[cfg(target_os = "windows")]
use std::os::windows::fs as windows_fs;

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
        let source = source.as_ref();
        let target = target.as_ref();

        let actual_link_type = match link_type {
            LinkType::Auto => self.determine_link_type(source, target)?,
            other => other,
        };

        #[cfg(target_os = "windows")]
        {
            match actual_link_type {
                LinkType::Symlink => {
                    if source.is_dir() {
                        windows_fs::symlink_dir(target, source)?;
                    } else {
                        windows_fs::symlink_file(target, source)?;
                    }
                }
                LinkType::Junction => {
                    if !source.is_dir() {
                        return Err(anyhow!("Junction can only be created for directories"));
                    }
                    junction::create(target, source)?;
                }
                LinkType::Hardlink => {
                    if source.is_dir() {
                        return Err(anyhow!("Hard links cannot be created for directories"));
                    }
                    fs::hard_link(target, source)?;
                }
                LinkType::Auto => unreachable!(),
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            std::os::unix::fs::symlink(target, source)?;
        }

        Ok(actual_link_type)
    }

    pub fn verify_link<P: AsRef<Path>>(&self, link_path: P) -> Result<bool> {
        let path = link_path.as_ref();
        
        if !path.exists() {
            return Ok(false);
        }

        let metadata = fs::symlink_metadata(path)?;
        
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)?;
            Ok(target.exists())
        } else {
            Ok(true)
        }
    }

    fn determine_link_type<P: AsRef<Path>>(&self, source: P, target: P) -> Result<LinkType> {
        let source = source.as_ref();
        let target = target.as_ref();

        let source_root = self.get_root_path(source)?;
        let target_root = self.get_root_path(target)?;

        if source.is_dir() {
            Ok(LinkType::Junction)
        } else if source_root == target_root {
            Ok(LinkType::Hardlink)
        } else {
            Ok(LinkType::Symlink)
        }
    }

    fn get_root_path<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let path = path.as_ref();
        
        #[cfg(target_os = "windows")]
        {
            let path_str = path.to_string_lossy();
            if let Some(root) = path_str.chars().take(2).collect::<String>().strip_suffix(':') {
                Ok(root.to_uppercase())
            } else {
                Err(anyhow!("Cannot determine root path"))
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Ok("/".to_string())
        }
    }
}
