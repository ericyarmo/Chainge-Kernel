//! Grant and Revoke receipt payloads.
//!
//! Permissions are expressed as receipts. A Grant receipt grants access
//! to a principal, and a Revoke receipt revokes a previous grant.

use serde::{Deserialize, Serialize};

use chainge_kernel_core::{Ed25519PublicKey, ReceiptId, StreamId};

/// Payload for a Grant receipt.
///
/// Grants permissions to a recipient principal. The grant is created
/// by the stream owner and stored as a receipt in a permissions stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantPayload {
    /// The principal being granted access.
    pub recipient: Ed25519PublicKey,

    /// What access is being granted.
    pub scope: PermissionScope,

    /// Optional conditions on the grant.
    pub conditions: Option<Conditions>,
    // NOTE: Key material is shared via separate KeyShare receipts, NOT embedded here.
    // This separation allows key rotation without re-granting permissions.
}

/// Payload for a Revoke receipt.
///
/// Revokes a previous grant. After revocation, the grant is no longer valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokePayload {
    /// The grant receipt being revoked.
    pub grant_receipt_id: ReceiptId,

    /// Optional reason for revocation.
    pub reason: Option<String>,
}

/// Scope of a permission grant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionScope {
    /// Read access to an entire stream.
    ReadStream {
        /// The stream to grant read access to.
        stream_id: StreamId,
    },

    /// Read access to a specific receipt.
    ReadReceipt {
        /// The specific receipt to grant read access to.
        receipt_id: ReceiptId,
    },

    /// Write access to a stream (delegation).
    ///
    /// Note: For v0, delegation is out of scope. Only stream owner can write.
    WriteStream {
        /// The stream to grant write access to.
        stream_id: StreamId,
    },

    /// Full administrative control over a stream.
    Admin {
        /// The stream to grant admin access to.
        stream_id: StreamId,
    },
}

impl PermissionScope {
    /// Check if this scope grants read access to a stream.
    pub fn can_read_stream(&self, stream_id: &StreamId) -> bool {
        match self {
            PermissionScope::ReadStream { stream_id: sid } => sid == stream_id,
            PermissionScope::Admin { stream_id: sid } => sid == stream_id,
            _ => false,
        }
    }

    /// Check if this scope grants read access to a receipt.
    pub fn can_read_receipt(&self, receipt_id: &ReceiptId, stream_id: &StreamId) -> bool {
        match self {
            PermissionScope::ReadReceipt { receipt_id: rid } => rid == receipt_id,
            PermissionScope::ReadStream { stream_id: sid } => sid == stream_id,
            PermissionScope::Admin { stream_id: sid } => sid == stream_id,
            _ => false,
        }
    }

    /// Check if this scope grants write access to a stream.
    pub fn can_write_stream(&self, stream_id: &StreamId) -> bool {
        match self {
            PermissionScope::WriteStream { stream_id: sid } => sid == stream_id,
            PermissionScope::Admin { stream_id: sid } => sid == stream_id,
            _ => false,
        }
    }

    /// Get the stream ID this scope applies to, if any.
    pub fn stream_id(&self) -> Option<&StreamId> {
        match self {
            PermissionScope::ReadStream { stream_id } => Some(stream_id),
            PermissionScope::WriteStream { stream_id } => Some(stream_id),
            PermissionScope::Admin { stream_id } => Some(stream_id),
            PermissionScope::ReadReceipt { .. } => None,
        }
    }
}

/// Conditions that may limit a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conditions {
    /// When the grant expires (Unix milliseconds).
    pub expires_at: Option<i64>,

    /// Maximum number of uses allowed.
    pub max_uses: Option<u32>,
}

impl Conditions {
    /// Create conditions with no restrictions.
    pub fn none() -> Option<Self> {
        None
    }

    /// Create conditions with an expiration time.
    pub fn expires_at(timestamp: i64) -> Self {
        Self {
            expires_at: Some(timestamp),
            max_uses: None,
        }
    }

    /// Create conditions with a usage limit.
    pub fn max_uses(count: u32) -> Self {
        Self {
            expires_at: None,
            max_uses: Some(count),
        }
    }

    /// Check if these conditions are still valid.
    pub fn is_valid(&self, now: i64, uses: u32) -> bool {
        // Check expiration
        if let Some(expires) = self.expires_at {
            if now > expires {
                return false;
            }
        }

        // Check usage limit
        if let Some(max) = self.max_uses {
            if uses >= max {
                return false;
            }
        }

        true
    }
}

impl GrantPayload {
    /// Create a new grant for stream read access.
    pub fn read_stream(recipient: Ed25519PublicKey, stream_id: StreamId) -> Self {
        Self {
            recipient,
            scope: PermissionScope::ReadStream { stream_id },
            conditions: None,
        }
    }

    /// Create a new grant for receipt read access.
    pub fn read_receipt(recipient: Ed25519PublicKey, receipt_id: ReceiptId) -> Self {
        Self {
            recipient,
            scope: PermissionScope::ReadReceipt { receipt_id },
            conditions: None,
        }
    }

    /// Add conditions to this grant.
    pub fn with_conditions(mut self, conditions: Conditions) -> Self {
        self.conditions = Some(conditions);
        self
    }

    /// Serialize to CBOR bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).expect("CBOR serialization failed");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ciborium::de::Error<std::io::Error>> {
        ciborium::from_reader(bytes)
    }
}

impl RevokePayload {
    /// Create a new revoke payload.
    pub fn new(grant_receipt_id: ReceiptId) -> Self {
        Self {
            grant_receipt_id,
            reason: None,
        }
    }

    /// Add a reason for the revocation.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Serialize to CBOR bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).expect("CBOR serialization failed");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ciborium::de::Error<std::io::Error>> {
        ciborium::from_reader(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chainge_kernel_core::Keypair;

    #[test]
    fn test_grant_payload_roundtrip() {
        let keypair = Keypair::generate();
        let stream_id = StreamId::derive(&keypair.public_key(), "test");

        let grant = GrantPayload::read_stream(keypair.public_key(), stream_id);
        let bytes = grant.to_bytes();
        let recovered = GrantPayload::from_bytes(&bytes).unwrap();

        assert_eq!(grant, recovered);
    }

    #[test]
    fn test_conditions_expiration() {
        let cond = Conditions::expires_at(1000);

        assert!(cond.is_valid(500, 0)); // Before expiration
        assert!(cond.is_valid(1000, 0)); // At expiration
        assert!(!cond.is_valid(1001, 0)); // After expiration
    }

    #[test]
    fn test_conditions_max_uses() {
        let cond = Conditions::max_uses(3);

        assert!(cond.is_valid(0, 0)); // First use
        assert!(cond.is_valid(0, 2)); // Third use
        assert!(!cond.is_valid(0, 3)); // Exceeded
    }

    #[test]
    fn test_scope_read_stream() {
        let stream_id = StreamId::ZERO;
        let other_stream = StreamId::from_bytes([1u8; 32]);
        let scope = PermissionScope::ReadStream { stream_id };

        assert!(scope.can_read_stream(&stream_id));
        assert!(!scope.can_read_stream(&other_stream));
    }
}
