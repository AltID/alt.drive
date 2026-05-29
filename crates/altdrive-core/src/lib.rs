//! # altdrive-core
//!
//! Cryptographic primitives, key hierarchy, and vault format for Alt.Drive.
//!
//! See `../../../DESIGN.md` for the full specification and
//! `../../../docs/threat-model.md` for the adversary models this code
//! defends against.
//!
//! This crate is the pure cryptography + data-model layer. It has no
//! dependencies on iroh, no networking, no filesystem operations beyond
//! serialization helpers, and no async runtime.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod secretbox;

use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 32-byte symmetric key.
///
/// All symmetric key material in the Alt.Drive key hierarchy (masterKey,
/// collectionKey, fileKey, KEK, recoveryKey) is held in this newtype.
/// See `DESIGN.md` §4 for the key hierarchy.
///
/// The underlying byte buffer is zeroized when the `SymKey` is dropped
/// (via `ZeroizeOnDrop`) or when `zeroize()` is called explicitly (via
/// `Zeroize`). See `docs/threat-model.md` scenarios S4 and S8 for why
/// this matters.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SymKey([u8; 32]);

impl SymKey {
    /// Construct a SymKey from a 32-byte array.
    ///
    /// The caller is responsible for ensuring the bytes were produced by
    /// a cryptographically appropriate source (CSPRNG, KDF, or unwrap
    /// of a wrapped key). This constructor does not validate the source.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Expose the raw 32 bytes for use by a cryptographic primitive.
    ///
    /// Named `expose_secret` (per the `secrecy` crate convention) to make
    /// the call site obvious wherever secret material is being unwrapped.
    /// Do not store the returned reference; pass it directly into the
    /// primitive that needs it.
    pub fn expose_secret(&self) -> &[u8; 32] {
        &self.0
    }
}
