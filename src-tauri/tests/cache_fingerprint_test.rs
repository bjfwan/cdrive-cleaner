//! 深度扫描缓存指纹与完整性测试。
//!
//! 这些用例只跑数据库 + 指纹结构，不依赖真实磁盘扫描，秒级完成。
//!
//! 覆盖：
//! - 提权状态变化 → `env_fingerprint_changed`，第二次读取必须 miss。
//! - 卷序列号变化 → 第二次读取必须 miss。
//! - 写入占位但还没 mark_completed（模拟进程崩溃） → 下次读取返回 None。
//! - 老记录（无 fingerprint 字段、cache_schema_version=0） → 一律不命中。

#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::database::ScanCacheDb;
    use cdrive_cleaner_lib::scanner::env_fingerprint::{
        EnvFingerprint, CACHE_SCHEMA_VERSION,
    };
    use std::path::PathBuf;

    fn temp_db_path(label: &str) -> PathBuf {
        let unique = format!(
            "csd-fp-{}-{}-{}.sqlite",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    }

    fn baseline_fp() -> EnvFingerprint {
        EnvFingerprint {
            volume_serial: Some(0xDEAD_BEEF),
            file_system: "NTFS".to_string(),
            is_elevated: false,
            user_sid: Some("S-1-5-21-1234567890-987654321-555".to_string()),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            rule_version: 1,
        }
    }

    fn save_with_fp(
        db: &ScanCacheDb,
        disk_path: &str,
        fp: &EnvFingerprint,
        completed: bool,
        result_json: &str,
    ) {
        let fp_json = serde_json::to_string(fp).unwrap();
        db.save_scan_result(
            disk_path,
            "deep",
            result_json,
            10,
            1024,
            &fp_json,
            completed,
        )
        .unwrap();
    }

    #[test]
    fn elevation_change_invalidates_cache() {
        let db_path = temp_db_path("elev");
        let db = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let fp_standard = EnvFingerprint {
            is_elevated: false,
            ..baseline_fp()
        };
        let fp_elevated = EnvFingerprint {
            is_elevated: true,
            ..baseline_fp()
        };

        save_with_fp(&db, "C:\\", &fp_standard, false, "{}");
        db.mark_scan_completed("C:\\", "deep").unwrap();

        // 同指纹一定能命中。
        let hit = db
            .get_valid_scan_result("C:\\", "deep", &fp_standard)
            .unwrap();
        assert!(hit.is_some(), "same fingerprint should hit cache");

        // 提权后再读：env_fingerprint_changed → miss。
        let miss = db
            .get_valid_scan_result("C:\\", "deep", &fp_elevated)
            .unwrap();
        assert!(
            miss.is_none(),
            "elevation flip should invalidate cache (env_fingerprint_changed)"
        );

        // 第二次写覆盖后用新指纹应该命中。
        save_with_fp(&db, "C:\\", &fp_elevated, false, "{}");
        db.mark_scan_completed("C:\\", "deep").unwrap();
        assert!(db
            .get_valid_scan_result("C:\\", "deep", &fp_elevated)
            .unwrap()
            .is_some());
        assert!(db
            .get_valid_scan_result("C:\\", "deep", &fp_standard)
            .unwrap()
            .is_none());

        cleanup(&db_path);
    }

    #[test]
    fn volume_serial_change_invalidates_cache() {
        let db_path = temp_db_path("vol");
        let db = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let fp_a = EnvFingerprint {
            volume_serial: Some(0x1111_1111),
            ..baseline_fp()
        };
        let fp_b = EnvFingerprint {
            volume_serial: Some(0x2222_2222),
            ..baseline_fp()
        };

        save_with_fp(&db, "D:\\", &fp_a, false, "{}");
        db.mark_scan_completed("D:\\", "deep").unwrap();

        assert!(db
            .get_valid_scan_result("D:\\", "deep", &fp_a)
            .unwrap()
            .is_some());

        let miss = db.get_valid_scan_result("D:\\", "deep", &fp_b).unwrap();
        assert!(
            miss.is_none(),
            "different volume_serial should invalidate cache"
        );

        // None volume_serial 也算不同。
        let fp_unknown = EnvFingerprint {
            volume_serial: None,
            ..baseline_fp()
        };
        assert!(db
            .get_valid_scan_result("D:\\", "deep", &fp_unknown)
            .unwrap()
            .is_none());

        cleanup(&db_path);
    }

    #[test]
    fn placeholder_record_without_mark_complete_is_invisible() {
        let db_path = temp_db_path("placeholder");
        let db = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let fp = baseline_fp();
        // 模拟"进程刚 INSERT 占位记录就被 kill"：completed=false 且永远不会 mark_completed。
        save_with_fp(&db, "E:\\", &fp, false, "{}");

        let result = db.get_valid_scan_result("E:\\", "deep", &fp).unwrap();
        assert!(
            result.is_none(),
            "scan_completed=0 must be treated as cache miss"
        );

        // 即便指纹一致，没有 mark_completed 都算脏。
        // 转正之后立刻能读。
        db.mark_scan_completed("E:\\", "deep").unwrap();
        assert!(db
            .get_valid_scan_result("E:\\", "deep", &fp)
            .unwrap()
            .is_some());

        cleanup(&db_path);
    }

    #[test]
    fn legacy_record_without_fingerprint_is_invisible() {
        let db_path = temp_db_path("legacy");
        let db = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        // 直接走 sqlite 模拟"老缓存"：result_json 不带新字段，env_fingerprint=NULL，
        // cache_schema_version=0。新版 ScanCacheDb 必须把它视为 miss。
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO scan_cache (
                 disk_path, scan_type, result_json, file_count, total_size, created_at,
                 env_fingerprint, scan_completed, cache_schema_version
             ) VALUES ('F:\\', 'deep', '{}', 0, 0, CURRENT_TIMESTAMP, NULL, 1, 0)",
            [],
        )
        .unwrap();
        drop(conn);

        let fp = baseline_fp();
        let result = db.get_valid_scan_result("F:\\", "deep", &fp).unwrap();
        assert!(
            result.is_none(),
            "legacy record (cache_schema_version=0, no fingerprint) must miss"
        );

        // 裸 get_scan_result 还是能拿到原始行，便于做诊断/迁移。
        let raw = db.get_scan_result("F:\\", "deep").unwrap();
        assert!(raw.is_some(), "raw read should still surface the legacy row");
        let raw = raw.unwrap();
        assert_eq!(raw.cache_schema_version, 0);
        assert!(raw.env_fingerprint_json.is_none());

        cleanup(&db_path);
    }

    #[test]
    fn schema_version_mismatch_invalidates_cache() {
        let db_path = temp_db_path("schema");
        let db = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();

        let fp = baseline_fp();
        let fp_json = serde_json::to_string(&fp).unwrap();
        // 写一条 schema_version 比当前低的记录（通过裸 sqlite 模拟）。
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO scan_cache (
                 disk_path, scan_type, result_json, file_count, total_size, created_at,
                 env_fingerprint, scan_completed, cache_schema_version
             ) VALUES ('G:\\', 'deep', '{}', 0, 0, CURRENT_TIMESTAMP, ?1, 1, 0)",
            rusqlite::params![fp_json],
        )
        .unwrap();
        drop(conn);

        assert!(
            CACHE_SCHEMA_VERSION >= 1,
            "test assumes current schema_version >= 1"
        );
        let result = db.get_valid_scan_result("G:\\", "deep", &fp).unwrap();
        assert!(
            result.is_none(),
            "cache_schema_version=0 < current must miss"
        );

        cleanup(&db_path);
    }

    #[test]
    fn migration_adds_new_columns_idempotently() {
        // 用一份空数据库实例化两次：第二次实例化不应该 panic（ALTER TABLE 已经做过）。
        let db_path = temp_db_path("mig");
        {
            let _db1 = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();
        }
        {
            let _db2 = ScanCacheDb::new(db_path.to_string_lossy().as_ref()).unwrap();
        }
        cleanup(&db_path);
    }
}
