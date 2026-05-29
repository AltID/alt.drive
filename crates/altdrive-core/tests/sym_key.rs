//! Behavior tests for `SymKey` — the 32-byte symmetric key newtype that
//! holds all symmetric key material in the Alt.Drive key hierarchy
//! (masterKey, collectionKey, fileKey, KEK, recoveryKey).
//!
//! See `../../../DESIGN.md` §4 for the key hierarchy this type underlies,
//! and `../../../docs/threat-model.md` for the threat model that motivates
//! the Zeroize discipline.

use altdrive_core::SymKey;
use zeroize::Zeroize;

#[test]
fn sym_key_preserves_bytes_for_crypto_use() {
    // A SymKey is the unit of symmetric key material. The bytes it was
    // constructed from must be retrievable for use by subsequent
    // cryptographic operations (AEAD encryption, key derivation, etc.).
    // Without this, the type would be useless — crypto primitives need
    // the raw key bytes.
    //
    // Test data uses distinct bytes per position (0x01..0x20) so that
    // position-shifting mutations (e.g., `bytes.rotate_left(1)` in
    // `from_bytes`) are caught. A uniform `[0x42; 32]` test vector
    // would let such mutations through.
    let bytes: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    let key = SymKey::from_bytes(bytes);
    assert_eq!(key.expose_secret(), &bytes);
}

#[test]
fn sym_key_zeroizes_on_demand() {
    // The threat model (docs/threat-model.md, scenarios S4 and S8)
    // assumes secret material does not linger in memory beyond its
    // useful lifetime. SymKey must support explicit zeroization so
    // callers can scrub keys as soon as they're no longer needed.
    //
    // This test verifies the in-place zeroization mechanism that
    // ZeroizeOnDrop relies on. If Zeroize::zeroize() works correctly,
    // the Drop-based zeroization will too (since ZeroizeOnDrop derives
    // a Drop impl that simply calls zeroize()).
    let mut key = SymKey::from_bytes([0x42; 32]);
    assert_eq!(key.expose_secret(), &[0x42; 32]);
    key.zeroize();
    assert_eq!(key.expose_secret(), &[0u8; 32]);
}

#[test]
fn sym_key_zeroizes_on_drop() {
    // The threat-model property (S4, S8) is "secret material does not
    // linger in memory after the SymKey goes out of scope." Empirically
    // testing post-drop memory contents requires unsafe pointer
    // manipulation that the crate forbids (`#![forbid(unsafe_code)]`).
    //
    // Instead, we statically verify SymKey opts into Drop-based
    // zeroization by checking the `ZeroizeOnDrop` trait bound. Combined
    // with `sym_key_zeroizes_on_demand` (which proves the underlying
    // mechanism works), this anchors the property without unsafe.
    fn assert_zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}
    assert_zeroize_on_drop::<SymKey>();
}
