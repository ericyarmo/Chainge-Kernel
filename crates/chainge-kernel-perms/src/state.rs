//! Permission state computation.
//!
//! Permission state is computed by replaying Grant and Revoke receipts.
//! This module provides the data structures and logic for maintaining
//! and querying permission state.

use std::collections::HashMap;

use chainge_kernel_core::{Ed25519PublicKey, Receipt, ReceiptId, ReceiptKind, StreamId};

use crate::error::{PermsError, Result};
use crate::grant::{Conditions, GrantPayload, PermissionScope, RevokePayload};

/// State of a single grant.
#[derive(Debug, Clone)]
pub struct GrantState {
    /// The receipt ID of the grant.
    pub grant_receipt_id: ReceiptId,

    /// Who granted this permission.
    pub grantor: Ed25519PublicKey,

    /// Who received this permission.
    pub recipient: Ed25519PublicKey,

    /// What permission was granted.
    pub scope: PermissionScope,

    /// When the grant was created (seq number in permissions stream).
    pub granted_at_seq: u64,

    /// Optional conditions.
    pub conditions: Option<Conditions>,

    /// Whether this grant has been revoked.
    pub revoked: bool,

    /// When it was revoked (if revoked).
    pub revoked_at_seq: Option<u64>,

    /// The receipt that revoked it (if revoked).
    pub revoke_receipt_id: Option<ReceiptId>,

    /// Number of times this grant has been used.
    pub use_count: u32,
}

impl GrantState {
    /// Check if this grant is currently valid.
    pub fn is_valid(&self, now: i64) -> bool {
        // Revoked grants are invalid
        if self.revoked {
            return false;
        }

        // Check conditions
        if let Some(ref conditions) = self.conditions {
            if !conditions.is_valid(now, self.use_count) {
                return false;
            }
        }

        true
    }

    /// Record a use of this grant.
    pub fn record_use(&mut self) {
        self.use_count += 1;
    }
}

/// Aggregated permission state.
///
/// Built by replaying Grant and Revoke receipts from a permissions stream.
#[derive(Debug, Default)]
pub struct PermissionState {
    /// All grants indexed by grant receipt ID.
    grants: HashMap<ReceiptId, GrantState>,

    /// Index: recipient -> list of their grants.
    by_recipient: HashMap<Ed25519PublicKey, Vec<ReceiptId>>,

    /// Index: (recipient, scope) -> grant ID for quick lookups.
    by_scope: HashMap<(Ed25519PublicKey, ScopeKey), ReceiptId>,
}

/// Simplified scope key for indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ScopeKey {
    ReadStream(StreamId),
    ReadReceipt(ReceiptId),
    WriteStream(StreamId),
    Admin(StreamId),
}

impl From<&PermissionScope> for ScopeKey {
    fn from(scope: &PermissionScope) -> Self {
        match scope {
            PermissionScope::ReadStream { stream_id } => ScopeKey::ReadStream(*stream_id),
            PermissionScope::ReadReceipt { receipt_id } => ScopeKey::ReadReceipt(*receipt_id),
            PermissionScope::WriteStream { stream_id } => ScopeKey::WriteStream(*stream_id),
            PermissionScope::Admin { stream_id } => ScopeKey::Admin(*stream_id),
        }
    }
}

impl PermissionState {
    /// Create a new empty permission state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a Grant receipt to the state.
    pub fn apply_grant(
        &mut self,
        receipt_id: ReceiptId,
        grantor: Ed25519PublicKey,
        seq: u64,
        payload: GrantPayload,
    ) {
        let scope_key = ScopeKey::from(&payload.scope);

        let grant = GrantState {
            grant_receipt_id: receipt_id,
            grantor,
            recipient: payload.recipient,
            scope: payload.scope,
            granted_at_seq: seq,
            conditions: payload.conditions,
            revoked: false,
            revoked_at_seq: None,
            revoke_receipt_id: None,
            use_count: 0,
        };

        // Add to grants map
        self.grants.insert(receipt_id, grant);

        // Update indexes
        self.by_recipient
            .entry(payload.recipient)
            .or_default()
            .push(receipt_id);

        self.by_scope
            .insert((payload.recipient, scope_key), receipt_id);
    }

    /// Apply a Revoke receipt to the state.
    pub fn apply_revoke(&mut self, revoke_receipt_id: ReceiptId, seq: u64, payload: RevokePayload) {
        if let Some(grant) = self.grants.get_mut(&payload.grant_receipt_id) {
            grant.revoked = true;
            grant.revoked_at_seq = Some(seq);
            grant.revoke_receipt_id = Some(revoke_receipt_id);
        }
    }

    /// Process a permission receipt (Grant or Revoke).
    pub fn apply_receipt(&mut self, receipt: &Receipt) -> Result<()> {
        let receipt_id = receipt.compute_id();
        let author = receipt.author().clone();
        let seq = receipt.seq();

        match receipt.kind() {
            ReceiptKind::Grant => {
                let payload = GrantPayload::from_bytes(&receipt.payload)
                    .map_err(|e| PermsError::InvalidGrant(e.to_string()))?;
                self.apply_grant(receipt_id, author, seq, payload);
            }
            ReceiptKind::Revoke => {
                let payload = RevokePayload::from_bytes(&receipt.payload)
                    .map_err(|e| PermsError::InvalidGrant(e.to_string()))?;
                self.apply_revoke(receipt_id, seq, payload);
            }
            _ => {
                // Not a permission receipt, ignore
            }
        }

        Ok(())
    }

    /// Check if a principal can read a stream.
    pub fn can_read_stream(
        &self,
        principal: &Ed25519PublicKey,
        stream_id: &StreamId,
        now: i64,
    ) -> bool {
        // Check for ReadStream grant
        let read_key = (principal.clone(), ScopeKey::ReadStream(*stream_id));
        if let Some(&grant_id) = self.by_scope.get(&read_key) {
            if let Some(grant) = self.grants.get(&grant_id) {
                if grant.is_valid(now) {
                    return true;
                }
            }
        }

        // Check for Admin grant (implies read)
        let admin_key = (principal.clone(), ScopeKey::Admin(*stream_id));
        if let Some(&grant_id) = self.by_scope.get(&admin_key) {
            if let Some(grant) = self.grants.get(&grant_id) {
                if grant.is_valid(now) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a principal can read a specific receipt.
    pub fn can_read_receipt(
        &self,
        principal: &Ed25519PublicKey,
        receipt_id: &ReceiptId,
        stream_id: &StreamId,
        now: i64,
    ) -> bool {
        // First check stream-level read access
        if self.can_read_stream(principal, stream_id, now) {
            return true;
        }

        // Check for specific receipt read access
        let receipt_key = (principal.clone(), ScopeKey::ReadReceipt(*receipt_id));
        if let Some(&grant_id) = self.by_scope.get(&receipt_key) {
            if let Some(grant) = self.grants.get(&grant_id) {
                if grant.is_valid(now) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a principal can write to a stream.
    pub fn can_write_stream(
        &self,
        principal: &Ed25519PublicKey,
        stream_id: &StreamId,
        now: i64,
    ) -> bool {
        // Check for WriteStream grant
        let write_key = (principal.clone(), ScopeKey::WriteStream(*stream_id));
        if let Some(&grant_id) = self.by_scope.get(&write_key) {
            if let Some(grant) = self.grants.get(&grant_id) {
                if grant.is_valid(now) {
                    return true;
                }
            }
        }

        // Check for Admin grant (implies write)
        let admin_key = (principal.clone(), ScopeKey::Admin(*stream_id));
        if let Some(&grant_id) = self.by_scope.get(&admin_key) {
            if let Some(grant) = self.grants.get(&grant_id) {
                if grant.is_valid(now) {
                    return true;
                }
            }
        }

        false
    }

    /// Get a grant by ID.
    pub fn get_grant(&self, grant_id: &ReceiptId) -> Option<&GrantState> {
        self.grants.get(grant_id)
    }

    /// Get a mutable grant by ID.
    pub fn get_grant_mut(&mut self, grant_id: &ReceiptId) -> Option<&mut GrantState> {
        self.grants.get_mut(grant_id)
    }

    /// List all grants for a recipient.
    pub fn grants_for(&self, recipient: &Ed25519PublicKey) -> Vec<&GrantState> {
        self.by_recipient
            .get(recipient)
            .map(|ids| ids.iter().filter_map(|id| self.grants.get(id)).collect())
            .unwrap_or_default()
    }

    /// List all valid grants for a recipient.
    pub fn valid_grants_for(&self, recipient: &Ed25519PublicKey, now: i64) -> Vec<&GrantState> {
        self.grants_for(recipient)
            .into_iter()
            .filter(|g| g.is_valid(now))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chainge_kernel_core::Keypair;

    #[test]
    fn test_grant_and_check() {
        let mut state = PermissionState::new();

        let grantor = Keypair::generate();
        let recipient = Keypair::generate();
        let stream_id = StreamId::derive(&grantor.public_key(), "test");

        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let payload = GrantPayload::read_stream(recipient.public_key(), stream_id);

        state.apply_grant(grant_id, grantor.public_key(), 1, payload);

        assert!(state.can_read_stream(&recipient.public_key(), &stream_id, 0));
        assert!(!state.can_write_stream(&recipient.public_key(), &stream_id, 0));
    }

    #[test]
    fn test_revoke_removes_access() {
        let mut state = PermissionState::new();

        let grantor = Keypair::generate();
        let recipient = Keypair::generate();
        let stream_id = StreamId::derive(&grantor.public_key(), "test");

        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let payload = GrantPayload::read_stream(recipient.public_key(), stream_id);

        state.apply_grant(grant_id, grantor.public_key(), 1, payload);
        assert!(state.can_read_stream(&recipient.public_key(), &stream_id, 0));

        let revoke_payload = RevokePayload::new(grant_id);
        let revoke_id = ReceiptId::from_bytes([0x43; 32]);
        state.apply_revoke(revoke_id, 2, revoke_payload);

        assert!(!state.can_read_stream(&recipient.public_key(), &stream_id, 0));
    }

    #[test]
    fn test_expired_grant() {
        let mut state = PermissionState::new();

        let grantor = Keypair::generate();
        let recipient = Keypair::generate();
        let stream_id = StreamId::derive(&grantor.public_key(), "test");

        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let payload = GrantPayload::read_stream(recipient.public_key(), stream_id)
            .with_conditions(Conditions::expires_at(1000));

        state.apply_grant(grant_id, grantor.public_key(), 1, payload);

        // Before expiration
        assert!(state.can_read_stream(&recipient.public_key(), &stream_id, 500));

        // After expiration
        assert!(!state.can_read_stream(&recipient.public_key(), &stream_id, 1500));
    }

    #[test]
    fn test_admin_implies_read_and_write() {
        let mut state = PermissionState::new();

        let grantor = Keypair::generate();
        let recipient = Keypair::generate();
        let stream_id = StreamId::derive(&grantor.public_key(), "test");

        let grant_id = ReceiptId::from_bytes([0x42; 32]);
        let payload = GrantPayload {
            recipient: recipient.public_key(),
            scope: PermissionScope::Admin { stream_id },
            conditions: None,
        };

        state.apply_grant(grant_id, grantor.public_key(), 1, payload);

        assert!(state.can_read_stream(&recipient.public_key(), &stream_id, 0));
        assert!(state.can_write_stream(&recipient.public_key(), &stream_id, 0));
    }
}
