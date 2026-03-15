use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

/// Managed state: the app data directory
pub struct AppDataDir(pub PathBuf);

/// Open (or create) the SQLite database and run migrations.
pub fn init_db(app_dir: &std::path::Path) -> Result<()> {
    let db_path = app_dir.join("aura.db");
    let conn = Connection::open(&db_path)?;
    run_migrations(&conn)?;
    Ok(())
}

/// Open a connection to the database (used by commands to get a fresh handle).
pub fn open(app_dir: &std::path::Path) -> Result<Connection> {
    let db_path = app_dir.join("aura.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    run_migrations(&conn)?;
    Ok(conn)
}

pub(crate) fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );

        INSERT OR IGNORE INTO schema_version (version) VALUES (0);
    ",
    )?;

    let version: i64 =
        conn.query_row("SELECT version FROM schema_version", [], |row| row.get(0))?;

    if version < 1 {
        migrate_v1(conn)?;
        conn.execute("UPDATE schema_version SET version = 1", [])?;
    }

    if version < 2 {
        migrate_v2(conn)?;
        conn.execute("UPDATE schema_version SET version = 2", [])?;
    }

    if version < 3 {
        migrate_v3(conn)?;
        conn.execute("UPDATE schema_version SET version = 3", [])?;
    }

    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id              TEXT PRIMARY KEY,
            title           TEXT,
            started_at      TEXT NOT NULL,  -- ISO8601
            ended_at        TEXT,
            status          TEXT NOT NULL DEFAULT 'recording',
            duration_secs   REAL,
            notes_path      TEXT,           -- path to .md file
            transcript_path TEXT            -- path to .txt file
        );

        CREATE TABLE IF NOT EXISTS transcript_chunks (
            id              TEXT PRIMARY KEY,
            session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            speaker_label   TEXT,
            timestamp_secs  REAL NOT NULL,
            text            TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS speakers (
            id              TEXT PRIMARY KEY,
            inferred_name   TEXT,
            created_at      TEXT NOT NULL
            -- voice_embedding_blob will be added in a later migration (P2)
        );

        CREATE TABLE IF NOT EXISTS settings (
            key     TEXT PRIMARY KEY,
            value   TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_chunks_session ON transcript_chunks(session_id);
    ",
    )?;
    Ok(())
}

fn migrate_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        ALTER TABLE sessions ADD COLUMN error_message TEXT;
    ",
    )?;
    Ok(())
}

fn migrate_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS speaker_labels (
            id              TEXT PRIMARY KEY,
            display_name    TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            notes           TEXT
        );

        CREATE TABLE IF NOT EXISTS speaker_mappings (
            id              TEXT PRIMARY KEY,
            session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            raw_label       TEXT NOT NULL,
            speaker_label_id TEXT NOT NULL REFERENCES speaker_labels(id) ON DELETE CASCADE,
            UNIQUE(session_id, raw_label)
        );

        CREATE INDEX IF NOT EXISTS idx_speaker_mappings_session ON speaker_mappings(session_id);
    ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn migrations_reach_latest_version() {
        let conn = in_memory_db();
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            version, 3,
            "schema_version should be at the latest migration"
        );
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = in_memory_db();
        // Running again must not fail or regress the version
        run_migrations(&conn).unwrap();
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn required_tables_exist() {
        let conn = in_memory_db();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for expected in &[
            "sessions",
            "transcript_chunks",
            "settings",
            "schema_version",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table: {expected}"
            );
        }
    }

    #[test]
    fn speaker_tables_exist_after_v3() {
        let conn = in_memory_db();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(tables.contains(&"speaker_labels".to_string()));
        assert!(tables.contains(&"speaker_mappings".to_string()));
    }

    #[test]
    fn sessions_table_has_error_message_column() {
        let conn = in_memory_db();
        // Migration v2 adds this column; verify it exists by inserting a value
        conn.execute(
            "INSERT INTO sessions (id, title, started_at, status, error_message)
             VALUES ('x', NULL, '2024-01-01', 'error', 'boom')",
            [],
        )
        .unwrap();
        let msg: String = conn
            .query_row("SELECT error_message FROM sessions WHERE id='x'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(msg, "boom");
    }
}
