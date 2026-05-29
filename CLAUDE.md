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
├── CLAUDE.md              # this file
├── Cargo.toml             # workspace
├── crates/
│   └── altdrive-core/     # pure crypto + key hierarchy + vault format
└── docs/
    ├── phase-0-spikes.md  # the six spikes to validate design choices
    └── threat-model.md    # adversary models, attack scenarios, mitigations
```

## Phase 0 status

Phase 0 deliverables that exist:
- `README.md` — strategic spec, comparing P2P-vault vs Proton-Drive-server-of-record
- `DESIGN.md` — Phase 0 operational spec with decisions log
- `docs/phase-0-spikes.md` — six spikes with time-boxes and exit criteria
- `docs/threat-model.md` — STRIDE-shape threat model with 14 attack scenarios

Phase 0 deliverables not yet started:
- The six spikes themselves (Spike 1: iroh-docs; Spike 2: iroh-blobs;
  Spike 3: macFUSE; Spike 4: pairing; Spike 5: decision write-up;
  Spike 6: DESIGN.md update)
- `crates/altdrive-core/` real (TDD-driven) implementation

## Specific gotchas for this project

- **Don't write production code before tests.** A previous attempt at
  scaffolding wrote `src/lib.rs` and `src/error.rs` without tests; those
  files were torn out. The Cargo.toml workspace and crate manifest
  remain (build configuration is needed to even run a failing test, and
  is treated pragmatically as not subject to TDD).
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
