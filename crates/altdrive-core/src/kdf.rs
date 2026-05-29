//! Argon2id key-derivation function — converts a user password into
//! the 32-byte KEK that unwraps the masterKey on every vault unlock.
//!
//! Underlying construction: libsodium's `crypto_pwhash` with
//! `PasswordHashAlgorithm::Argon2id13`.
//! See `../../../DESIGN.md` §3 for the algorithm decision (Argon2id at
//! SENSITIVE-tier parameters: ops=8, mem=512 MiB targeting ~2-second
//! derivation on a 2020-era laptop) and §4 for where the KEK fits in
//! the unwrap chain (password → KEK → masterKey → collectionKey →
//! fileKey).

use dryoc::classic::crypto_pwhash::{crypto_pwhash, PasswordHashAlgorithm};

use crate::SymKey;

/// Parameters that tune Argon2id's computational cost.
///
/// `ops_limit` is the number of iterations. `mem_limit` is the working
/// memory in bytes. Together they trade attacker cost for legitimate
/// derivation time. DESIGN.md §3 specifies `ops_limit=8`,
/// `mem_limit=512 MiB` for production vault unlock (~2-second
/// derivation on a 2020-era laptop). Tests use smaller values for
/// speed; the parameters are stored alongside the encrypted masterKey
/// so the same values are used at every unlock.
#[derive(Debug, Clone, Copy)]
pub struct KdfParams {
    /// Number of Argon2id iterations.
    pub ops_limit: u64,
    /// Maximum working memory in bytes.
    pub mem_limit: usize,
}

/// Failure to derive a KEK.
///
/// Returned when the underlying Argon2id primitive rejects the inputs
/// (typically: parameter values below libsodium's minimums, or
/// allocation failure at the requested memory limit). The variant is
/// intentionally opaque to avoid leaking side-channel information.
#[derive(Debug)]
pub struct KdfError;

impl core::fmt::Display for KdfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("failed to derive key from password")
    }
}

impl std::error::Error for KdfError {}

/// Derive a 32-byte KEK from a password and salt using Argon2id.
///
/// The same `(password, salt, params)` triple must always produce the
/// same KEK — this is the determinism property the unlock flow relies
/// on. If Argon2id ever rejects the inputs (e.g., parameters below
/// libsodium's minimums), [`KdfError`] is returned.
pub fn derive_kek(
    password: &[u8],
    salt: &[u8; 16],
    params: &KdfParams,
) -> Result<SymKey, KdfError> {
    let mut output = [0u8; 32];
    crypto_pwhash(
        &mut output,
        password,
        salt,
        params.ops_limit,
        params.mem_limit,
        PasswordHashAlgorithm::Argon2id13,
    )
    .map_err(|_| KdfError)?;
    Ok(SymKey::from_bytes(output))
}
