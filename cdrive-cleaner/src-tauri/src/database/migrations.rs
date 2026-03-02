use anyhow::Result;
use rusqlite::Connection;

pub struct MigrationDb {
    conn: Connection,
}

impl MigrationDb {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS migrations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                link_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                status TEXT NOT NULL DEFAULT 'active'
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn insert_migration(
        &self,
        source_path: &str,
        target_path: &str,
        link_type: &str,
        file_size: u64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO migrations (source_path, target_path, link_type, file_size) VALUES (?1, ?2, ?3, ?4)",
            [source_path, target_path, link_type, &file_size.to_string()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}
