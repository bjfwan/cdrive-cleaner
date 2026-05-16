//! 迁移历史数据库测试：验证 insert / 查询 / 状态更新（即回滚标记）/ 统计 都能正常往返。
//!
//! 用临时 sqlite 文件，无 GUI、无副作用，秒级跑完。

#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::database::MigrationDb;
    use std::path::PathBuf;

    fn temp_db_path(label: &str) -> PathBuf {
        let unique = format!(
            "csd-db-{}-{}-{}.sqlite",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn insert_query_and_rollback_status_round_trip() {
        let db_path = temp_db_path("rt");
        let db = MigrationDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let id = db
            .insert_migration(r"C:\Users\test\foo", r"D:\target\foo", "Junction", 12345)
            .unwrap();
        assert!(id > 0);

        let record = db.get_migration_by_id(id).unwrap().expect("record exists");
        assert_eq!(record.source_path, r"C:\Users\test\foo");
        assert_eq!(record.target_path, r"D:\target\foo");
        assert_eq!(record.link_type, "Junction");
        assert_eq!(record.file_size, 12345);
        assert_eq!(record.status, "active");

        // 回滚状态更新
        db.update_status(id, "rolled_back").unwrap();
        let record = db.get_migration_by_id(id).unwrap().unwrap();
        assert_eq!(record.status, "rolled_back");

        // 清理
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    }

    #[test]
    fn stats_count_active_and_rolled_back_correctly() {
        let db_path = temp_db_path("stats");
        let db = MigrationDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let id1 = db.insert_migration("a", "b", "Junction", 100).unwrap();
        let _id2 = db.insert_migration("c", "d", "Symlink", 200).unwrap();
        let id3 = db.insert_migration("e", "f", "Hardlink", 50).unwrap();

        db.update_status(id1, "rolled_back").unwrap();
        db.update_status(id3, "rolled_back").unwrap();

        let stats = db.get_stats().unwrap();
        assert_eq!(stats.total_count, 3);
        assert_eq!(stats.total_size, 350);
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.rolled_back_count, 2);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    }
}
