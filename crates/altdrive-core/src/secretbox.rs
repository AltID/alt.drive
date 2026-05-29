//! Symmetric AEAD on small fields — the primitive that wraps every
//! layer of the Alt.Drive key hierarchy and encrypts manifest entries.
//!
//! Underlying construction: libsodium's `crypto_secretbox_easy`
//! (XSalsa20-Poly1305 with a 24-byte random nonce and 16-byte auth tag).
//! See `../../../DESIGN.md` §3 for the algorithm decision and §5.4 for
//! the on-disk format (`nonce || ciphertext || auth_tag`).

use dryoc::classic::crypto_secretbox::{crypto_secretbox_easy, crypto_secretbox_open_easy, Nonce};
use dryoc::rng::randombytes_buf;

use crate::SymKey;

/// Size of the random nonce prefixed to every sealed blob.
const NONCE_BYTES: usize = 24;

/// Size of the Poly1305 authentication tag appended by the AEAD.
const TAG_BYTES: usize = 16;

/// Failure to open a sealed blob.
///
/// Returned when the ciphertext is shorter than the minimum
/// `NONCE_BYTES + TAG_BYTES`, when the auth tag does not verify against
/// the supplied key, or when the underlying primitive otherwise rejects
/// the input. The variant is intentionally opaque to avoid leaking
/// side-channel information about which check failed.
#[derive(Debug)]
pub struct OpenError;

impl core::fmt::Display for OpenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("failed to open sealed blob")
    }
}

impl std::error::Error for OpenError {}

/// Seal a plaintext under a symmetric key with a fresh random nonce.
///
/// The returned `Vec<u8>` has the layout `nonce || ciphertext` where
/// `ciphertext` includes the appended Poly1305 auth tag. The nonce is
/// generated from the OS CSPRNG (via libsodium's `randombytes_buf`).
///
/// This function does not fail under valid inputs — the underlying
/// AEAD has no input-validation failure mode at fixed key/nonce sizes.
pub fn seal(plaintext: &[u8], key: &SymKey) -> Vec<u8> {
    let mut nonce: Nonce = [0u8; NONCE_BYTES];
    let random = randombytes_buf(NONCE_BYTES);
    nonce.copy_from_slice(&random);

    let mut output = vec![0u8; NONCE_BYTES + plaintext.len() + TAG_BYTES];
    output[..NONCE_BYTES].copy_from_slice(&nonce);
    crypto_secretbox_easy(
        &mut output[NONCE_BYTES..],
        plaintext,
        &nonce,
        key.expose_secret(),
    )
    .expect(
        "crypto_secretbox_easy cannot fail because key and nonce are fixed-size arrays validated by the type system",
    );
    output
}

/// Open a sealed blob with the same key it was sealed under.
///
/// `input` must have the layout produced by `seal`: a 24-byte nonce
/// prefix followed by the ciphertext+tag. Returns the plaintext on
/// success or [`OpenError`] on any failure (input too short, auth tag
/// mismatch, wrong key).
pub fn open(input: &[u8], key: &SymKey) -> Result<Vec<u8>, OpenError> {
    if input.len() < NONCE_BYTES + TAG_BYTES {
        return Err(OpenError);
    }

    let (nonce_bytes, ciphertext) = input.split_at(NONCE_BYTES);
    let mut nonce: Nonce = [0u8; NONCE_BYTES];
    nonce.copy_from_slice(nonce_bytes);

    let plaintext_len = ciphertext.len() - TAG_BYTES;
    let mut output = vec![0u8; plaintext_len];
    crypto_secretbox_open_easy(&mut output, ciphertext, &nonce, key.expose_secret())
        .map_err(|_| OpenError)?;
    Ok(output)
}
