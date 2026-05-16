use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Serialize, Clone)]
pub struct MigrationRecord {
    pub id: i64,
    pub source_path: String,
    pub target_path: String,
    pub link_type: String,
    pub file_size: u64,
    pub created_at: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct MigrationStats {
    pub total_count: i64,
    pub total_size: u64,
    pub active_count: i64,
    pub rolled_back_count: i64,
}

#[derive(Clone)]
pub struct MigrationDb {
    conn: Arc<Mutex<Connection>>,
}

impl MigrationDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS migrations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                link_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                status TEXT NOT NULL DEFAULT 'active'
            );
        ",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert_migration(
        &self,
        source_path: &str,
        target_path: &str,
        link_type: &str,
        file_size: u64,
    ) -> Result<i64> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO migrations (source_path, target_path, link_type, file_size) VALUES (?1, ?2, ?3, ?4)",
            [source_path, target_path, link_type, &file_size.to_string()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_all_migrations(&self) -> Result<Vec<MigrationRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, source_path, target_path, link_type, file_size, created_at, status FROM migrations ORDER BY created_at DESC"
        )?;
        let records = stmt
            .query_map([], |row| {
                Ok(MigrationRecord {
                    id: row.get(0)?,
                    source_path: row.get(1)?,
                    target_path: row.get(2)?,
                    link_type: row.get(3)?,
                    file_size: row.get::<_, i64>(4)? as u64,
                    created_at: row.get(5)?,
                    status: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn get_migration_by_id(&self, id: i64) -> Result<Option<MigrationRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, source_path, target_path, link_type, file_size, created_at, status FROM migrations WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(MigrationRecord {
                id: row.get(0)?,
                source_path: row.get(1)?,
                target_path: row.get(2)?,
                link_type: row.get(3)?,
                file_size: row.get::<_, i64>(4)? as u64,
                created_at: row.get(5)?,
                status: row.get(6)?,
            })),
            None => Ok(None),
        }
    }

    pub fn update_status(&self, id: i64, status: &str) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE migrations SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    pub fn get_stats(&self) -> Result<MigrationStats> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*), SUM(file_size), SUM(CASE WHEN status='active' THEN 1 ELSE 0 END), SUM(CASE WHEN status='rolled_back' THEN 1 ELSE 0 END) FROM migrations"
        )?;
        stmt.query_row([], |row| {
            Ok(MigrationStats {
                total_count: row.get(0)?,
                total_size: row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u64,
                // 空表时 SUM(CASE ...) 返回 NULL，必须用 Option 接
                active_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                rolled_back_count: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            })
        })
        .map_err(Into::into)
    }
}
