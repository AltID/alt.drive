# Alt.Drive — Threat Model

A deeper treatment of `DESIGN.md` §2. Where DESIGN.md gives the one-page
summary, this document enumerates assets, adversaries, attack scenarios,
and mitigations in detail.

**Status**: Phase 0 draft. Will be revisited as the spikes surface new
attack surface (new ALPN protocols, new sync messages, new key-handoff
flows).

---

## 1. Assets

Things we're trying to protect, ranked by sensitivity:

| ID | Asset | Sensitivity | Where it lives |
|---|---|---|---|
| A1 | Plaintext file content | High | Memory during de/encrypt; mount decryption layer |
| A2 | Plaintext file metadata (names, sizes, paths, modified times) | High | iroh-docs manifest values are encrypted; the local index DB stores plaintext (file paths) but is itself encrypted at rest |
| A3 | masterKey | Critical | Memory while vault is open; otherwise wrapped on disk |
| A4 | collectionKeys (per vault) | High | Memory while in use; wrapped on disk |
| A5 | fileKeys (per file) | Medium | Derived on-demand from collectionKey; not persisted unwrapped |
| A6 | devicePrivateKey (X25519 + Ed25519) | High | Memory while node is running; wrapped on disk |
| A7 | recoveryKey (the BIP39 mnemonic) | Critical | User's physical possession (paper, password manager); we never persist it |
| A8 | KEK (Argon2id output) | High | Memory only during password verification; never persisted |
| A9 | Password | Critical | User's head; we never persist; only the Argon2id-derived KEK touches code briefly |
| A10 | Vault structure metadata (vault IDs, peer relationships, taint table) | Medium | `peers.db` plaintext (NodeIds and labels — these are intentionally local-visible) |
| A11 | NodeIds of paired peers | Low (intentionally local-visible) | `peers.db` plaintext |
| A12 | iroh-docs replica state | Encrypted-at-rest via the manifest value encryption | iroh-docs replica file on disk |
| A13 | The vault's existence | Medium (metadata leak risk) | Implied by file-system presence of vault directory |

**Note**: NodeIds are public by design (they're the addressing mechanism). They
leak "you have a connection with someone whose pubkey is X" to anyone observing
your network traffic. We accept this — true unlinkability would require Tor or
similar.

---

## 2. Adversary models

Each adversary is named, characterized, and assigned a threat level. The
threat model below addresses each.

### M1 — Passive network observer

**Who**: ISP, coffee-shop wifi snooper, anyone on the wire between you and
iroh's relays.
**Capabilities**: Read all traffic in transit. Cannot inject or modify.
**Threat level**: Medium (constant exposure, low impact).

### M2 — Active network attacker (local)

**Who**: Same wire as M1, but capable of TCP/UDP injection, BGP-shape
hijacks, etc.
**Capabilities**: Modify, drop, or inject traffic. Cannot break TLS/QUIC
crypto.
**Threat level**: Medium.

### M3 — Compromised iroh relay

**Who**: Operator of an iroh relay server we use (iroh.network defaults or
self-hosted).
**Capabilities**: Sees encrypted QUIC traffic + NodeIds + relay-level
metadata (which NodeIds talk to which, when, how much). Cannot read
content.
**Threat level**: Medium.

### M4 — Device theft (with strong password)

**Who**: Someone who steals or finds your unlocked-but-app-closed laptop
or phone.
**Capabilities**: Full disk access. Cannot trivially extract masterKey
because of Argon2id cost. Brute force possible but expensive.
**Threat level**: High impact, low likelihood (depends on password
strength).

### M5 — Device theft (with weak password)

**Who**: Same as M4.
**Capabilities**: Argon2id slows them down but doesn't stop them. ~$1000
of cloud compute can attempt ~10^7 weak passwords in days.
**Threat level**: Critical for users with weak passwords.

### M6 — Malicious paired peer

**Who**: Someone you paired with who turns hostile (or whose account is
compromised after pairing).
**Capabilities**: They have the wrapped collectionKeys for vaults they
were granted. They can read those vaults. They can write to vaults they
have write access to.
**Threat level**: Whatever level the vaults they accessed deserve. They
were granted access by definition.

### M7 — Compromised paired peer (without the user's knowledge)

**Who**: Same as M6, but the user doesn't know yet.
**Capabilities**: All of M6, plus continuing access until the user
revokes.
**Threat level**: High impact, mitigatable by re-keying once detected.

### M8 — Malware on the user's device

**Who**: Userland malware, browser exploit, supply-chain attack.
**Capabilities**: Full access to userland memory. Can extract masterKey
from RAM while vault is open. Can keylog the password.
**Threat level**: Critical when present. Effectively defeats E2EE.

### M9 — Quantum adversary

**Who**: A future actor with a sufficiently large quantum computer.
**Capabilities**: Break X25519 and Ed25519. Symmetric AEAD with 256-bit
keys is still safe.
**Threat level**: Low today, planning required for "harvest now, decrypt
later" timeline.

### M10 — Subpoena / legal compulsion

**Who**: Law enforcement, court orders.
**Capabilities**: Compel anyone in possession of data to produce it.
Cannot trivially compel the user to produce a password (varies by
jurisdiction — 5th Amendment in US, but UK's RIPA is different).
**Threat level**: Depends on jurisdiction and content. Alt.Drive's
no-operator design means no one *else* can be compelled to produce
your ciphertext.

### M11 — Supply chain attack (Alt.Drive itself)

**Who**: An attacker compromises the Alt.Drive release artifacts or a
dependency.
**Capabilities**: Distribute a malicious build that exfiltrates keys.
**Threat level**: Critical. The single highest residual risk for any
serious E2EE system.

### M12 — User error

**Who**: The user.
**Capabilities**: Lose the password and the mnemonic. Share the wrong
vault with the wrong person. Click "approve" on a pairing they didn't
initiate.
**Threat level**: Variable. The most common cause of data loss in
practice.

---

## 3. Trust boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│  User's head                                                     │
│   - Password                                                     │
│   - BIP39 recovery mnemonic                                      │
└─────────────────────────────────────────────────────────────────┘
                                │
                                │  typed into device
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  User's device (laptop / phone / NUC) — TRUST BOUNDARY           │
│   - masterKey in memory while vault open                         │
│   - Unwrapped collectionKeys / fileKeys                          │
│   - Decrypted file content while reading                         │
│   - devicePrivateKey while node is running                       │
│                                                                  │
│   Threats inside this boundary: M4, M5, M8                       │
│   Mitigations: OS-level security (FileVault, BitLocker), strong  │
│   password, OS keychain integration (v0.5 enhancement),          │
│   process-level memory protection                                │
└─────────────────────────────────────────────────────────────────┘
                                │
                                │  encrypted QUIC over UDP
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  Network path — UNTRUSTED                                        │
│   - ISP, wifi, intermediate routers                              │
│   - iroh relays (when hole-punching fails)                       │
│                                                                  │
│   What's visible here: encrypted QUIC, NodeIds, traffic timing,  │
│   traffic volume                                                 │
│                                                                  │
│   Threats: M1, M2, M3                                            │
│   Mitigations: QUIC's TLS 1.3, all content layered-encrypted     │
│   under that (defense in depth)                                  │
└─────────────────────────────────────────────────────────────────┘
                                │
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  Paired peer's device — TRUSTED (within share scope)             │
│   - They hold sealed-to-them collectionKeys for shared vaults    │
│   - They have plaintext access to those vaults' content          │
│                                                                  │
│   Threats: M6, M7                                                │
│   Mitigations: explicit pairing model (you choose), per-vault    │
│   sharing (only shared vaults are visible to them), revocation   │
│   via re-keying                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 4. Attack scenarios and mitigations

### S1 — Network observer reads file content (M1, M2)

**Attack**: Attacker on the wire between Chase's laptop and his NUC tries
to read the contents of a file being synced.
**Mitigation**:
1. iroh transport: QUIC with TLS 1.3 (DH key agreement per connection,
   not vulnerable to passive replay).
2. Content layer: file ciphertext is XChaCha20-Poly1305 encrypted with a
   fileKey that the observer doesn't have.
3. Even if QUIC were broken, the inner ciphertext remains unbroken.
**Residual risk**: Traffic analysis (timing, size patterns). Sophisticated
adversary could infer "Chase synced a ~5MB file at 14:32" without knowing
content. Acceptable.

### S2 — Network observer infers file metadata (M1)

**Attack**: Observer infers which files are being modified, which folders
are growing, who's sharing with whom.
**Mitigation**: All manifest updates are also encrypted (iroh-docs values
are secretbox-encrypted before storage; keys are deterministically
encrypted). Observer sees only opaque blobs flowing.
**Residual risk**: Per-blob size + timing leaks. We do not pad blobs to
fixed sizes (would be expensive). Acceptable for v0; document.

### S3 — Compromised relay learns peer relationships (M3)

**Attack**: iroh.network is subpoenaed/compromised. They produce records
of which NodeIds connected to which.
**Mitigation**:
1. NodeIds don't link to identities by default (unless the user publishes
   them).
2. For higher-threat users: run your own iroh relay (iroh supports this).
3. For maximum: route iroh traffic through Tor (iroh has Tor support;
   v0.5+).
**Residual risk**: Peer relationships visible to relay operators by
default. v0 documents this; users who need stronger unlinkability route
through their own relays.

### S4 — Stolen device, attacker tries password brute force (M4)

**Attack**: Laptop stolen. Attacker images the disk, extracts
`keys/master.enc` + `keys/master.params`. Tries to brute-force the password.
**Mitigation**:
1. Argon2id with SENSITIVE parameters (ops=8, mem=512MiB). Each guess
   costs ~2 seconds on a single core, much more on cloud GPU/ASIC
   (Argon2id is memory-hard).
2. Password strength matters: 12+ character passphrase (4+ random words)
   resists $10K of cloud compute over months.
**Residual risk**: Weak passwords (single dictionary word, short
character strings) are recoverable. v0 enforces minimum 12 characters
and uses zxcvbn-style strength estimation; rejects "password123"-class
choices.

### S5 — Stolen device, attacker bypasses password via OS-level keychain (M4, M5)

**Attack**: User stored password in macOS Keychain. Attacker boots the
device, uses Touch ID (or your fingerprint, if they coerce you) to unlock
Keychain.
**Mitigation v0**: Don't store password in Keychain by default. v0.5 will
offer Secure Enclave wrapping (Touch ID required for additional access
factor beyond password).
**Residual risk**: Some users will store passwords in Keychain anyway.
Document the tradeoff loudly.

### S6 — Paired peer turns hostile (M6, M7)

**Attack**: A friend you shared a vault with starts copying its content
elsewhere, or you discover their device was compromised.
**Mitigation**:
1. **Revocation by re-keying**: generate new collectionKey, re-wrap to
   remaining peers, encrypt new file/manifest writes with the new key.
   The peer's existing copies of old files remain readable (they had
   the old key), but new writes are unavailable to them.
2. **Eviction from taint table**: remove their NodeId from your
   `peers.db`. They cannot sync new manifest updates from you.
**Residual risk**: They already have plaintext of everything you shared
up to the revocation point. Re-keying does not retroactively un-share.
Document this prominently.

### S7 — MitM during pairing (M2)

**Attack**: Attacker intercepts the ticket exchange between two devices
during initial pairing, substitutes their own ephemeral pubkey,
impersonates each side to the other.
**Mitigation**:
1. The 6-digit pairing code is mixed into the session key derivation
   (DESIGN.md §8.2). Attacker who substituted ephemeral pubkeys but doesn't
   know the pairing code derives a different session key on each side.
2. The 4-word visual phrase confirmation (DESIGN.md §8.3) lets users
   detect a successful MitM: their displayed phrases will not match if
   an attacker is in the middle.
**Residual risk**:
- 6-digit code has 10^6 = 1M entropy. An active on-path attacker who can
  rate-limit-evade could brute-force in the 10-minute ticket window. v0.5
  may enforce rate limiting on the listener side.
- Users who skip visual phrase confirmation lose the defense-in-depth
  layer. UX should make skipping deliberately friction-y.

### S8 — Malware on user device extracts masterKey (M8)

**Attack**: Browser zero-day → userland code execution → reads Alt.Drive
process memory while vault is open.
**Mitigation v0**: None effective. This is the fundamental limitation of
software-defined E2EE on general-purpose computers.
**Mitigation v0.5+**:
- Process isolation (Apple App Sandbox, Linux namespaces)
- Hardware-backed key storage (Secure Enclave, TPM) — masterKey unwrapped
  inside hardware enclave, never touches OS memory directly
- Use specific entitlements to limit which APIs the Alt.Drive process
  can call
**Residual risk**: A truly motivated attacker with a userland exploit
defeats v0 E2EE. The honest framing: Alt.Drive protects against the data
leaving, not against an attacker who's already inside.

### S9 — Quantum adversary harvests now, decrypts later (M9)

**Attack**: Adversary captures all your network traffic today and stores
it. When quantum computers can break Curve25519, they retroactively
decrypt the captured traffic.
**Mitigation**:
- QUIC's session keys are derived per-connection — even broken
  Curve25519 doesn't give past traffic, only future session establishment
- Symmetric encryption (XChaCha20-Poly1305 with 256-bit keys) is
  quantum-safe under Grover's algorithm (effective security 128 bits,
  still adequate)
- The masterKey is symmetric (32 bytes) — quantum attacks on it require
  Grover, not Shor; 128-bit effective security is acceptable
**Residual risk**:
- X25519 key wraps in `vault_shares.encrypted_collection_key` could be
  unwrapped by Shor's algorithm if captured. The captured ciphertext
  remains valuable indefinitely.
- v1+ migration path: replace X25519 with a PQ KEM (Kyber, FrodoKEM, etc.)
  when libsodium ships one. Plan, not built.

### S10 — Subpoena to Alt.Drive (M10)

**Attack**: Law enforcement subpoenas "the operator of Alt.Drive."
**Mitigation**: There is no operator. The cooperative legal structure
(when it exists) holds copyright + maintains code, but has no servers,
no user data, no recovery keys. Subpoena recipient: the cooperative
hands over the source code (already public) and a notarized statement
that they have no user data. Nothing else available.
**Residual risk**: A subpoena to the *user* still works. The user can
be compelled to produce their password (jurisdiction-dependent). This
is no different from any other E2EE system.

### S11 — Subpoena to iroh.network (M3, M10)

**Attack**: Law enforcement subpoenas iroh's relay infrastructure.
**Mitigation**:
- Relay sees encrypted QUIC + NodeIds + timing/volume metadata.
- They can produce "NodeId X talked to NodeId Y on these timestamps"
  but cannot produce content.
- For higher-threat users: self-hosted relay.
**Residual risk**: Peer-relationship metadata is recoverable from relay
records. Acceptable for v0; users requiring stronger anti-surveillance
properties route through their own relays or Tor.

### S12 — Supply chain attack on Alt.Drive (M11)

**Attack**: Attacker pushes a malicious release of Alt.Drive that
exfiltrates masterKey on next vault unlock.
**Mitigation v0**:
- Open source — code is auditable
- Build from source instructions for paranoid users
**Mitigation v0.5+**:
- Reproducible builds (Bazel-shape or Nix-shape)
- Code-signing with a hardware-backed signing key
- Sigstore-style transparency log for releases
**Residual risk**: The hardest threat to address. Even open source +
reproducible builds requires the user to actually verify, which most
won't. Document the threat; offer the tools for the paranoid; accept the
rest.

### S13 — User loses both password and mnemonic (M12)

**Attack**: User error. They forget the password and lose the recovery
mnemonic.
**Mitigation**: None. By design.
**Residual risk**: Data loss. v0 emphasizes the mnemonic with anti-skip
UX (require typing back 4 random words at setup); v0.5 may offer
optional Shamir social recovery (split the mnemonic across 3-of-5
trusted contacts).

### S14 — User accidentally pairs with attacker's device (M12)

**Attack**: User generates a ticket, doesn't notice it's being shoulder-
surfed or that the QR code was captured by a hidden camera, and the
attacker's device pairs.
**Mitigation**:
1. 6-digit pairing code (must be communicated separately from the QR
   code).
2. Visual phrase confirmation on both sides.
3. Ticket expiry (10-minute window).
**Residual risk**: User skips the visual phrase confirmation or shares
the pairing code over a compromised channel. UX matters here.

---

## 5. Trust assumptions

What we explicitly assume:

1. **libsodium / dryoc / RustCrypto primitives are correct.** We trust
   the cryptographic implementations. If they're wrong, the system is
   broken in ways we can't recover from at the application layer.
2. **The OS is not actively malicious.** macFUSE / FUSE / FileProvider
   accurately report file operations. The OS does not silently leak the
   plaintext mount contents.
3. **iroh's hole-punching does not accidentally connect us to the wrong
   peer.** We rely on iroh's authentication of NodeIds. A bug there
   would be catastrophic.
4. **BIP39 mnemonic encoding is reversible without information loss.**
   24-word mnemonic encodes 256 bits + 8-bit checksum cleanly.
5. **The system clock is roughly correct.** Conflict resolution uses
   timestamps; wildly wrong clocks (decades-off) cause weird behavior.

---

## 6. Comparison with related projects

How Alt.Drive's threat model compares to closest peers:

### vs Proton Drive

**Stronger**:
- No operator to compel or compromise (S10, S11 partially)
- No central directory of users that could leak

**Weaker**:
- No professional security team auditing infrastructure
- No coordinated incident response if a vulnerability is found in
  Alt.Drive
- Recovery is weaker (no server-mediated recovery flow)

### vs Cryptomator

**Stronger**:
- Active sync layer with conflict resolution; Cryptomator relies on
  whatever cloud storage you stuff the vault into
- Direct E2EE on the wire; Cryptomator's transit is whatever the
  hosting provider gives you

**Weaker**:
- More complex codebase = larger attack surface
- More protocols (iroh, manifest sync) = more places bugs can hide

### vs Syncthing

**Stronger**:
- E2EE at rest by default; Syncthing only encrypts in transit unless
  you opt into receive-encrypted mode
- Sharing model is per-vault, not per-folder-on-disk

**Weaker**:
- Newer; less battle-tested
- More moving parts (vault format + sync + crypto + mount)

### vs Tahoe-LAFS

**Stronger**:
- Modern UX (in time); Tahoe-LAFS never crossed the chasm
- Modern crypto primitives (Curve25519 vs RSA-era choices in Tahoe)

**Weaker**:
- Less rigorous formal modeling. Tahoe-LAFS had Brian Warner et al.
  doing capability-string design with mathematical care for years.

---

## 7. Residual risks and future directions

The honest list of what we know we don't fully solve:

1. **Malware on the user's device defeats E2EE.** v0.5+ explores
   hardware-backed key storage (Secure Enclave, TPM). Won't fully solve
   but raises the bar significantly.
2. **Traffic analysis at the iroh transport layer leaks peer-relationship
   and timing information.** v1+ could integrate Tor or Nym for users who
   need it.
3. **Quantum migration is unaddressed in v0.** v1+ migrates X25519 to a
   post-quantum KEM once libsodium ships one. Symmetric primitives
   remain safe.
4. **Supply chain integrity is informal.** v0.5+ adds reproducible
   builds and Sigstore-shape transparency log.
5. **No formal security audit yet.** v1 should include a paid audit
   from a reputable firm before encouraging non-technical users to
   adopt.
6. **Weak passwords remain a class break.** v0 enforces minimum length +
   strength estimation; v0.5 explores biometric/hardware factors.

---

## 8. STRIDE summary

| STRIDE category | Coverage |
|---|---|
| **Spoofing** | iroh NodeId authentication + per-device key wraps prevent peer impersonation. Pairing has 6-digit code + visual phrase against MitM. |
| **Tampering** | All content is AEAD-authenticated. Manifest signatures (Ed25519) authenticate the last-writer of each entry. iroh-blobs verifies BLAKE3 hash on receive. |
| **Repudiation** | Manifest entries are signed by `modified_by_node_id`. We do not provide non-repudiable timestamps (relies on local clocks). |
| **Information Disclosure** | Plaintext never leaves the device. AEAD on all content + manifest. Relay sees only encrypted QUIC. |
| **Denial of Service** | A malicious paired peer can fill your blob store with garbage (limited by your taint-table grants). A malicious relay can refuse to relay. v0: trust your paired peers; v0.5: storage quotas per peer. |
| **Elevation of Privilege** | Per-vault sharing means a peer with access to vault A cannot access vault B unless explicitly granted. Revocation is via re-key, not retroactive. |

---

## 9. Open security questions (Phase 0 unknowns)

To revisit after spikes:

1. **iroh-docs concurrent-write semantics**: does our model of
   last-writer-wins with `(ts, node_id)` tiebreak survive contact with
   reality? (Spike 1)
2. **macFUSE writeback timing**: are there windows where plaintext can
   appear on disk in writeback caches before encryption? (Spike 3)
3. **Pairing code brute-force window**: should we enforce rate limiting
   on the existing-device listener? How tight? (Spike 4)
4. **Memory locking**: should the masterKey live in `mlock`'d memory?
   Worth the engineering complexity in v0?
5. **Key zeroization on panic**: do we have a robust way to ensure
   unwrapped keys get zeroed even when the process panics? `zeroize`
   crate helps but isn't bulletproof.

---

## 10. Threat model maintenance

This document should be updated:

- When a new attack surface is introduced (new ALPN, new sync message
  type, new key-handoff flow)
- When a spike reveals a previously-unconsidered scenario
- Before any external audit
- Annually as a calendar review even if nothing else triggers it

Each update should be dated and the changelog kept at the bottom.

---

## Changelog

| Date | Change |
|---|---|
| 2026-05-28 | Initial draft (Phase 0). |
