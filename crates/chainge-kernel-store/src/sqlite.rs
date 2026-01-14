//! SQLite implementation of the Store trait.
//!
//! This is the primary storage backend for the Chainge Kernel. It uses
//! rusqlite with bundled SQLite, wrapped in async via tokio::spawn_blocking.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use rusqlite::{params, Connection, OptionalExtension};

use chainge_kernel_core::{
    canonical_bytes, Blake3Hash, Ed25519PublicKey, Ed25519Signature, Receipt, ReceiptHeader,
    ReceiptId, ReceiptKind, StreamHealth, StreamId, StreamState,
};

use crate::error::{Result, StoreError};
use crate::migration;
use crate::traits::{Fork, InsertResult, Store};

/// SQLite-based store implementation.
///
/// Thread-safe via internal Mutex. All operations use spawn_blocking
/// to avoid blocking the async runtime.
pub struct SqliteStore {
    /// The SQLite connection, protected by a mutex.
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open a SQLite database at the given path.
    ///
    /// Creates the file and runs migrations if it doesn't exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        migration::migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory SQLite database.
    ///
    /// Useful for testing.
    pub fn open_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migration::migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Execute a blocking operation on the connection.
    fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = self.conn.lock().map_err(|e| {
            StoreError::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                Some(format!("mutex poisoned: {}", e)),
            ))
        })?;
        f(&conn)
    }

    /// Execute a blocking operation that needs mutable access.
    fn with_conn_mut<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T>,
    {
        let mut conn = self.conn.lock().map_err(|e| {
            StoreError::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                Some(format!("mutex poisoned: {}", e)),
            ))
        })?;
        f(&mut conn)
    }
}

// Helper to convert a row to Receipt
fn row_to_receipt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Receipt> {
    let author_bytes: Vec<u8> = row.get("author")?;
    let stream_id_bytes: Vec<u8> = row.get("stream_id")?;
    let payload_hash_bytes: Vec<u8> = row.get("payload_hash")?;
    let prev_receipt_id_bytes: Option<Vec<u8>> = row.get("prev_receipt_id")?;
    let refs_cbor: Vec<u8> = row.get("refs")?;
    let payload: Vec<u8> = row.get("payload")?;
    let signature_bytes: Vec<u8> = row.get("signature")?;

    // Parse refs from CBOR
    let refs: Vec<ReceiptId> = if refs_cbor.is_empty() {
        Vec::new()
    } else {
        ciborium::from_reader(&refs_cbor[..]).unwrap_or_default()
    };

    let header = ReceiptHeader {
        version: row.get::<_, u8>("version").unwrap_or(0),
        author: Ed25519PublicKey(
            author_bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidColumnType(0, "author".into(), rusqlite::types::Type::Blob))?,
        ),
        stream_id: StreamId::from_bytes(
            stream_id_bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidColumnType(1, "stream_id".into(), rusqlite::types::Type::Blob))?,
        ),
        seq: row.get("seq")?,
        timestamp: row.get("timestamp")?,
        kind: ReceiptKind::from_u16(row.get::<_, u16>("kind")?).unwrap_or(ReceiptKind::Data),
        prev_receipt_id: prev_receipt_id_bytes.map(|b| {
            ReceiptId::from_bytes(
                b.try_into().unwrap_or([0u8; 32]),
            )
        }),
        refs,
        payload_hash: Blake3Hash(
            payload_hash_bytes
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidColumnType(7, "payload_hash".into(), rusqlite::types::Type::Blob))?,
        ),
    };

    let signature = Ed25519Signature(
        signature_bytes
            .try_into()
            .map_err(|_| rusqlite::Error::InvalidColumnType(8, "signature".into(), rusqlite::types::Type::Blob))?,
    );

    Ok(Receipt {
        header,
        payload: Bytes::from(payload),
        signature,
    })
}

// Helper to encode refs to CBOR
fn encode_refs(refs: &[ReceiptId]) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::into_writer(refs, &mut buf).unwrap_or_default();
    buf
}

#[async_trait]
impl Store for SqliteStore {
    async fn insert_receipt(&self, receipt: &Receipt, canonical: &[u8]) -> Result<InsertResult> {
        let receipt = receipt.clone();
        let canonical = canonical.to_vec();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let receipt_id = receipt.compute_id();
            let now = now_millis();

            // Check if receipt already exists by ID
            let existing_by_id: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT receipt_id FROM receipts WHERE receipt_id = ?1",
                    params![receipt_id.0.as_slice()],
                    |row| row.get(0),
                )
                .optional()?;

            if existing_by_id.is_some() {
                return Ok(InsertResult::AlreadyExists);
            }

            // Check if a different receipt exists at the same position
            let existing_at_pos: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT receipt_id FROM receipts WHERE stream_id = ?1 AND seq = ?2",
                    params![receipt.stream_id().as_bytes().as_slice(), receipt.seq()],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(existing_bytes) = existing_at_pos {
                let existing_id = ReceiptId::from_bytes(
                    existing_bytes.try_into().unwrap_or([0u8; 32]),
                );
                return Ok(InsertResult::Conflict { existing: existing_id });
            }

            // Insert the receipt
            let refs_cbor = encode_refs(&receipt.header.refs);

            conn.execute(
                "INSERT INTO receipts (
                    receipt_id, stream_id, seq, author, timestamp, kind,
                    prev_receipt_id, refs, payload_hash, payload, signature,
                    canonical_bytes, ingested_at, verified
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    receipt_id.0.as_slice(),
                    receipt.stream_id().as_bytes().as_slice(),
                    receipt.seq() as i64,
                    receipt.author().0.as_slice(),
                    receipt.header.timestamp,
                    receipt.kind().to_u16() as i64,
                    receipt.header.prev_receipt_id.as_ref().map(|id| id.0.as_slice()),
                    refs_cbor,
                    receipt.header.payload_hash.0.as_slice(),
                    receipt.payload.as_ref(),
                    receipt.signature.0.as_slice(),
                    canonical.as_slice(),
                    now,
                    1i64, // verified=1 (we validate before insert)
                ],
            )?;

            Ok(InsertResult::Inserted)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_receipt(&self, id: &ReceiptId) -> Result<Option<Receipt>> {
        let id = *id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            conn.query_row(
                "SELECT 0 as version, author, stream_id, seq, timestamp, kind, prev_receipt_id,
                        refs, payload_hash, payload, signature
                 FROM receipts WHERE receipt_id = ?1",
                params![id.0.as_slice()],
                row_to_receipt,
            )
            .optional()
            .map_err(StoreError::from)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_receipt_by_position(
        &self,
        stream_id: &StreamId,
        seq: u64,
    ) -> Result<Option<Receipt>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            conn.query_row(
                "SELECT 0 as version, author, stream_id, seq, timestamp, kind, prev_receipt_id,
                        refs, payload_hash, payload, signature
                 FROM receipts WHERE stream_id = ?1 AND seq = ?2",
                params![stream_id.as_bytes().as_slice(), seq as i64],
                row_to_receipt,
            )
            .optional()
            .map_err(StoreError::from)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_receipts_range(
        &self,
        stream_id: &StreamId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Receipt>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let mut stmt = conn.prepare(
                "SELECT 0 as version, author, stream_id, seq, timestamp, kind, prev_receipt_id,
                        refs, payload_hash, payload, signature
                 FROM receipts WHERE stream_id = ?1 AND seq >= ?2 AND seq <= ?3
                 ORDER BY seq",
            )?;

            let receipts = stmt
                .query_map(
                    params![stream_id.as_bytes().as_slice(), start as i64, end as i64],
                    row_to_receipt,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(receipts)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn has_receipt(&self, id: &ReceiptId) -> Result<bool> {
        let id = *id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM receipts WHERE receipt_id = ?1)",
                    params![id.0.as_slice()],
                    |row| row.get(0),
                )?;

            Ok(exists)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_canonical_bytes(&self, id: &ReceiptId) -> Result<Option<Vec<u8>>> {
        let id = *id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            conn.query_row(
                "SELECT canonical_bytes FROM receipts WHERE receipt_id = ?1",
                params![id.0.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_stream_state(&self, stream_id: &StreamId) -> Result<Option<StreamState>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            // Get stream state
            let row: Option<(Vec<u8>, String, i64, Option<Vec<u8>>, i64, Option<Vec<u8>>, i64, i64, i64)> = conn
                .query_row(
                    "SELECT author, stream_name, head_seq, head_receipt_id, known_max_seq,
                            state_hash, health, created_at, updated_at
                     FROM streams WHERE stream_id = ?1",
                    params![stream_id.as_bytes().as_slice()],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    },
                )
                .optional()?;

            let Some((author_bytes, stream_name, head_seq, head_receipt_id_bytes, known_max_seq, state_hash_bytes, health_int, created_at, updated_at)) = row else {
                return Ok(None);
            };

            // Get gaps
            let mut gaps_stmt = conn.prepare(
                "SELECT missing_seq FROM stream_gaps WHERE stream_id = ?1 ORDER BY missing_seq",
            )?;
            let gaps: BTreeSet<u64> = gaps_stmt
                .query_map(params![stream_id.as_bytes().as_slice()], |row| {
                    row.get::<_, i64>(0).map(|v| v as u64)
                })?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;

            // Convert health
            let health = match health_int {
                0 => StreamHealth::Healthy,
                1 => StreamHealth::HasGaps {
                    missing: gaps.iter().copied().collect(),
                },
                2 => {
                    // For forked, we'd need to query the forks table
                    // For now, return a placeholder
                    StreamHealth::Forked {
                        at_seq: 0,
                        receipts: vec![],
                    }
                }
                _ => StreamHealth::Healthy,
            };

            let state = StreamState {
                stream_id,
                author: Ed25519PublicKey(
                    author_bytes.try_into().unwrap_or([0u8; 32]),
                ),
                stream_name,
                head_seq: head_seq as u64,
                head_receipt_id: head_receipt_id_bytes.map(|b| {
                    ReceiptId::from_bytes(b.try_into().unwrap_or([0u8; 32]))
                }),
                known_max_seq: known_max_seq as u64,
                gaps,
                state_hash: state_hash_bytes.map(|b| Blake3Hash(b.try_into().unwrap_or([0u8; 32]))),
                health,
                created_at,
                updated_at,
            };

            Ok(Some(state))
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn upsert_stream_state(&self, state: &StreamState) -> Result<()> {
        let state = state.clone();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let tx = conn.transaction()?;

            // Convert health to int
            let health_int: i64 = match &state.health {
                StreamHealth::Healthy => 0,
                StreamHealth::HasGaps { .. } => 1,
                StreamHealth::Forked { .. } => 2,
            };

            // Upsert stream state
            tx.execute(
                "INSERT INTO streams (
                    stream_id, author, stream_name, head_seq, head_receipt_id,
                    known_max_seq, state_hash, health, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(stream_id) DO UPDATE SET
                    head_seq = excluded.head_seq,
                    head_receipt_id = excluded.head_receipt_id,
                    known_max_seq = excluded.known_max_seq,
                    state_hash = excluded.state_hash,
                    health = excluded.health,
                    updated_at = excluded.updated_at",
                params![
                    state.stream_id.as_bytes().as_slice(),
                    state.author.0.as_slice(),
                    &state.stream_name,
                    state.head_seq as i64,
                    state.head_receipt_id.as_ref().map(|id| id.0.as_slice()),
                    state.known_max_seq as i64,
                    state.state_hash.as_ref().map(|h| h.0.as_slice()),
                    health_int,
                    state.created_at,
                    state.updated_at,
                ],
            )?;

            // Update gaps: delete all, then insert current
            tx.execute(
                "DELETE FROM stream_gaps WHERE stream_id = ?1",
                params![state.stream_id.as_bytes().as_slice()],
            )?;

            for seq in &state.gaps {
                tx.execute(
                    "INSERT INTO stream_gaps (stream_id, missing_seq) VALUES (?1, ?2)",
                    params![state.stream_id.as_bytes().as_slice(), *seq as i64],
                )?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn list_streams(&self, author: Option<&Ed25519PublicKey>) -> Result<Vec<StreamId>> {
        let author = author.cloned();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let streams: Vec<StreamId> = if let Some(author) = author {
                let mut stmt = conn.prepare(
                    "SELECT stream_id FROM streams WHERE author = ?1",
                )?;
                stmt.query_map(params![author.0.as_slice()], |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    Ok(StreamId::from_bytes(bytes.try_into().unwrap_or([0u8; 32])))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
            } else {
                let mut stmt = conn.prepare("SELECT stream_id FROM streams")?;
                stmt.query_map([], |row| {
                    let bytes: Vec<u8> = row.get(0)?;
                    Ok(StreamId::from_bytes(bytes.try_into().unwrap_or([0u8; 32])))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
            };

            Ok(streams)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_gaps(&self, stream_id: &StreamId) -> Result<Vec<u64>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let mut stmt = conn.prepare(
                "SELECT missing_seq FROM stream_gaps WHERE stream_id = ?1 ORDER BY missing_seq",
            )?;

            let gaps: Vec<u64> = stmt
                .query_map(params![stream_id.as_bytes().as_slice()], |row| {
                    row.get::<_, i64>(0).map(|v| v as u64)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(gaps)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn add_gaps(&self, stream_id: &StreamId, seqs: &[u64]) -> Result<()> {
        let stream_id = *stream_id;
        let seqs = seqs.to_vec();
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            for seq in seqs {
                conn.execute(
                    "INSERT OR IGNORE INTO stream_gaps (stream_id, missing_seq) VALUES (?1, ?2)",
                    params![stream_id.as_bytes().as_slice(), seq as i64],
                )?;
            }

            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn remove_gap(&self, stream_id: &StreamId, seq: u64) -> Result<()> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            conn.execute(
                "DELETE FROM stream_gaps WHERE stream_id = ?1 AND missing_seq = ?2",
                params![stream_id.as_bytes().as_slice(), seq as i64],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn mark_gap_requested(&self, stream_id: &StreamId, seq: u64, at: i64) -> Result<()> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            conn.execute(
                "UPDATE stream_gaps SET requested_at = ?3 WHERE stream_id = ?1 AND missing_seq = ?2",
                params![stream_id.as_bytes().as_slice(), seq as i64, at],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn record_fork(
        &self,
        stream_id: &StreamId,
        seq: u64,
        receipt_id: &ReceiptId,
    ) -> Result<()> {
        let stream_id = *stream_id;
        let receipt_id = *receipt_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let now = now_millis();
            conn.execute(
                "INSERT OR IGNORE INTO forks (stream_id, seq, receipt_id, detected_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    stream_id.as_bytes().as_slice(),
                    seq as i64,
                    receipt_id.0.as_slice(),
                    now,
                ],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_forks(&self, stream_id: &StreamId) -> Result<Vec<Fork>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let mut stmt = conn.prepare(
                "SELECT seq, receipt_id, detected_at FROM forks WHERE stream_id = ?1 ORDER BY seq",
            )?;

            let forks: Vec<Fork> = stmt
                .query_map(params![stream_id.as_bytes().as_slice()], |row| {
                    let seq: i64 = row.get(0)?;
                    let receipt_id_bytes: Vec<u8> = row.get(1)?;
                    let detected_at: i64 = row.get(2)?;

                    Ok(Fork {
                        stream_id,
                        seq: seq as u64,
                        receipt_id: ReceiptId::from_bytes(
                            receipt_id_bytes.try_into().unwrap_or([0u8; 32]),
                        ),
                        detected_at,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(forks)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_receipt_ids_since(
        &self,
        stream_id: &StreamId,
        after_seq: u64,
    ) -> Result<Vec<(u64, ReceiptId)>> {
        let stream_id = *stream_id;
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let mut stmt = conn.prepare(
                "SELECT seq, receipt_id FROM receipts
                 WHERE stream_id = ?1 AND seq > ?2
                 ORDER BY seq",
            )?;

            let pairs: Vec<(u64, ReceiptId)> = stmt
                .query_map(
                    params![stream_id.as_bytes().as_slice(), after_seq as i64],
                    |row| {
                        let seq: i64 = row.get(0)?;
                        let id_bytes: Vec<u8> = row.get(1)?;
                        Ok((
                            seq as u64,
                            ReceiptId::from_bytes(id_bytes.try_into().unwrap_or([0u8; 32])),
                        ))
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(pairs)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }

    async fn get_all_stream_heads(&self) -> Result<Vec<(StreamId, u64, ReceiptId)>> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| {
                StoreError::Database(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                    Some(format!("mutex poisoned: {}", e)),
                ))
            })?;

            let mut stmt = conn.prepare(
                "SELECT stream_id, head_seq, head_receipt_id FROM streams
                 WHERE head_receipt_id IS NOT NULL",
            )?;

            let heads: Vec<(StreamId, u64, ReceiptId)> = stmt
                .query_map([], |row| {
                    let stream_id_bytes: Vec<u8> = row.get(0)?;
                    let head_seq: i64 = row.get(1)?;
                    let head_receipt_id_bytes: Vec<u8> = row.get(2)?;

                    Ok((
                        StreamId::from_bytes(stream_id_bytes.try_into().unwrap_or([0u8; 32])),
                        head_seq as u64,
                        ReceiptId::from_bytes(head_receipt_id_bytes.try_into().unwrap_or([0u8; 32])),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(heads)
        })
        .await
        .map_err(|e| StoreError::Database(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!("spawn_blocking failed: {}", e)),
        )))?
    }
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
    use chainge_kernel_core::{canonical_bytes, Keypair, ReceiptBuilder, ReceiptKind};

    fn make_test_receipt(keypair: &Keypair, seq: u64) -> Receipt {
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let mut builder = ReceiptBuilder::new(keypair.public_key(), stream_id, seq)
            .timestamp(1234567890000)
            .payload(format!("test payload {}", seq).into_bytes());

        if seq == 1 {
            builder = builder.kind(ReceiptKind::StreamInit);
        } else {
            builder = builder
                .kind(ReceiptKind::Data)
                .prev(ReceiptId::from_bytes([0xaa; 32]));
        }

        builder.sign(keypair)
    }

    #[tokio::test]
    async fn test_insert_and_get_receipt() {
        let store = SqliteStore::open_memory().unwrap();
        let keypair = Keypair::generate();
        let receipt = make_test_receipt(&keypair, 1);
        let canonical = canonical_bytes(&receipt);
        let receipt_id = receipt.compute_id();

        // Insert
        let result = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(result, InsertResult::Inserted);

        // Get by ID
        let retrieved = store.get_receipt(&receipt_id).await.unwrap().unwrap();
        assert_eq!(retrieved.seq(), 1);
        assert_eq!(retrieved.kind(), ReceiptKind::StreamInit);
    }

    #[tokio::test]
    async fn test_idempotent_insert() {
        let store = SqliteStore::open_memory().unwrap();
        let keypair = Keypair::generate();
        let receipt = make_test_receipt(&keypair, 1);
        let canonical = canonical_bytes(&receipt);

        // First insert
        let r1 = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(r1, InsertResult::Inserted);

        // Second insert - should be idempotent
        let r2 = store.insert_receipt(&receipt, &canonical).await.unwrap();
        assert_eq!(r2, InsertResult::AlreadyExists);
    }

    #[tokio::test]
    async fn test_conflict_detection() {
        let store = SqliteStore::open_memory().unwrap();
        let keypair = Keypair::generate();

        // Create two different receipts at the same position
        let receipt1 = make_test_receipt(&keypair, 1);
        let canonical1 = canonical_bytes(&receipt1);
        let id1 = receipt1.compute_id();

        // Different payload = different receipt
        let stream_id = StreamId::derive(&keypair.public_key(), "test");
        let receipt2 = ReceiptBuilder::new(keypair.public_key(), stream_id, 1)
            .timestamp(1234567890000)
            .kind(ReceiptKind::StreamInit)
            .payload(b"different payload".to_vec())
            .sign(&keypair);
        let canonical2 = canonical_bytes(&receipt2);

        // Insert first
        store.insert_receipt(&receipt1, &canonical1).await.unwrap();

        // Insert second - should conflict
        let result = store.insert_receipt(&receipt2, &canonical2).await.unwrap();
        assert!(matches!(result, InsertResult::Conflict { existing } if existing == id1));
    }

    #[tokio::test]
    async fn test_stream_state() {
        let store = SqliteStore::open_memory().unwrap();
        let keypair = Keypair::generate();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        // Create and save state
        let state = StreamState::new(keypair.public_key(), "test".to_string(), 1000);
        store.upsert_stream_state(&state).await.unwrap();

        // Retrieve
        let retrieved = store.get_stream_state(&stream_id).await.unwrap().unwrap();
        assert_eq!(retrieved.stream_id, stream_id);
        assert_eq!(retrieved.head_seq, 0);
    }

    #[tokio::test]
    async fn test_gap_tracking() {
        let store = SqliteStore::open_memory().unwrap();
        let keypair = Keypair::generate();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        // Add gaps
        store.add_gaps(&stream_id, &[2, 3, 5]).await.unwrap();

        // Check gaps
        let gaps = store.get_gaps(&stream_id).await.unwrap();
        assert_eq!(gaps, vec![2, 3, 5]);

        // Remove a gap
        store.remove_gap(&stream_id, 3).await.unwrap();
        let gaps = store.get_gaps(&stream_id).await.unwrap();
        assert_eq!(gaps, vec![2, 5]);
    }
}
