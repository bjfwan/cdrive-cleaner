use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedScanResult {
    pub id: i64,
    pub disk_path: String,
    pub scan_type: String,
    pub result_json: String,
    pub created_at: String,
    pub file_count: i64,
    pub total_size: i64,
}

#[derive(Clone)]
pub struct ScanCacheDb {
    conn: Arc<Mutex<Connection>>,
}

impl ScanCacheDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("
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
        ")?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn save_scan_result(&self, disk_path: &str, scan_type: &str, result_json: &str, file_count: i64, total_size: i64) -> Result<i64> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO scan_cache (disk_path, scan_type, result_json, file_count, total_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
             ON CONFLICT(disk_path, scan_type) DO UPDATE SET
                 result_json = excluded.result_json,
                 file_count = excluded.file_count,
                 total_size = excluded.total_size,
                 created_at = CURRENT_TIMESTAMP",
            params![disk_path, scan_type, result_json, file_count, total_size],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_scan_result(&self, disk_path: &str, scan_type: &str) -> Result<Option<CachedScanResult>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, disk_path, scan_type, result_json, created_at, file_count, total_size FROM scan_cache WHERE disk_path = ?1 AND scan_type = ?2"
        )?;
        let mut rows = stmt.query(params![disk_path, scan_type])?;
        match rows.next()? {
            Some(row) => Ok(Some(CachedScanResult {
                id: row.get(0)?, disk_path: row.get(1)?, scan_type: row.get(2)?,
                result_json: row.get(3)?, created_at: row.get(4)?,
                file_count: row.get(5)?, total_size: row.get(6)?,
            })),
            None => Ok(None),
        }
    }

    pub fn clear_all(&self) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute("DELETE FROM scan_cache", [])?;
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
            "SELECT disk_path, scan_type, file_count, total_size, datetime(created_at, 'localtime'), LENGTH(result_json)
             FROM scan_cache
             ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })?.collect::<Result<Vec<_>, _>>()?;
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
