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
        is_directory: bool,
    ) -> Result<LinkType> {
        let source = source.as_ref();
        let target = target.as_ref();

        println!("  [创建链接] 源: {}", source.display());
        println!("  [创建链接] 目标: {}", target.display());
        println!("  [创建链接] 请求类型: {:?}", link_type);
        println!("  [创建链接] 是否为目录: {}", is_directory);

        let actual_link_type = match link_type {
            LinkType::Auto => {
                let determined = self.determine_link_type(target, is_directory)?;
                println!("  [创建链接] 自动选择类型: {:?}", determined);
                determined
            },
            other => other,
        };

        #[cfg(target_os = "windows")]
        {
            match actual_link_type {
                LinkType::Symlink => {
                    println!("  [创建链接] 创建符号链接...");
                    if is_directory {
                        println!("  [创建链接] 目录符号链接");
                        windows_fs::symlink_dir(target, source)?;
                    } else {
                        println!("  [创建链接] 文件符号链接");
                        windows_fs::symlink_file(target, source)?;
                    }
                    println!("  [创建链接] ✓ 符号链接创建完成");
                }
                LinkType::Junction => {
                    if !is_directory {
                        return Err(anyhow!("Junction can only be created for directories"));
                    }
                    println!("  [创建链接] 创建 Junction...");
                    junction::create(target, source)?;
                    println!("  [创建链接] ✓ Junction 创建完成");
                }
                LinkType::Hardlink => {
                    if is_directory {
                        return Err(anyhow!("Hard links cannot be created for directories"));
                    }
                    println!("  [创建链接] 创建硬链接...");
                    fs::hard_link(target, source)?;
                    println!("  [创建链接] ✓ 硬链接创建完成");
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

    pub fn verify_link<P: AsRef<Path>>(&self, link_path: P, expected_target: P) -> Result<bool> {
        let path = link_path.as_ref();
        let expected = expected_target.as_ref();
        
        println!("  [验证] 检查路径: {}", path.display());
        println!("  [验证] 期望目标: {}", expected.display());
        
        if !path.exists() {
            println!("  [验证] ❌ 路径不存在");
            return Ok(false);
        }
        println!("  [验证] ✓ 路径存在");

        let metadata = fs::symlink_metadata(path)?;
        println!("  [验证] 文件类型: {:?}", metadata.file_type());
        
        if metadata.file_type().is_symlink() {
            println!("  [验证] 这是一个符号链接");
            let target = fs::read_link(path)?;
            println!("  [验证] 链接目标: {}", target.display());
            
            let target_canonical = target.canonicalize().unwrap_or(target.clone());
            let expected_canonical = expected.canonicalize().unwrap_or(expected.to_path_buf());
            
            println!("  [验证] 规范化目标: {}", target_canonical.display());
            println!("  [验证] 规范化期望: {}", expected_canonical.display());
            
            let targets_match = target_canonical == expected_canonical;
            let target_exists = expected.exists();
            
            println!("  [验证] 目标匹配: {}", targets_match);
            println!("  [验证] 目标存在: {}", target_exists);
            
            Ok(targets_match && target_exists)
        } else {
            println!("  [验证] 这不是符号链接，可能是 Junction 或 Hardlink");
            Ok(path.exists() && expected.exists())
        }
    }

    fn determine_link_type<P: AsRef<Path>>(&self, target: P, is_directory: bool) -> Result<LinkType> {
        let target = target.as_ref();

        if is_directory {
            Ok(LinkType::Junction)
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
