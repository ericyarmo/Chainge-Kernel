//! Store trait: the minimal interface for receipt persistence.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::crypto::Author;
use crate::error::Result;
use crate::receipt::{Receipt, ReceiptId};

/// Result of inserting a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertResult {
    /// Receipt was inserted (new).
    Inserted,
    /// Receipt already exists (idempotent, not an error).
    AlreadyExists,
}

/// The store trait: minimal interface for receipt persistence.
///
/// Implementations can be in-memory, SQLite, or distributed.
/// The kernel doesn't care—it just needs these operations.
pub trait Store: Send + Sync {
    /// Insert a receipt.
    ///
    /// Returns `Inserted` if new, `AlreadyExists` if duplicate.
    /// No conflicts possible because IDs are content-addressed.
    fn insert(&self, receipt: &Receipt) -> Result<InsertResult>;

    /// Get a receipt by ID.
    fn get(&self, id: &ReceiptId) -> Result<Option<Receipt>>;

    /// Check if a receipt exists.
    fn has(&self, id: &ReceiptId) -> Result<bool>;

    /// Get all receipts by a specific author.
    fn by_author(&self, author: &Author) -> Result<Vec<Receipt>>;

    /// Get all receipts that reference a given ID.
    fn refs_to(&self, id: &ReceiptId) -> Result<Vec<Receipt>>;

    /// Get all receipt IDs (for sync).
    fn all_ids(&self) -> Result<Vec<ReceiptId>>;

    /// Count of receipts.
    fn count(&self) -> Result<usize>;
}

/// In-memory store for testing and simple use cases.
pub struct MemoryStore {
    receipts: RwLock<HashMap<ReceiptId, Receipt>>,
}

impl MemoryStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            receipts: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for MemoryStore {
    fn insert(&self, receipt: &Receipt) -> Result<InsertResult> {
        let id = receipt.id();
        let mut receipts = self.receipts.write().unwrap();

        if receipts.contains_key(&id) {
            Ok(InsertResult::AlreadyExists)
        } else {
            receipts.insert(id, receipt.clone());
            Ok(InsertResult::Inserted)
        }
    }

    fn get(&self, id: &ReceiptId) -> Result<Option<Receipt>> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts.get(id).cloned())
    }

    fn has(&self, id: &ReceiptId) -> Result<bool> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts.contains_key(id))
    }

    fn by_author(&self, author: &Author) -> Result<Vec<Receipt>> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts
            .values()
            .filter(|r| &r.author == author)
            .cloned()
            .collect())
    }

    fn refs_to(&self, id: &ReceiptId) -> Result<Vec<Receipt>> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts
            .values()
            .filter(|r| r.references(id))
            .cloned()
            .collect())
    }

    fn all_ids(&self) -> Result<Vec<ReceiptId>> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts.keys().cloned().collect())
    }

    fn count(&self) -> Result<usize> {
        let receipts = self.receipts.read().unwrap();
        Ok(receipts.len())
    }
}

/// Sync two stores by exchanging all receipts.
///
/// After sync, both stores have the union of their receipts.
pub fn sync<S1: Store, S2: Store>(store1: &S1, store2: &S2) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    // Get all IDs from both stores
    let ids1 = store1.all_ids()?;
    let ids2 = store2.all_ids()?;

    // Send from store1 to store2
    for id in &ids1 {
        if !store2.has(id)? {
            if let Some(receipt) = store1.get(id)? {
                store2.insert(&receipt)?;
                report.sent_1_to_2 += 1;
            }
        }
    }

    // Send from store2 to store1
    for id in &ids2 {
        if !store1.has(id)? {
            if let Some(receipt) = store2.get(id)? {
                store1.insert(&receipt)?;
                report.sent_2_to_1 += 1;
            }
        }
    }

    Ok(report)
}

/// Report from a sync operation.
#[derive(Debug, Default)]
pub struct SyncReport {
    /// Receipts sent from store1 to store2.
    pub sent_1_to_2: usize,
    /// Receipts sent from store2 to store1.
    pub sent_2_to_1: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    #[test]
    fn test_insert_and_get() {
        let store = MemoryStore::new();
        let keypair = Keypair::generate();
        let receipt = Receipt::new(&keypair, "test/v1", vec![], b"hello".to_vec()).unwrap();

        let id = receipt.id();
        assert_eq!(store.insert(&receipt).unwrap(), InsertResult::Inserted);
        assert_eq!(store.insert(&receipt).unwrap(), InsertResult::AlreadyExists);

        let retrieved = store.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.id(), id);
    }

    #[test]
    fn test_by_author() {
        let store = MemoryStore::new();
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();

        let r1 = Receipt::new(&kp1, "test/v1", vec![], b"from kp1".to_vec()).unwrap();
        let r2 = Receipt::new(&kp2, "test/v1", vec![], b"from kp2".to_vec()).unwrap();
        let r3 = Receipt::new(&kp1, "test/v1", vec![], b"also from kp1".to_vec()).unwrap();

        store.insert(&r1).unwrap();
        store.insert(&r2).unwrap();
        store.insert(&r3).unwrap();

        let kp1_receipts = store.by_author(&kp1.author()).unwrap();
        assert_eq!(kp1_receipts.len(), 2);

        let kp2_receipts = store.by_author(&kp2.author()).unwrap();
        assert_eq!(kp2_receipts.len(), 1);
    }

    #[test]
    fn test_refs_to() {
        let store = MemoryStore::new();
        let keypair = Keypair::generate();

        let r1 = Receipt::new(&keypair, "test/v1", vec![], b"first".to_vec()).unwrap();
        let r1_id = r1.id();

        let r2 = Receipt::new(&keypair, "test/v1", vec![r1_id], b"refs r1".to_vec()).unwrap();
        let r3 = Receipt::new(&keypair, "test/v1", vec![r1_id], b"also refs r1".to_vec()).unwrap();
        let r4 = Receipt::new(&keypair, "test/v1", vec![], b"no refs".to_vec()).unwrap();

        store.insert(&r1).unwrap();
        store.insert(&r2).unwrap();
        store.insert(&r3).unwrap();
        store.insert(&r4).unwrap();

        let refs = store.refs_to(&r1_id).unwrap();
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_sync() {
        let store1 = MemoryStore::new();
        let store2 = MemoryStore::new();
        let keypair = Keypair::generate();

        // Add receipts to store1
        let r1 = Receipt::new(&keypair, "test/v1", vec![], b"only in 1".to_vec()).unwrap();
        store1.insert(&r1).unwrap();

        // Add receipts to store2
        let r2 = Receipt::new(&keypair, "test/v1", vec![], b"only in 2".to_vec()).unwrap();
        store2.insert(&r2).unwrap();

        // Add common receipt
        let r3 = Receipt::new(&keypair, "test/v1", vec![], b"in both".to_vec()).unwrap();
        store1.insert(&r3).unwrap();
        store2.insert(&r3).unwrap();

        // Before sync
        assert_eq!(store1.count().unwrap(), 2);
        assert_eq!(store2.count().unwrap(), 2);

        // Sync
        let report = sync(&store1, &store2).unwrap();
        assert_eq!(report.sent_1_to_2, 1);
        assert_eq!(report.sent_2_to_1, 1);

        // After sync - both have all 3
        assert_eq!(store1.count().unwrap(), 3);
        assert_eq!(store2.count().unwrap(), 3);
    }
}
