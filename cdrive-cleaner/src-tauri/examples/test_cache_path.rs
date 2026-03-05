fn main() {
    let exe_path = std::env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    let data_dir = exe_dir.join("data");
    
    println!("应用可执行文件路径: {}", exe_path.display());
    println!("应用安装目录: {}", exe_dir.display());
    println!("数据存储目录: {}", data_dir.display());
    println!("扫描缓存路径: {}", data_dir.join("scan_cache.db").display());
    println!("迁移记录路径: {}", data_dir.join("migrations.db").display());
}
