//! # Chainge Kernel Testkit
//!
//! Testing utilities for the Chainge Kernel.
//!
//! ## Overview
//!
//! This crate provides:
//!
//! - **Golden vectors**: Known test cases with expected outputs for cross-platform verification
//! - **Generators**: Proptest strategies for property-based testing
//! - **Fixtures**: Helper structs for setting up test scenarios
//!
//! ## Golden Vectors
//!
//! Golden vectors ensure deterministic canonicalization across implementations:
//!
//! ```rust
//! use chainge_kernel_testkit::vectors::{all_vectors, generate_receipt_from_vector};
//!
//! for vector in all_vectors() {
//!     let receipt = generate_receipt_from_vector(&vector);
//!     let id = receipt.compute_id();
//!     println!("{}: {}", vector.name, id.to_hex());
//! }
//! ```
//!
//! ## Property Testing
//!
//! Use the generators with proptest:
//!
//! ```rust,ignore
//! use proptest::prelude::*;
//! use chainge_kernel_testkit::generators::{ReceiptParams, receipt_from_params};
//!
//! proptest! {
//!     #[test]
//!     fn receipt_id_is_deterministic(params: ReceiptParams) {
//!         let r1 = receipt_from_params(&params);
//!         let r2 = receipt_from_params(&params);
//!         prop_assert_eq!(r1.compute_id(), r2.compute_id());
//!     }
//! }
//! ```
//!
//! ## Test Fixtures
//!
//! Quickly set up test scenarios:
//!
//! ```rust
//! use chainge_kernel_testkit::fixtures::TestFixture;
//!
//! let fixture = TestFixture::new();
//! let receipt = fixture.make_stream_init("my-stream", b"initial data");
//! ```

pub mod fixtures;
pub mod generators;
pub mod vectors;

pub use fixtures::{multi_party_fixtures, TestFixture};
pub use generators::{receipt_from_params, ReceiptParams};
pub use vectors::{all_vectors, generate_receipt_from_vector, verify_all_vectors, GoldenVector};
