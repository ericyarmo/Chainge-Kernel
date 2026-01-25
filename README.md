# Kernel (Rust)

Cryptographic receipts for portable, verifiable memory.

## What It Is

A receipt is a signed attestation:

```
Receipt {
    author:    [u8; 32]      // Ed25519 public key
    schema:    String        // Payload type identifier (≤256 bytes)
    refs:      Vec<[u8; 32]> // Links to other receipts (sorted, ≤128)
    payload:   Bytes         // Opaque content (≤64KB)
    signature: [u8; 64]      // Ed25519 signature
}
```

Properties:
- **Signed** - Cryptographic proof of authorship
- **Immutable** - Content-addressed, can't be changed
- **Linkable** - Refs create DAGs (chains, merges, witnesses)
- **Portable** - Verify anywhere, no network required

## Install

```toml
[dependencies]
chainge-kernel = { git = "https://github.com/ericyarmo/Kernel" }
```

## Usage

```rust
use chainge_kernel::{Keypair, Receipt};

// Create keypair
let keypair = Keypair::generate();

// Create receipt
let receipt = Receipt::create(
    &keypair,
    "example/v1",
    vec![],  // refs
    b"hello".to_vec(),
)?;

// Verify
receipt.verify()?;

// Serialize
let bytes = receipt.to_bytes();
let decoded = Receipt::from_bytes(&bytes)?;

// Get IDs
let receipt_id = receipt.id();  // 32-byte hash
let cid = receipt.cid();        // IPFS CIDv1
```

## Encoding

DAG-CBOR with deterministic key ordering:
- Keys: `refs` < `author` < `schema` < `payload` < `signature`
- Refs sorted lexicographically, no duplicates

Domain separation:
- Signing: `sha256("chainge/receipt-sig/v1" || content_cbor)`
- Receipt ID: `sha256("chainge/receipt-id/v1" || receipt_cbor)`

## Test Vectors

`tests/golden_vectors.json` contains 10 vectors. Any implementation producing identical outputs is compatible.

```bash
cargo test
```

## Other Implementations

| Language | Repository |
|----------|------------|
| TypeScript | [github.com/ericyarmo/ChaingeKernelTS](https://github.com/ericyarmo/ChaingeKernelTS) |
| Swift | [github.com/ericyarmo/ChaingeKernelSwift](https://github.com/ericyarmo/ChaingeKernelSwift) |
| Go | [github.com/ericyarmo/ChaingeKernelGo](https://github.com/ericyarmo/ChaingeKernelGo) |
| Kotlin | [github.com/ericyarmo/ChaingeKernelKotlin](https://github.com/ericyarmo/ChaingeKernelKotlin) |
| Java | [github.com/ericyarmo/ChaingeKernelJava](https://github.com/ericyarmo/ChaingeKernelJava) |
| Python | [github.com/ericyarmo/ChaingeKernelPy](https://github.com/ericyarmo/ChaingeKernelPy) |
| C# | [github.com/ericyarmo/ChaingeKernelCSharp](https://github.com/ericyarmo/ChaingeKernelCSharp) |
| JavaScript | [github.com/ericyarmo/ChaingeKernelJS](https://github.com/ericyarmo/ChaingeKernelJS) |
| R | [github.com/ericyarmo/ChaingeKernelR](https://github.com/ericyarmo/ChaingeKernelR) |

## License

MIT OR Apache-2.0

## Contact

chaingestl@gmail.com
