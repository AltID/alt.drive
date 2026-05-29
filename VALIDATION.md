# Alt.Drive — Phase 0 Validation Walkthrough

This document is a step-by-step checklist for validating the current
state of Alt.Drive against expectations. Two purposes:

1. **Sync check** — anyone (Chase, a future collaborator, future-you on
   a new machine) can clone the repo, run this walkthrough, and confirm
   the state matches what was committed.
2. **Cross-node consistency** — the same cryptographic operations must
   produce identical outputs across machines, OSes, and CPU
   architectures. That's the property a P2P sovereign-vault system
   depends on. This walkthrough exercises that property explicitly
   via the KAT (known-answer-test) vector.

**Phase 0 milestone status as of this commit**: three capabilities
implemented, all TDD-driven, all tests green.

---

## Prerequisites

You need:

- **Rust toolchain** — `rustup`-managed, stable channel. Tested with
  Rust 1.75+ (see `Cargo.toml` `rust-version`).
- **A working network** for the first `cargo build` (downloads
  `dryoc`, `zeroize`, and their transitive deps from crates.io). After
  the first build, everything is local.
- **~600 MiB free disk** for `target/` (Rust release artifacts are
  chunky).
- **macOS, Linux, or Windows** — the crate has no platform-specific
  code yet. KAT vector should produce identical output on all three.

Check your toolchain:

```bash
rustc --version    # should be 1.75 or newer
cargo --version
```

---

## Step 1 — Repository state

The repository root should contain:

```
alt-drive/
├── .gitignore              # Rust + editor + Claude project-local
├── CLAUDE.md               # wires into chasemp/coding-agents
├── Cargo.lock              # locked dep versions for reproducibility
├── Cargo.toml              # workspace manifest
├── DESIGN.md               # Phase 0 operational spec
├── README.md               # strategic v0 spec
├── VALIDATION.md           # this file
├── crates/
│   └── altdrive-core/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── kdf.rs       # Argon2id KEK derivation
│       │   ├── lib.rs       # crate root, SymKey definition
│       │   └── secretbox.rs # XSalsa20-Poly1305 AEAD primitive
│       └── tests/
│           ├── kdf.rs       # 4 KDF tests
│           ├── secretbox.rs # 5 secretbox tests
│           └── sym_key.rs   # 3 SymKey tests
└── docs/
    ├── phase-0-spikes.md   # spike plan (not yet executed)
    └── threat-model.md     # adversary models + STRIDE
```

Verify:

```bash
ls -la
ls -la crates/altdrive-core/src/
ls -la crates/altdrive-core/tests/
ls -la docs/
```

Each file listed above should be present. No `target/` directory should
be in git (it's gitignored).

---

## Step 2 — Workspace builds cleanly

```bash
cargo build
```

Expected:
- First run: downloads `dryoc 0.8.x`, `zeroize 1.x`, transitive deps
  (libsodium-sys, etc.). Takes 1–3 minutes depending on machine.
- Subsequent runs: incremental, ~5 seconds.
- Final line: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in <time>`.

If you see warnings, they should be only the expected `#![warn(missing_docs)]` reminders for any newly-added items. The current
code has none.

If you see errors, the most likely causes:
- Rust toolchain older than 1.75 → upgrade with `rustup update stable`
- Missing C compiler (dryoc has small native bits) → install
  Xcode CLI tools (macOS) or build-essential (Linux)

---

## Step 3 — Test suite

```bash
cargo test
```

**Expected output (the load-bearing line):**

```
cargo test: 12 passed (5 suites, ~0.3s)
```

(Suite count includes doctests; current code has none, so 5 = the test
binaries + the library itself.)

The 12 tests, grouped by capability:

### `sym_key.rs` (3 tests)

| Test | What it verifies |
|------|------------------|
| `sym_key_preserves_bytes_for_crypto_use` | 32 bytes in, 32 bytes out — with distinct-byte test data so position-shifting mutations are caught |
| `sym_key_zeroizes_on_demand` | `Zeroize::zeroize()` overwrites the buffer with zeros |
| `sym_key_zeroizes_on_drop` | Trait-bound check that `SymKey: ZeroizeOnDrop` — anchors the threat-model property without requiring `unsafe` to read post-drop memory |

### `secretbox.rs` (5 tests)

| Test | What it verifies |
|------|------------------|
| `secretbox_round_trips_plaintext` | Seal then open returns the original plaintext |
| `secretbox_uses_unique_nonce_per_seal` | Two consecutive seals of the same plaintext produce different ciphertexts (random nonces) |
| `secretbox_rejects_tampered_ciphertext` | Single-bit flip in the ciphertext causes open to fail (AEAD authenticity) |
| `secretbox_rejects_wrong_key` | Open with a different key returns Err |
| `secretbox_rejects_truncated_input` | Input shorter than `NONCE_BYTES + TAG_BYTES` returns Err without panic |

### `kdf.rs` (4 tests)

| Test | What it verifies |
|------|------------------|
| `kdf_derives_same_kek_for_same_inputs` | Determinism — same (password, salt, params) → same KEK |
| `kdf_is_password_sensitive` | Different passwords (same salt) → different KEKs |
| `kdf_is_salt_sensitive` | Different salts (same password) → different KEKs (defeats rainbow tables across vaults) |
| `kdf_matches_known_answer` | KAT vector — anchors that the algorithm is actually Argon2id13, not a substituted hash. **See Step 5 for cross-machine implications.** |

If any test fails, stop here and investigate. The most likely cause:
underlying `dryoc` version has changed and the KAT vector needs
re-capture (see Step 5).

---

## Step 4 — Linter and formatter clean

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: `Finished` line, no warnings, no errors. If clippy fires,
the rust-enforcer agent (per chasemp/coding-agents) should be
consulted — some warnings are accepted with `#[allow(clippy::name)]`
and a comment; none should be silent.

```bash
cargo fmt --check
```

Expected: silent exit with status 0. If formatting diverges, `cargo
fmt` (without `--check`) applies the fixes.

---

## Step 5 — Cross-machine KAT verification

The `kdf_matches_known_answer` test is special: its expected output
is a hex string that should match **regardless of machine, OS, or CPU
architecture**, as long as `dryoc 0.8.0` (or the resolved
patch-compatible version) is in use.

Expected hex output:

```
dd4faa88d9dc3067d806d1cc27435ba72af896bb871b6990a7e070059116a535
```

Inputs (held constant for cross-node verification):

| Field | Value |
|-------|-------|
| Password | `"alt-drive KAT vector"` (ASCII bytes) |
| Salt | `0x00, 0x01, 0x02, ..., 0x0f` (16 bytes, sequential) |
| `ops_limit` | 2 |
| `mem_limit` | 8388608 (8 MiB) |
| Algorithm | `Argon2id13` |

If you get a different output, one of these is true:

1. **`dryoc` version drift.** Run `cargo tree | grep dryoc` and confirm
   you're on `0.8.0`. If on a later version, the KAT may need re-capture
   (see Step 6 of `docs/phase-0-spikes.md` for the procedure).
2. **A hardware-specific code path was activated** (e.g., a SIMD
   acceleration for Argon2). Argon2id should still be byte-identical
   across implementations — if you see drift here, this is a real bug
   worth investigating before any production use.
3. **The test was modified** — check `git status` and `git diff`.

To re-verify against an authoritative source independent of dryoc, run
the same inputs through `libsodium`'s `crypto_pwhash` directly (any
binding: C, Python `pynacl`, Go, etc.). The output bytes should match
exactly. **If they don't, it's a dryoc bug worth filing upstream.**

This step is the critical cross-node consistency check. Multi-device
sync depends on every node deriving the same KEK from the same inputs.
Validate this property before trusting the substrate.

---

## Step 6 — Capability walkthrough

For each capability, what it does and how it composes with the others.
This is the "narrative" view of the unwrap chain in `DESIGN.md` §4.

### Capability 1 — `SymKey` (the secret-material newtype)

**Where**: `crates/altdrive-core/src/lib.rs`

**Public API**:
- `SymKey::from_bytes([u8; 32]) -> SymKey`
- `SymKey::expose_secret(&self) -> &[u8; 32]`
- Implements `Zeroize` (explicit zeroization) and `ZeroizeOnDrop`
  (automatic zeroization when dropped).

**What it gives you**: a typed container for all 32-byte symmetric
keys in the hierarchy. Same memory layout as `[u8; 32]` but with the
zeroization discipline baked in.

**What it does NOT do**: there is no `Debug` derive. You cannot
accidentally print a `SymKey` via `println!("{:?}", key)` — that's a
compile error by design.

**Try it interactively** (from a `cargo test` or scratch binary):

```rust
use altdrive_core::SymKey;
let key = SymKey::from_bytes([0x01; 32]);
assert_eq!(key.expose_secret(), &[0x01; 32]);
// `dbg!(key)` would NOT compile — no Debug impl. Good.
```

---

### Capability 2 — `secretbox` (AEAD wrapping for the hierarchy)

**Where**: `crates/altdrive-core/src/secretbox.rs`

**Public API**:
- `secretbox::seal(&[u8], &SymKey) -> Vec<u8>` — generates a random
  24-byte nonce, encrypts, returns `nonce || ciphertext || auth_tag`.
- `secretbox::open(&[u8], &SymKey) -> Result<Vec<u8>, OpenError>` —
  parses the nonce prefix, decrypts, returns plaintext or opaque
  error.

**What it gives you**: the primitive that wraps every layer of the key
hierarchy. `seal(masterKey.expose_secret(), &kek)` produces the
encryptedMasterKey blob; `open(blob, &kek)` returns the masterKey bytes.

**Threat-model properties it provides**:
- Authenticity (AEAD): tampered ciphertext is rejected.
- Confidentiality: ciphertext reveals nothing about plaintext under a
  passive observer.
- Key-binding: `open` requires the same key that was used to `seal`.

**Try it**:

```rust
use altdrive_core::{secretbox, SymKey};
let key = SymKey::from_bytes([0x42; 32]);
let sealed = secretbox::seal(b"secret message", &key);
let opened = secretbox::open(&sealed, &key).unwrap();
assert_eq!(opened, b"secret message");

// Wrong key:
let wrong = SymKey::from_bytes([0x43; 32]);
assert!(secretbox::open(&sealed, &wrong).is_err());
```

---

### Capability 3 — `kdf` (Argon2id KEK derivation)

**Where**: `crates/altdrive-core/src/kdf.rs`

**Public API**:
- `kdf::KdfParams { ops_limit: u64, mem_limit: usize }` — Argon2id
  cost parameters. DESIGN.md §3 specifies `ops_limit=8`,
  `mem_limit=512 MiB` for production unlock.
- `kdf::derive_kek(password, &salt, &params) -> Result<SymKey,
  KdfError>` — runs Argon2id13 to produce a deterministic 32-byte KEK.

**What it gives you**: the entry point of the unwrap chain. Converts
"user typed a password" into a 32-byte KEK suitable for `secretbox::
open` against the stored `encryptedMasterKey`.

**Composition with the other capabilities** (the full unlock flow,
end-to-end):

```rust
use altdrive_core::{kdf, secretbox, SymKey};

// At vault creation (one-time):
let password = b"correct horse battery staple";
let salt: [u8; 16] = [/* 16 random bytes from CSPRNG */];
let params = kdf::KdfParams { ops_limit: 8, mem_limit: 512 * 1024 * 1024 };

let master_key = SymKey::from_bytes([/* 32 random bytes from CSPRNG */]);
let kek = kdf::derive_kek(password, &salt, &params).unwrap();
let encrypted_master_key = secretbox::seal(master_key.expose_secret(), &kek);
// Store (salt, params, encrypted_master_key) on disk.

// At every subsequent unlock:
let kek_again = kdf::derive_kek(password, &salt, &params).unwrap();
let master_key_bytes = secretbox::open(&encrypted_master_key, &kek_again).unwrap();
// master_key_bytes is now the same 32 bytes we started with.
```

This is the full key-hierarchy unwrap pattern, ready to compose into a
real `Vault::unlock(password)` function in the next phase.

---

## Step 7 — Git state verification

Three commits, all by `Chase Pettet <chase.mp@owasp.org>`:

```bash
git log --format="%h %an <%ae>  %s"
```

Expected output:

```
5fe5b40 Chase Pettet <chase.mp@owasp.org>  Add Argon2id KEK derivation (kdf::derive_kek)
0b4de13 Chase Pettet <chase.mp@owasp.org>  Add secretbox AEAD primitive (seal/open) for key-hierarchy unwrap
37c956c Chase Pettet <chase.mp@owasp.org>  Initial commit: Phase 0 design + SymKey capability
```

(Hashes may differ if commits were amended; the commit messages and
author identity should match exactly.)

Working tree should be clean:

```bash
git status
```

Expected: `nothing to commit, working tree clean`.

---

## Expected Phase 0 milestone state

**Done**:

- ✅ Phase 0 design artifacts (`README.md`, `DESIGN.md`, `CLAUDE.md`,
  `docs/threat-model.md`, `docs/phase-0-spikes.md`)
- ✅ Repo wired into `chasemp/coding-agents` (TDD discipline,
  rust-enforcer, Zeroize discipline)
- ✅ Three TDD-driven crypto primitives:
  1. `SymKey` — 32-byte zeroizing secret-material newtype
  2. `secretbox` — XSalsa20-Poly1305 AEAD for hierarchy wrap/unwrap
  3. `kdf` — Argon2id13 KEK derivation from password
- ✅ Twelve tests, all green; clippy clean; fmt clean.

**Not yet done** (intentional — Phase 0 is design + foundational
primitives only):

- ⏳ The six spikes in `docs/phase-0-spikes.md` (iroh-docs, iroh-blobs,
  macFUSE, dumbpipe-shape pairing, decision write-up, DESIGN.md
  update). These are the spikes that validate the *system* design
  decisions before Phase 1 production work begins.
- ⏳ Higher-level vault operations (`Vault::create`, `Vault::unlock`,
  `Vault::seal_collection_key`). The primitives are in place; the
  hierarchy struct work is Phase 1.
- ⏳ The on-disk vault format (parsers, serializers for `vault.json`,
  `manifest.iroh-doc`, etc.). Phase 1.
- ⏳ The iroh transport layer (sync engine, taint table, pairing). Phase 1.
- ⏳ The macFUSE mount. Phase 1.
- ⏳ All UI / app code. Phase 1.

---

## What to do if validation fails

| Symptom | Diagnosis | Fix |
|---------|-----------|-----|
| `cargo build` fails on dryoc compilation | Missing C compiler | Install Xcode CLI tools (macOS) / `build-essential` (Linux) |
| `cargo test` shows fewer than 12 passes | Test file deleted, or test renamed | `git status` to see what's modified; `git diff` to inspect |
| `kdf_matches_known_answer` fails | `dryoc` version drift, or hardware-specific code path issue | Run `cargo tree \| grep dryoc`; investigate version diff |
| `clippy` fires new warnings | Code modified without TDD; new `#[allow]` needed | Consult `rust-enforcer.md` |
| `cargo fmt --check` fails | New code added without running fmt | `cargo fmt` to fix |
| Wrong author identity on commits | `git config user.email` is set to L360 identity | Use `git -c user.email="chase.mp@owasp.org"` per commit |

---

## Next step after validation

If all of the above passes, you are in sync with the milestone. The
next development steps (suggested order):

1. **Random key generation** — `SymKey::generate()` for fresh
   masterKey / collectionKey / fileKey at vault creation time.
2. **BIP39 recovery mnemonic** — `recoveryKey ↔ 24-word phrase`
   encoding for the paper recovery flow.
3. **First Vault-level type** — `Vault::create(password)` that
   composes `kdf`, `secretbox`, and `SymKey::generate()` into a
   single end-to-end flow. The first integration test that exercises
   all three primitives together.
4. **The six Phase 0 spikes** (`docs/phase-0-spikes.md`) — iroh-docs,
   iroh-blobs, macFUSE, pairing protocol. These validate the *system*
   design before Phase 1 implementation begins.

---

## Document maintenance

Update this file whenever:

- A new capability is added (extend Steps 3 and 6).
- The KAT vector changes (update Step 5, document the change in
  `DESIGN.md`'s decisions log).
- The expected test count changes (update Step 3 header line).
- A new dependency is added (mention in Step 2 prereqs).

Last updated: 2026-05-29 (after `5fe5b40` — KDF capability).
