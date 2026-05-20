use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::scanner::env_fingerprint::{EnvFingerprint, CACHE_SCHEMA_VERSION};

/// 当前 SQLite schema 版本号，与 `PRAGMA user_version` 对齐。
/// - v0：原始 schema
/// - v1：新增 `env_fingerprint` / `scan_completed` / `cache_schema_version` 三列
/// - v2：新增 `result_blob` 列（bincode 序列化，替代 JSON 提升性能）
pub const DB_USER_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedScanResult {
    pub id: i64,
    pub disk_path: String,
    pub scan_type: String,
    pub result_json: String,
    pub created_at: String,
    pub file_count: i64,
    pub total_size: i64,
    /// 缓存里持久化的环境指纹 JSON。老记录可能是 NULL/空串。
    pub env_fingerprint_json: Option<String>,
    /// `true` 表示扫描已成功完成；`false` 是占位/中断状态。
    pub scan_completed: bool,
    /// 写入时的 schema 版本号；老记录是 0。
    pub cache_schema_version: u32,
    /// bincode 序列化的扫描结果。新写入的缓存使用此字段；老记录为 None。
    pub result_blob: Option<Vec<u8>>,
}

impl CachedScanResult {
    /// 从缓存记录反序列化 `ScanResult`。优先使用 `result_blob`（bincode），
    /// 回退到 `result_json`（JSON，兼容老缓存）。
    pub fn deserialize_result<T: DeserializeOwned>(&self) -> Result<T> {
        if let Some(ref blob) = self.result_blob {
            match bincode::deserialize(blob) {
                Ok(result) => return Ok(result),
                Err(err) if !self.result_json.is_empty() => {
                    tracing::warn!("[scan-cache] bincode deserialize failed id={} disk_path={} scan_type={} blob_bytes={} err={}; falling back to json", self.id, self.disk_path, self.scan_type, blob.len(), err);
                }
                Err(err) => return Err(anyhow::anyhow!("bincode: {err}")),
            }
        }
        serde_json::from_str(&self.result_json).map_err(|e| anyhow::anyhow!("json: {e}"))
    }
}

#[derive(Clone)]
pub struct ScanCacheDb {
    conn: Arc<Mutex<Connection>>,
}

impl ScanCacheDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            PRAGMA cache_size=-8000;
            CREATE TABLE IF NOT EXISTS scan_cache (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                disk_path TEXT NOT NULL,
                scan_type TEXT NOT NULL,
                result_json TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                file_count INTEGER NOT NULL,
                total_size INTEGER NOT NULL,
                UNIQUE(disk_path, scan_type)
            );
            CREATE INDEX IF NOT EXISTS idx_disk_path ON scan_cache(disk_path);
        ",
        )?;
        Self::run_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        let current: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if current >= DB_USER_VERSION {
            return Ok(());
        }
        // v0 -> v1：补充指纹相关三列。ALTER TABLE ADD COLUMN 在 SQLite 里是幂等
        // 失败的（重复加列会报错），所以先看看列在不在。
        if current < 1 {
            ensure_column(conn, "scan_cache", "env_fingerprint", "TEXT")?;
            ensure_column(
                conn,
                "scan_cache",
                "scan_completed",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_column(
                conn,
                "scan_cache",
                "cache_schema_version",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        // v1 -> v2：新增 result_blob 列（bincode 序列化）
        if current < 2 {
            ensure_column(conn, "scan_cache", "result_blob", "BLOB")?;
        }
        conn.execute_batch(&format!("PRAGMA user_version = {DB_USER_VERSION};"))?;
        Ok(())
    }

    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 写入扫描结果。
    ///
    /// `scan_completed=false` 时是"占位"记录（扫描尚未真正完成）；调用方应当在
    /// 数据真正落盘后再调 [`Self::mark_scan_completed`] 把它置为 `true`，从而
    /// 让下次启动看到中断态记录直接判脏。
    pub fn save_scan_result(
        &self,
        disk_path: &str,
        scan_type: &str,
        result_json: &str,
        file_count: i64,
        total_size: i64,
        env_fingerprint_json: &str,
        scan_completed: bool,
    ) -> Result<i64> {
        let conn = self.lock_conn();
        let completed = if scan_completed { 1 } else { 0 };
        // 尝试将 JSON 转为 bincode blob 以获得更好的读取性能。
        // 如果 result_json 为空或解析失败（占位记录），blob 为 NULL，保留 json 原样。
        let blob: Option<Vec<u8>> = if result_json.is_empty() || result_json == "{}" {
            None
        } else {
            use crate::scanner::file_info::ScanResult;
            let sr = serde_json::from_str::<ScanResult>(result_json)
                .map_err(|e| anyhow::anyhow!("scan cache json self-check failed: {e}"))?;
            bincode::serialize(&sr).ok().and_then(|blob| {
                let _: ScanResult = bincode::deserialize(&blob).ok()?;
                Some(blob)
            })
        };
        let stored_json = result_json;
        conn.execute(
            "INSERT INTO scan_cache (
                 disk_path, scan_type, result_json, file_count, total_size, created_at,
                 env_fingerprint, scan_completed, cache_schema_version, result_blob
             ) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, ?6, ?7, ?8, ?9)
             ON CONFLICT(disk_path, scan_type) DO UPDATE SET
                 result_json = excluded.result_json,
                 file_count = excluded.file_count,
                 total_size = excluded.total_size,
                 created_at = CURRENT_TIMESTAMP,
                 env_fingerprint = excluded.env_fingerprint,
                 scan_completed = excluded.scan_completed,
                 cache_schema_version = excluded.cache_schema_version,
                 result_blob = excluded.result_blob",
            params![
                disk_path,
                scan_type,
                stored_json,
                file_count,
                total_size,
                env_fingerprint_json,
                completed,
                CACHE_SCHEMA_VERSION as i64,
                blob,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 把已经写入的占位记录标记为完成态。如果缓存条目不存在就什么都不做。
    pub fn mark_scan_completed(&self, disk_path: &str, scan_type: &str) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE scan_cache
             SET scan_completed = 1,
                 created_at = CURRENT_TIMESTAMP
             WHERE disk_path = ?1 AND scan_type = ?2",
            params![disk_path, scan_type],
        )?;
        Ok(())
    }

    /// 不做指纹/完成态校验的裸读，给迁移、调试、诊断脚本和老的 `get_scan_cache`
    /// Tauri 命令用。新代码请走 [`Self::get_valid_scan_result`]。
    pub fn get_scan_result(
        &self,
        disk_path: &str,
        scan_type: &str,
    ) -> Result<Option<CachedScanResult>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, disk_path, scan_type, result_json, created_at, file_count, total_size,
                    env_fingerprint, scan_completed, cache_schema_version, result_blob
             FROM scan_cache
             WHERE disk_path = ?1 AND scan_type = ?2",
        )?;
        let mut rows = stmt.query(params![disk_path, scan_type])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_cached(row)?)),
            None => Ok(None),
        }
    }

    /// 严格读：只有当扫描完成、schema 版本匹配、环境指纹完全相等时才返回。
    /// 任何不匹配都返回 `None`。
    pub fn get_valid_scan_result(
        &self,
        disk_path: &str,
        scan_type: &str,
        current_fp: &EnvFingerprint,
    ) -> Result<Option<CachedScanResult>> {
        let Some(record) = self.get_scan_result(disk_path, scan_type)? else {
            return Ok(None);
        };
        if !record.scan_completed {
            return Ok(None);
        }
        if record.cache_schema_version != CACHE_SCHEMA_VERSION {
            return Ok(None);
        }
        let cached_fp = match record.env_fingerprint_json.as_deref() {
            Some(json) if !json.is_empty() => match serde_json::from_str::<EnvFingerprint>(json) {
                Ok(fp) => fp,
                Err(_) => return Ok(None),
            },
            _ => return Ok(None),
        };
        if &cached_fp != current_fp {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn clear_all(&self) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute("DELETE FROM scan_cache", [])?;
        Ok(())
    }

    pub fn purge_legacy_scan_types(&self) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute("DELETE FROM scan_cache WHERE scan_type <> 'deep'", [])?;
        Ok(())
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute_batch("VACUUM")?;
        Ok(())
    }

    pub fn get_all_entries(&self) -> Result<Vec<(String, String, i64, i64, String, i64)>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT disk_path, scan_type, file_count, total_size, datetime(created_at, 'localtime'), COALESCE(LENGTH(result_blob), LENGTH(result_json))
             FROM scan_cache
             WHERE scan_type = 'deep'
             ORDER BY created_at DESC"
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_entry(&self, disk_path: &str, scan_type: &str) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "DELETE FROM scan_cache WHERE disk_path = ?1 AND scan_type = ?2",
            params![disk_path, scan_type],
        )?;
        Ok(())
    }
}

fn row_to_cached(row: &rusqlite::Row<'_>) -> rusqlite::Result<CachedScanResult> {
    let scan_completed_int: i64 = row.get(8)?;
    let cache_schema_version_int: i64 = row.get(9)?;
    Ok(CachedScanResult {
        id: row.get(0)?,
        disk_path: row.get(1)?,
        scan_type: row.get(2)?,
        result_json: row.get(3)?,
        created_at: row.get(4)?,
        file_count: row.get(5)?,
        total_size: row.get(6)?,
        env_fingerprint_json: row.get::<_, Option<String>>(7)?,
        scan_completed: scan_completed_int != 0,
        cache_schema_version: cache_schema_version_int.max(0) as u32,
        result_blob: row.get::<_, Option<Vec<u8>>>(10)?,
    })
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<()> {
    let exists: bool = conn
        .prepare(&format!("PRAGMA table_info({table});"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .any(|name| name.eq_ignore_ascii_case(column));
    if exists {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {column_def};"
    ))?;
    Ok(())
}
