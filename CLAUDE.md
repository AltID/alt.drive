# Alt.Drive — Project Instructions

This project follows the standards and discipline defined in
`/Users/cpettet/git/chasemp/coding-agents/`. The relevant files are:

@/Users/cpettet/git/chasemp/coding-agents/CLAUDE.md
@/Users/cpettet/git/chasemp/coding-agents/agents.md
@/Users/cpettet/git/chasemp/coding-agents/tdd-guardian.md
@/Users/cpettet/git/chasemp/coding-agents/rust-enforcer.md

## Project Context

**Alt.Drive** is the encrypted personal vault substrate spec'd in
`README.md` and `DESIGN.md`. Current phase: Phase 0 (design + spikes,
pre-implementation).

## Discipline (project-specific reinforcement of the chasemp/coding-agents standards)

1. **TDD is non-negotiable.** Every line of production code in any
   `crates/*/src/` directory must be written in response to a failing
   test. RED → VERIFY RED → GREEN → VERIFY GREEN → MUTATE → REFACTOR.
   See `tdd-guardian.md` for the full cycle and the mental mutation pass.

2. **The Rust enforcer applies.** See `rust-enforcer.md`. Key points
   specific to this codebase:
   - **Secret material always wraps in `Zeroize`** — masterKey,
     collectionKey, fileKey, recoveryKey, KEK, devicePrivateKey. No
     exceptions. The threat model (`docs/threat-model.md`) depends on this.
   - **No `Debug` derive on secret newtypes** — manual `Debug` impl that
     prints `<redacted>` only, or no `Debug` at all.
   - **No `unwrap()` outside `#[cfg(test)]`** — crypto failures must
     propagate as `Result<T, Error>`.
   - **`unsafe` blocks** — none expected for altdrive-core. If one becomes
     necessary, follow the `// SAFETY:` comment discipline strictly.

3. **No category of production code is exempt from TDD.** Type
   definitions, error enums, key hierarchy structs, vault format parsers
   — all driven by failing tests first. The thought "this is just data,
   it doesn't need a test" is the signal to stop and write the test.

4. **Crypto primitives — verify against test vectors.** Where possible,
   use published test vectors (libsodium's test suite, RFC 7539
   ChaCha20-Poly1305 vectors, RFC 8032 Ed25519 vectors, BIP39 official
   test vectors) as the first failing tests. We are not designing new
   crypto; we are correctly applying well-defined primitives.

5. **Wait for commit approval** before every commit. The user makes
   commits, not Claude.

## Project Layout

```
alt-drive/
├── README.md              # v0 spec (strategic)
├── DESIGN.md              # vault format + crypto + sync protocol (operational)
├── VALIDATION.md          # Phase 0 milestone walkthrough + cross-node KAT
├── CLAUDE.md              # this file
├── Cargo.toml             # workspace (deps: zeroize, dryoc — no iroh yet)
├── Cargo.lock             # locked for cross-node KAT reproducibility
├── crates/
│   └── altdrive-core/     # pure crypto + key hierarchy + vault format
│       ├── src/
│       │   ├── lib.rs       # SymKey (Zeroize + ZeroizeOnDrop, no Debug)
│       │   ├── kdf.rs       # Argon2id KEK derivation
│       │   └── secretbox.rs # XSalsa20-Poly1305 AEAD primitive
│       └── tests/
│           ├── sym_key.rs   # 3 tests
│           ├── secretbox.rs # 5 tests
│           └── kdf.rs       # 4 tests (incl. KAT)
└── docs/
    ├── phase-0-spikes.md  # the six (+ deferred Spike 7) spikes
    ├── threat-model.md    # adversary models, attack scenarios, mitigations
    ├── roadmap.md         # Now / Next / After + open-decisions register
    └── transport-layers.md# iroh-vs-Veilid layer map, breakpoint, narrow-port guardrail
```

## Phase 0 status

Phase 0 deliverables that exist:
- `README.md` — strategic spec, comparing P2P-vault vs Proton-Drive-server-of-record
- `DESIGN.md` — Phase 0 operational spec with decisions log (most recent entries 2026-06-03)
- `VALIDATION.md` — milestone walkthrough; documents the cross-node Argon2id KAT vector
- `docs/phase-0-spikes.md` — six spikes (+ deferred Spike 7 for iOS-iroh runtime)
- `docs/threat-model.md` — STRIDE-shape threat model with 14 attack scenarios
- `docs/roadmap.md` — Now / Next / After framing + open-decisions register
- `docs/transport-layers.md` — iroh-vs-Veilid layer map; the iroh-blobs breakpoint; the
  narrow-port guardrail that lets the transport decision stay reversible
- `crates/altdrive-core/` — three TDD-driven crypto primitives (12 tests, all green):
  - `SymKey` — 32-byte zeroizing secret-material newtype (no Debug derive)
  - `secretbox::{seal, open}` — XSalsa20-Poly1305 AEAD on `nonce || ct || tag`
  - `kdf::derive_kek` — Argon2id13 KEK derivation, anchored by a captured KAT vector

Phase 0 deliverables not yet started:
- The six spikes themselves (Spike 1: iroh-docs; Spike 2: iroh-blobs;
  Spike 3: macFUSE; Spike 4: pairing; Spike 5: decision write-up;
  Spike 6: DESIGN.md update). Throwaway crates under `crates/altdrive-spike-*/`.
  Spike 7 (iOS-iroh-blob) is DEFERRED until a physical iOS device is available.
- No `docs/spike-results/` or `docs/decisions/` yet — created when spikes run.
- Higher-level vault operations (`Vault::create`, `Vault::unlock`,
  `Vault::seal_collection_key`) and on-disk format parsers are Phase 1.

## Specific gotchas for this project

- **Don't write production code before tests.** Every item currently in
  `crates/altdrive-core/src/` was driven by a failing test under `tests/`
  first — that is the standard, not the exception. Build configuration
  (`Cargo.toml`, workspace manifest) is treated pragmatically as not
  subject to TDD; everything else is. If you find yourself writing a
  type or function "for the next test," stop and write the test first.
- **No `error.rs` aggregate module.** Errors are defined per-module
  (`KdfError` in `kdf.rs`, `OpenError` in `secretbox.rs`), kept opaque
  to avoid side-channel leaks, and propagated as `Result`. Do not
  introduce a crate-wide `Error` enum without a test that forces it.
- **The library is the most-cited document** in the parent project
  (`../vivian-main/transcripts/`). Decisions here have downstream
  consequences across the transcript library. Update `README.md` and
  `DESIGN.md` deliberately, with rationale captured in the decisions log.
- **Comparison with Proton Drive is load-bearing.** The README's §10
  comparison is the strategic justification for the entire design. Keep
  it current as design decisions evolve.
- **Phase 0 spikes are not implementation.** Spike code lives in
  throwaway crates (`crates/altdrive-spike-*/`) and is meant to validate
  decisions, not to evolve into the real implementation. Phase 1's first
  line of production code starts from a fresh, TDD-driven `altdrive-core`.

## When in doubt

- TDD violations: stop, write the failing test, watch it fail, then proceed
- Crypto questions: consult `DESIGN.md` §3 first; check libsodium/dryoc
  docs second; never invent new constructions
- Threat-model questions: consult `docs/threat-model.md`; if the scenario
  isn't covered, add it
- Architecture questions: consult `README.md` §I-IV; if the question
  isn't covered, escalate before deciding

The chasemp/coding-agents standards (imported above) are authoritative.
This file extends them with project-specific reinforcement, not exceptions.
