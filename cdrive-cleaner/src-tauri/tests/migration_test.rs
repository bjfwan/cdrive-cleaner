#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    #[tokio::test]
    async fn test_file_migration_real() {
        // 注意：这个测试会实际移动文件！
        println!("\n=== 测试文件迁移 ===");
        
        let source_path = "C:\\test_migration_source\\test.txt";
        let target_disk = "D:\\";
        
        println!("源路径: {}", source_path);
        println!("目标磁盘: {}", target_disk);
        
        // 检查前置条件
        assert!(Path::new(source_path).exists(), "源文件不存在");
        assert!(Path::new(target_disk).exists(), "目标磁盘不存在");
        println!("✓ 前置条件检查通过");
        
        // 读取原文件内容
        let original_content = fs::read_to_string(source_path).expect("无法读取源文件");
        println!("原文件内容: {}", original_content.trim());
        
        // 创建迁移器
        use cdrive_cleaner_lib::migration::{FileMigrator, LinkType};
        let migrator = FileMigrator::new();
        
        println!("开始迁移...");
        let result = migrator.migrate(source_path, target_disk, LinkType::Auto, None).await;
        
        match result {
            Ok(migration_result) => {
                println!("\n迁移结果:");
                println!("  成功: {}", migration_result.success);
                println!("  源路径: {}", migration_result.source_path);
                println!("  目标路径: {}", migration_result.target_path);
                println!("  链接类型: {:?}", migration_result.link_type);
                println!("  文件大小: {} 字节", migration_result.file_size);
                println!("  耗时: {} 毫秒", migration_result.duration_ms);
                
                if let Some(error) = &migration_result.error {
                    println!("  错误: {}", error);
                }
                
                if migration_result.success {
                    // 验证链接是否有效
                    println!("\n验证链接...");
                    assert!(Path::new(source_path).exists(), "链接不存在");
                    
                    // 尝试通过链接读取内容
                    let linked_content = fs::read_to_string(source_path).expect("无法通过链接读取文件");
                    println!("通过链接读取的内容: {}", linked_content.trim());
                    
                    assert_eq!(original_content, linked_content, "内容不匹配");
                    println!("✓ 链接验证通过");
                    
                    // 验证目标文件存在
                    assert!(Path::new(&migration_result.target_path).exists(), "目标文件不存在");
                    println!("✓ 目标文件存在");
                } else {
                    panic!("迁移失败: {:?}", migration_result.error);
                }
            }
            Err(e) => {
                panic!("迁移失败: {}", e);
            }
        }
        
        println!("\n=== 测试完成 ===\n");
    }

    #[tokio::test]
    async fn test_directory_migration_real() {
        // 测试目录迁移（使用 Junction，不需要管理员权限）
        println!("\n=== 测试目录迁移 ===");
        
        let source_path = "C:\\test_migration_source\\subdir";
        let target_disk = "D:\\";
        
        println!("源路径: {}", source_path);
        println!("目标磁盘: {}", target_disk);
        
        // 检查前置条件
        assert!(Path::new(source_path).exists(), "源目录不存在");
        assert!(Path::new(target_disk).exists(), "目标磁盘不存在");
        println!("✓ 前置条件检查通过");
        
        // 读取原目录中的文件
        let original_file = format!("{}\\file2.txt", source_path);
        let original_content = fs::read_to_string(&original_file).expect("无法读取源文件");
        println!("原文件内容: {}", original_content.trim());
        
        // 创建迁移器
        use cdrive_cleaner_lib::migration::{FileMigrator, LinkType};
        let migrator = FileMigrator::new();
        
        println!("开始迁移...");
        let result = migrator.migrate(source_path, target_disk, LinkType::Auto, None).await;
        
        match result {
            Ok(migration_result) => {
                println!("\n迁移结果:");
                println!("  成功: {}", migration_result.success);
                println!("  源路径: {}", migration_result.source_path);
                println!("  目标路径: {}", migration_result.target_path);
                println!("  链接类型: {:?}", migration_result.link_type);
                println!("  文件大小: {} 字节", migration_result.file_size);
                println!("  耗时: {} 毫秒", migration_result.duration_ms);
                
                assert!(migration_result.success, "迁移应该成功");
            }
            Err(e) => {
                panic!("迁移失败: {}", e);
            }
        }
        
        println!("\n=== 测试完成 ===\n");
    }
}
