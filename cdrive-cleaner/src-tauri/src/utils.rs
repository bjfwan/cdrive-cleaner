use std::path::PathBuf;
use anyhow::Result;

/// 获取应用数据目录（应用安装目录下的 data 文件夹）
pub fn get_app_data_dir() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent()
        .ok_or_else(|| anyhow::anyhow!("无法获取应用目录"))?;
    
    let data_dir = exe_dir.join("data");
    
    // 确保目录存在
    std::fs::create_dir_all(&data_dir)?;
    
    Ok(data_dir)
}

/// 获取扫描缓存数据库路径
pub fn get_scan_cache_db_path() -> Result<PathBuf> {
    Ok(get_app_data_dir()?.join("scan_cache.db"))
}

/// 获取迁移记录数据库路径
pub fn get_migrations_db_path() -> Result<PathBuf> {
    Ok(get_app_data_dir()?.join("migrations.db"))
}
