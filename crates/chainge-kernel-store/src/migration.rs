//! Database schema migrations for SQLite.
//!
//! We use a simple versioned migration system. Each migration is a SQL string
//! that transforms the schema from version N to N+1.

use rusqlite::Connection;

use crate::error::{Result, StoreError};

/// Current schema version.
pub const CURRENT_VERSION: u32 = 1;

/// Initialize or migrate the database schema.
///
/// This function is idempotent - it can be called multiple times safely.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Get current version
    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Apply migrations
    if current < CURRENT_VERSION {
        let tx = conn.transaction()?;

        for version in (current + 1)..=CURRENT_VERSION {
            apply_migration(&tx, version)?;

            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![version, now_millis()],
            )?;
        }

        tx.commit()?;
    }

    Ok(())
}

/// Apply a specific migration version.
fn apply_migration(conn: &Connection, version: u32) -> Result<()> {
    match version {
        1 => apply_v1(conn),
        _ => Err(StoreError::Migration(format!(
            "unknown migration version: {}",
            version
        ))),
    }
}

/// Migration v1: Initial schema.
fn apply_v1(conn: &Connection) -> Result<()> {
    // Core receipt storage
    conn.execute_batch(
        r#"
        -- Receipts table: stores all receipts
        CREATE TABLE receipts (
            receipt_id BLOB PRIMARY KEY,      -- 32 bytes, Blake3 hash of canonical bytes
            stream_id BLOB NOT NULL,          -- 32 bytes
            seq INTEGER NOT NULL,             -- sequence number within stream
            author BLOB NOT NULL,             -- 32 bytes, Ed25519 public key
            timestamp INTEGER NOT NULL,       -- author-claimed timestamp (Unix ms)
            kind INTEGER NOT NULL,            -- ReceiptKind as u16
            prev_receipt_id BLOB,             -- 32 bytes, nullable (None for seq=1)
            refs BLOB NOT NULL,               -- CBOR array of receipt_ids
            payload_hash BLOB NOT NULL,       -- 32 bytes, Blake3 hash of payload
            payload BLOB NOT NULL,            -- raw payload bytes
            signature BLOB NOT NULL,          -- 64 bytes, Ed25519 signature
            canonical_bytes BLOB NOT NULL,    -- cached canonical encoding
            ingested_at INTEGER NOT NULL,     -- local timestamp of ingestion
            verified INTEGER NOT NULL DEFAULT 0,  -- 0=unverified, 1=valid, -1=invalid

            UNIQUE(stream_id, seq)
        );

        -- Stream state tracking
        CREATE TABLE streams (
            stream_id BLOB PRIMARY KEY,
            author BLOB NOT NULL,
            stream_name TEXT NOT NULL,
            head_seq INTEGER NOT NULL DEFAULT 0,
            head_receipt_id BLOB,
            known_max_seq INTEGER NOT NULL DEFAULT 0,
            state_hash BLOB,
            health INTEGER NOT NULL DEFAULT 0,  -- 0=healthy, 1=gaps, 2=forked
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Gap tracking for sync protocol
        CREATE TABLE stream_gaps (
            stream_id BLOB NOT NULL,
            missing_seq INTEGER NOT NULL,
            requested_at INTEGER,              -- when we last requested this
            PRIMARY KEY (stream_id, missing_seq)
        );

        -- Fork evidence
        CREATE TABLE forks (
            stream_id BLOB NOT NULL,
            seq INTEGER NOT NULL,
            receipt_id BLOB NOT NULL,
            detected_at INTEGER NOT NULL,
            PRIMARY KEY (stream_id, seq, receipt_id)
        );

        -- Indexes for common queries
        CREATE INDEX idx_receipts_stream_seq ON receipts(stream_id, seq);
        CREATE INDEX idx_receipts_author ON receipts(author);
        CREATE INDEX idx_receipts_kind ON receipts(kind);
        CREATE INDEX idx_receipts_timestamp ON receipts(timestamp);
        CREATE INDEX idx_receipts_ingested ON receipts(ingested_at);
        "#,
    )?;

    Ok(())
}

/// Get current time in milliseconds.
fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_creates_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"receipts".to_string()));
        assert!(tables.contains(&"streams".to_string()));
        assert!(tables.contains(&"stream_gaps".to_string()));
        assert!(tables.contains(&"forks".to_string()));
        assert!(tables.contains(&"schema_migrations".to_string()));
    }

    #[test]
    fn test_migration_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap(); // Should not error
        migrate(&mut conn).unwrap(); // Still should not error

        // Verify version is 1
        let version: u32 = conn
            .query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 1);
    }
}
