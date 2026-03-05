use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Serialize, Deserialize};

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

pub struct ScanCacheDb {
    pub conn: Connection,
}

impl ScanCacheDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS scan_cache (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                disk_path TEXT NOT NULL,
                scan_type TEXT NOT NULL,
                result_json TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                file_count INTEGER NOT NULL,
                total_size INTEGER NOT NULL,
                UNIQUE(disk_path, scan_type)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_disk_path ON scan_cache(disk_path)",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn save_scan_result(
        &self,
        disk_path: &str,
        scan_type: &str,
        result_json: &str,
        file_count: i64,
        total_size: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT OR REPLACE INTO scan_cache (disk_path, scan_type, result_json, file_count, total_size) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![disk_path, scan_type, result_json, file_count, total_size],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_scan_result(&self, disk_path: &str, scan_type: &str) -> Result<Option<CachedScanResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, disk_path, scan_type, result_json, created_at, file_count, total_size 
             FROM scan_cache 
             WHERE disk_path = ?1 AND scan_type = ?2"
        )?;

        let mut rows = stmt.query(params![disk_path, scan_type])?;
        
        if let Some(row) = rows.next()? {
            Ok(Some(CachedScanResult {
                id: row.get(0)?,
                disk_path: row.get(1)?,
                scan_type: row.get(2)?,
                result_json: row.get(3)?,
                created_at: row.get(4)?,
                file_count: row.get(5)?,
                total_size: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_old_cache(&self, days: i64) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM scan_cache WHERE created_at < datetime('now', '-' || ?1 || ' days')",
            params![days],
        )?;
        Ok(deleted)
    }

    pub fn clear_all(&self) -> Result<()> {
        self.conn.execute("DELETE FROM scan_cache", [])?;
        Ok(())
    }

    pub fn get_cache_stats(&self) -> Result<(i64, i64)> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(result_json)), 0) FROM scan_cache"
        )?;
        
        let (count, size) = stmt.query_row([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        
        Ok((count, size))
    }
}
