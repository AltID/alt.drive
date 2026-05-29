//! Behavior tests for the secretbox AEAD primitive — small-field
//! authenticated encryption used throughout the Alt.Drive key
//! hierarchy.
//!
//! `secretbox::seal` and `secretbox::open` are the symmetric AEAD pair
//! that wrap and unwrap every layer of the key hierarchy:
//!
//!   KEK + encryptedMasterKey       → masterKey
//!   masterKey + encryptedCollectionKey → collectionKey
//!   collectionKey + encryptedFileKey   → fileKey
//!
//! See `../../../DESIGN.md` §3 (algorithms), §4 (key hierarchy),
//! §5.4 (on-disk format = nonce || ciphertext || auth_tag).
//!
//! The underlying primitive is libsodium's `crypto_secretbox_easy`
//! (XSalsa20-Poly1305 with a 24-byte nonce and 16-byte auth tag).

use altdrive_core::{secretbox, SymKey};

#[test]
fn secretbox_round_trips_plaintext() {
    // The fundamental AEAD property: anything you seal, you can open
    // back to the original plaintext using the same key. Without this,
    // every layer of the key hierarchy would fail to unwrap.
    //
    // We don't assert on the ciphertext bytes here because `seal`
    // generates a random nonce each call — the ciphertext varies
    // every time. The round-trip property is what we actually need
    // from the primitive.
    let key = SymKey::from_bytes([
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ]);
    let plaintext = b"hello, alt drive";

    let sealed = secretbox::seal(plaintext, &key);
    let opened = secretbox::open(&sealed, &key).expect("open must succeed with the sealing key");

    assert_eq!(opened, plaintext);
}

#[test]
fn secretbox_uses_unique_nonce_per_seal() {
    // Each call to seal must use a fresh random nonce. Reusing a nonce
    // under XSalsa20-Poly1305 with the same key is a cryptographic
    // catastrophe — key recovery becomes possible. The defense is
    // "always random" — verified here by checking that two consecutive
    // seals of the same plaintext produce different ciphertexts.
    //
    // Catches mutation: replacing `randombytes_buf` with a fixed
    // nonce (e.g., `[0u8; 24]`).
    let key = SymKey::from_bytes([0x42; 32]);
    let plaintext = b"the same plaintext, twice";

    let sealed_a = secretbox::seal(plaintext, &key);
    let sealed_b = secretbox::seal(plaintext, &key);

    // Both must round-trip correctly...
    assert_eq!(
        secretbox::open(&sealed_a, &key).expect("sealed_a opens"),
        plaintext
    );
    assert_eq!(
        secretbox::open(&sealed_b, &key).expect("sealed_b opens"),
        plaintext
    );
    // ...but the on-the-wire bytes must differ (because of distinct nonces).
    assert_ne!(sealed_a, sealed_b);
}

#[test]
fn secretbox_rejects_tampered_ciphertext() {
    // AEAD authenticity: any single-bit modification to the sealed
    // bytes must cause open to fail. Without this property, an
    // attacker who can modify ciphertext-in-transit could substitute
    // arbitrary garbage and the receiver would accept it.
    //
    // Catches mutation: removing the auth-tag verification, or
    // returning Ok regardless of the underlying AEAD result.
    let key = SymKey::from_bytes([0x42; 32]);
    let mut sealed = secretbox::seal(b"important secret", &key);

    // Flip a bit somewhere in the ciphertext region (past the nonce).
    let target_index = sealed.len() / 2;
    sealed[target_index] ^= 0x01;

    assert!(secretbox::open(&sealed, &key).is_err());
}

#[test]
fn secretbox_rejects_wrong_key() {
    // AEAD confidentiality property surfaced via authentication:
    // a different key must produce an auth-tag mismatch on open.
    // Without this, the key hierarchy unwrap chain would not be
    // protected — anyone with any key could attempt to unwrap any
    // wrapped key.
    //
    // Catches mutation: ignoring the key argument in open.
    let key_a = SymKey::from_bytes([0x42; 32]);
    let key_b = SymKey::from_bytes([0x43; 32]);
    let sealed = secretbox::seal(b"the secret", &key_a);

    assert!(secretbox::open(&sealed, &key_b).is_err());
}

#[test]
fn secretbox_rejects_truncated_input() {
    // Input shorter than NONCE_BYTES + TAG_BYTES (24 + 16 = 40)
    // cannot possibly be a valid sealed blob. The function must
    // reject it cleanly rather than panic or read out-of-bounds.
    //
    // Catches mutation: removing the length precheck.
    let key = SymKey::from_bytes([0x42; 32]);
    let too_short = [0u8; 39];

    assert!(secretbox::open(&too_short, &key).is_err());
}
