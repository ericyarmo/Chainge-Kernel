//! Strong type definitions for the Chainge Kernel.
//!
//! All identifiers are newtypes to prevent misuse at compile time.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A 32-byte receipt identifier, computed as Blake3(canonical_bytes(receipt)).
///
/// This is the content-address of a receipt. Two receipts with the same
/// content will have the same ReceiptId.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId(pub [u8; 32]);

impl ReceiptId {
    /// Create a new ReceiptId from raw bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// The zero receipt ID (used as a sentinel).
    pub const ZERO: Self = Self([0u8; 32]);
}

impl fmt::Debug for ReceiptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReceiptId({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for ReceiptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

impl AsRef<[u8]> for ReceiptId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for ReceiptId {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for ReceiptId {
    type Error = std::array::TryFromSliceError;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let arr: [u8; 32] = slice.try_into()?;
        Ok(Self(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receipt_id_hex_roundtrip() {
        let id = ReceiptId::from_bytes([0x42; 32]);
        let hex = id.to_hex();
        let recovered = ReceiptId::from_hex(&hex).unwrap();
        assert_eq!(id, recovered);
    }

    #[test]
    fn test_receipt_id_display() {
        let id = ReceiptId::from_bytes([0xab; 32]);
        let display = format!("{}", id);
        assert_eq!(display, "abababababababab");
    }

    #[test]
    fn test_receipt_id_debug() {
        let id = ReceiptId::from_bytes([0xcd; 32]);
        let debug = format!("{:?}", id);
        assert!(debug.starts_with("ReceiptId("));
    }
}
