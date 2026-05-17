use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone, Serialize)]
pub struct DiskSnapshot {
    pub id: i64,
    pub drive_letter: String,
    pub total_size: i64,
    pub used_size: i64,
    pub scanned_size: i64,
    pub captured_at: String,
}

#[derive(Clone)]
pub struct SpaceHistoryDb {
    conn: Arc<Mutex<Connection>>,
}

impl SpaceHistoryDb {
    /// Open or create the disk snapshots database.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS disk_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                drive_letter TEXT NOT NULL,
                total_size INTEGER NOT NULL,
                used_size INTEGER NOT NULL,
                scanned_size INTEGER NOT NULL,
                captured_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_disk_snapshots_drive
                ON disk_snapshots(drive_letter, captured_at);
            ",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record a snapshot of disk space metrics for a drive.
    pub fn record_snapshot(
        &self,
        drive: &str,
        total: u64,
        used: u64,
        scanned: u64,
    ) -> Result<i64> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO disk_snapshots (drive_letter, total_size, used_size, scanned_size)
             VALUES (?1, ?2, ?3, ?4)",
            params![drive, total as i64, used as i64, scanned as i64],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Return snapshots captured within the last `days` days, oldest first.
    pub fn get_history(&self, drive: &str, days: u32) -> Result<Vec<DiskSnapshot>> {
        let conn = self.lock_conn();
        let cutoff_clause = format!("-{} days", days.max(1));
        let mut stmt = conn.prepare(
            "SELECT id, drive_letter, total_size, used_size, scanned_size, captured_at
             FROM disk_snapshots
             WHERE drive_letter = ?1 AND captured_at >= datetime('now', ?2)
             ORDER BY captured_at ASC",
        )?;
        let rows = stmt
            .query_map(params![drive, cutoff_clause], |row| {
                Ok(DiskSnapshot {
                    id: row.get(0)?,
                    drive_letter: row.get(1)?,
                    total_size: row.get(2)?,
                    used_size: row.get(3)?,
                    scanned_size: row.get(4)?,
                    captured_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Delete snapshots older than `days` days to keep the table bounded.
    pub fn purge_old(&self, days: u32) -> Result<usize> {
        let conn = self.lock_conn();
        let cutoff_clause = format!("-{} days", days.max(1));
        let removed = conn.execute(
            "DELETE FROM disk_snapshots WHERE captured_at < datetime('now', ?1)",
            params![cutoff_clause],
        )?;
        Ok(removed)
    }
}
