//! Behavior tests for the Argon2id key-derivation function — the entry
//! point of the Alt.Drive key-hierarchy unwrap chain.
//!
//! `derive_kek(password, salt, params)` converts a user password into a
//! deterministic 32-byte key-encryption-key (KEK). The KEK is then used
//! to unwrap the encrypted masterKey on every vault unlock; from there
//! the rest of the hierarchy unwraps via secretbox.
//!
//! See `../../../DESIGN.md` §3 (Argon2id with SENSITIVE parameters as
//! the password-to-KEK construction) and §4 (the unwrap chain).
//!
//! Tests use small Argon2id parameters (ops=2, mem=8 MiB) for speed —
//! production unlock uses the SENSITIVE-tier params (ops=8, mem=512 MiB)
//! that take ~2 seconds per derivation.

use altdrive_core::kdf;

/// Shared test parameters — small enough to keep tests fast, large
/// enough to satisfy libsodium's minimums. Production unlock uses
/// the SENSITIVE values from DESIGN.md §3 (ops=8, mem=512 MiB).
fn test_params() -> kdf::KdfParams {
    kdf::KdfParams {
        ops_limit: 2,
        mem_limit: 8 * 1024 * 1024, // 8 MiB
    }
}

#[test]
fn kdf_derives_same_kek_for_same_inputs() {
    // The vault-unlock flow re-derives the KEK from the password on
    // every unlock. The KDF must be deterministic — same inputs must
    // produce the same 32 bytes — or unlocks would fail
    // non-deterministically.
    let password = b"correct horse battery staple";
    let salt: [u8; 16] = [0x42; 16];

    let kek_a = kdf::derive_kek(password, &salt, &test_params()).expect("known-good params");
    let kek_b = kdf::derive_kek(password, &salt, &test_params()).expect("known-good params");

    assert_eq!(kek_a.expose_secret(), kek_b.expose_secret());
}

#[test]
fn kdf_is_password_sensitive() {
    // Two different passwords with the same salt must produce
    // different KEKs. Without this, anyone could unlock any vault
    // (and a wrong password would silently produce the right KEK).
    //
    // Catches mutation: ignoring the password argument in derive_kek
    // (e.g., always hashing a constant).
    let salt: [u8; 16] = [0x42; 16];
    let kek_a = kdf::derive_kek(b"password one", &salt, &test_params()).expect("known-good params");
    let kek_b = kdf::derive_kek(b"password two", &salt, &test_params()).expect("known-good params");

    assert_ne!(kek_a.expose_secret(), kek_b.expose_secret());
}

#[test]
fn kdf_is_salt_sensitive() {
    // Two different salts with the same password must produce
    // different KEKs. Without per-vault unique salts, two users with
    // the same password would share a KEK — and rainbow-table attacks
    // against common passwords would target every vault at once.
    //
    // Catches mutation: ignoring the salt argument in derive_kek.
    let password = b"correct horse battery staple";
    let salt_a: [u8; 16] = [0x01; 16];
    let salt_b: [u8; 16] = [0x02; 16];
    let kek_a = kdf::derive_kek(password, &salt_a, &test_params()).expect("known-good params");
    let kek_b = kdf::derive_kek(password, &salt_b, &test_params()).expect("known-good params");

    assert_ne!(kek_a.expose_secret(), kek_b.expose_secret());
}

#[test]
fn kdf_matches_known_answer() {
    // Known-answer test against a vector empirically captured from
    // dryoc 0.8.0's Argon2id13 implementation. Anchors the algorithm
    // identity — a mutation that substitutes a different hash (SHA256,
    // BLAKE3, plain Argon2i) would pass the deterministic +
    // sensitivity tests above but fail here.
    //
    // To re-verify against an authoritative source, run the equivalent
    // input through libsodium's crypto_pwhash directly (any language
    // binding) with the same parameters and Argon2id13 algorithm.
    //
    // Inputs:
    //   password = "alt-drive KAT vector"
    //   salt     = 0x00, 0x01, ..., 0x0f (16 bytes)
    //   ops      = 2
    //   mem      = 8 MiB
    //   alg      = Argon2id13
    let password = b"alt-drive KAT vector";
    let salt: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ];
    // Expected: empirical capture from dryoc 0.8.0's Argon2id13 on
    // the inputs above (captured 2026-05-29). If this ever changes,
    // the underlying Argon2id implementation has diverged and we need
    // to investigate before shipping a new release.
    let expected_hex = "dd4faa88d9dc3067d806d1cc27435ba72af896bb871b6990a7e070059116a535";

    let kek = kdf::derive_kek(password, &salt, &test_params()).expect("known-good params");
    let actual_hex: String = kek
        .expose_secret()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    assert_eq!(
        actual_hex, expected_hex,
        "KEK output drifted from the captured KAT vector"
    );
}
