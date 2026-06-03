# Alt.Drive

An end-to-end encrypted personal vault that mounts as a local folder on every
device you own. Files are yours, encrypted at rest and in transit, synced
peer-to-peer between explicitly-trusted nodes you control. No SaaS, no
server-of-record, no operator who could read your data.

> *"If you had a really good encrypted, meta file system mounting like Dropbox
> or Google Drive on OSX, you could do all kinds of things with it at the
> endpoints. The data is expected to be there and trustable locally. It's a
> separation of concerns play."*

---

## Status

**Pre-implementation, planning only.** This document is the v0 spec.

---

## Intent (the separation-of-concerns framing)

Alt.Drive is **substrate**, not a feature. The product is a really good
encrypted file/metafile system that mounts everywhere. Everything else —
personal AI memory (Vivian), federated social, document workflows, photo
storage, code repos — runs *agnostic* on top of the substrate.

The substrate guarantees:

1. Bytes are E2EE at rest and in transit.
2. Sync rules between explicitly-paired peers are honored.
3. The local mount is trustable — apps on the OS read/write through it like
   any other folder.

Everything else is downstream.

This is a UNIX-flavored design: *the filesystem is the API*. Every feature
debate becomes "does this belong in the substrate, or does it run on top?"
That question alone prevents most scope creep.

---

## What v0 is, and what v0 isn't

**v0 IS:**

- A macOS native vault (one user, your own devices)
- An encrypted vault format on disk (Cryptomator-shape, Ente-shape key hierarchy)
- Peer-to-peer sync over iroh between explicitly-paired devices
- Tainting / sync rules per peer (what gets shared with whom)
- BYO third node (your own NUC, VPS, or always-on laptop = always-on peer)
- Paper-mnemonic recovery (no server fallback, no Shamir, no social recovery)

**v0 is NOT:**

- A Dropbox replacement for non-technical users
- A federated social platform
- A cooperative legal entity
- A discoverable peer-to-peer network (no DHT, no public discovery)
- A web app (no browser access in v0)
- A multi-platform release (macOS first, then Linux daemon, then iOS, etc.)
- Search (rely on macOS Spotlight indexing the mount's decrypted view)
- A sharing UX beyond your own devices

---

## The killer move (and the realistic scoping)

The honest framing: a full Dropbox-parity E2EE drive is a five-year company.
Proton Drive is the existence proof that it's possible; it also proves that
getting where they are took a decade of Proton-platform development.

But the **minimum viable Chase-uses-it-daily version is much smaller**. A
Mac native app + macFUSE mount + iroh transport + a vault format =
3-6 months of focused work for one person.

That version is independently valuable. The cooperative + federation +
anchor-peer-as-service material from transcript 262 layers on top of a
working substrate. **They are not preconditions for the substrate
existing.** They become real questions once there are real users.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  macOS Apps (Finder, your Vivian, any app reading files)         │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │  reads/writes through mount
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  Mount layer:  macFUSE → /Volumes/AltDrive                       │
│                (later: FileProvider extension, Linux FUSE, etc.) │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │  transparently encrypts/decrypts
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  Vault layer (Rust core)                                         │
│   - Key hierarchy: masterKey → collectionKey → fileKey           │
│   - Crypto: libsodium (XChaCha20-Poly1305 streaming for files)   │
│   - Filename encryption + manifest                               │
│   - Local SQLite index (encrypted-at-rest)                       │
│   - Content-addressed blob store (BLAKE3-named ciphertext)       │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │  blob sync, peer-to-peer
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│  Sync layer (iroh)                                               │
│   - NodeId = Ed25519 pubkey per device                           │
│   - QUIC over hole-punching (relay fallback for hard NATs)       │
│   - iroh-blobs for content-addressed BLAKE3 sync                 │
│   - iroh-docs OR custom CRDT for the vault manifest              │
│   - Taint table: (peer_NodeId, vault_id, mode) tuples            │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │  hardcoded NodeId list (v0)
                                ▼
                    Your devices: laptop ↔ phone ↔ NUC
```

### What iroh gives you for free

- **NAT traversal**: QUIC over hole-punching first, relay fallback for hard
  NATs. iroh.network operates default relays; you can self-host. Your devices
  reach each other transparently through home routers, cellular NAT, corporate
  firewalls.
- **NodeId addressing**: each device has an Ed25519 keypair. Authenticated
  and encrypted channels by construction.
- **iroh-blobs**: content-addressed blob sync via BLAKE3 verified streaming.
  Partial range requests, multi-source download, in-memory or filesystem
  store. The right substrate for "send this chunk of this file to this peer."
- **iroh-docs**: collaborative key-value store. Candidate for the vault
  manifest if its CRDT semantics fit; otherwise we roll our own with version
  vectors (Syncthing-shape).
- **Discovery options later**: DNS, DHT, mDNS. v0 ships with hardcoded NodeId
  list only.

### What you build on top

- The vault format (key hierarchy, on-disk layout, manifest design)
- Crypto plumbing (libsodium calls; key derivation; sharing key wrap)
- The macOS app (Swift wrapping the Rust core)
- The macFUSE mount that transparently de/encrypts
- The taint table + sync-rule enforcement
- CLI for setup, pairing, status
- Pairing flow (QR code or short-code exchange between devices)
- Paper-mnemonic recovery export at setup

---

## Vault format (the Ente-shape key hierarchy)

Following Ente's well-documented model, adapted for the file-vault case:

```
masterKey                                 # 32 bytes, per-user, never leaves device unencrypted
  │
  ├── encryptedMasterKey                  # masterKey encrypted with KEK
  │   KEK = Argon2id(password, salt)      # for password unlock on a single device
  │
  ├── encryptedRecoveryKey                # masterKey encrypted with recoveryKey
  │                                       # recoveryKey is the printed paper mnemonic
  │
  └── encryptedPrivateKey                 # device's X25519 privateKey, encrypted with masterKey
                                          # publicKey is plaintext on the network for sharing

collectionKey  (per vault folder)         # generated when the vault is created
  ├── encrypted with masterKey
  └── (later) encrypted with each recipient's publicKey when sharing

fileKey  (per file)                       # generated when the file is added
  ├── encrypted with collectionKey
  └── used as the symmetric key for the file's encrypted blob

File ciphertext = XChaCha20-Poly1305 streaming, chunked (libsodium
                   crypto_secretstream_*)

Filename ciphertext = deterministic encryption per collection (so the same
                       name in the same collection produces the same
                       ciphertext, enabling lookup) OR randomized (better
                       unlinkability, costs an index). v0: deterministic.

Blob naming on disk = BLAKE3(ciphertext)  # content-addressed
```

This buys you:

- Per-file unique keys (compromise of one doesn't compromise others)
- Per-collection sharing (give a collectionKey to a peer, they get the whole
  folder; revoke by re-keying)
- Recovery via paper mnemonic
- Multi-device unlock without trusting a server (pairing transfers the
  encryptedMasterKey + the wrapping key over a paired channel)

---

## On-disk layout

```
~/Library/Application Support/AltDrive/
├── config.json                # device config (NodeId, etc.)
├── vaults/
│   └── <vault-id>/
│       ├── vault.json         # vault metadata (encrypted; format version, cipher mode)
│       ├── manifest.bin       # encrypted vault manifest (file tree, metadata)
│       ├── blobs/             # content-addressed encrypted blobs
│       │   ├── 0a/
│       │   │   └── 0a3f7c...  # BLAKE3-named ciphertext
│       │   └── ...
│       └── index.sqlite       # local metadata index (encrypted-at-rest)
├── keys/
│   ├── master.enc             # encryptedMasterKey
│   ├── recovery.enc           # encryptedRecoveryKey
│   └── private.enc            # encryptedPrivateKey (X25519)
└── peers.db                   # taint table (peer_NodeId, vault_id, mode)
```

Mount point: `/Volumes/AltDrive/<vault-name>/`

---

## Tech stack

| Layer | Choice | Rationale |
|---|---|---|
| Core | **Rust** | iroh is Rust-native; crypto crates are mature; cross-compile to mobile later |
| Crypto | **libsodium** (via [sodiumoxide](https://crates.io/crates/sodiumoxide) or [crypto_box](https://crates.io/crates/crypto_box)) | Same primitives Ente uses; well-vetted; XChaCha20-Poly1305 + X25519 + Argon2id |
| Hashing | **BLAKE3** | iroh-blobs uses it natively for content addressing |
| Transport | **iroh + iroh-blobs** | QUIC, NAT traversal, content-addressed sync — all built-in |
| Sync state | **iroh-docs** (candidate) or **version vectors** (fallback) | iroh-docs gives CRDT for free; if its semantics don't fit, Syncthing-shape version vectors are well-understood |
| Local index | **SQLite (rusqlite)** | encrypted-at-rest via SQLCipher or per-field encryption |
| Mount | **macFUSE** via [fuser](https://crates.io/crates/fuser) | simplest path on macOS for v0; FileProvider extension is the v0.5 polish |
| App shell | **Swift** wrapping the Rust core via [UniFFI](https://github.com/mozilla/uniffi-rs) | native menubar app; Rust core stays portable |

Deferred (v0.5+):

- iOS Files-app provider (FileProvider framework on iOS)
- Linux FUSE daemon
- Windows Explorer integration (WinFsp)
- Web access (probably never — that's where Proton Drive's model wins)
- Tauri 2 cross-platform desktop (alternative if Swift-wrapping gets gnarly)

---

## Comparison: Alt.Drive vs Proton Drive (server-of-record)

This is the comparison the design hinges on. Both are E2EE; the trust model
differs.

| Dimension | Proton Drive (server-of-record) | Alt.Drive (P2P vault) |
|---|---|---|
| **Where canonical state lives** | Proton's servers in Switzerland | Your devices (collectively, via CRDT/version-vector consensus) |
| **Who runs the always-on infrastructure** | Proton AG (B-corp, Swiss law, EU-adjacent) | You (NUC at home, $5 VPS, or one always-on laptop) |
| **What the server sees** | Encrypted blobs, encrypted metadata, encrypted master key, public keys | *There is no server*. Your iroh relay (if used) sees encrypted QUIC traffic and NodeIds — no content, no metadata |
| **What the server doesn't see** | Plaintext files, filenames (if encrypted), passwords, unwrapped keys | Same, plus: no server to compel, no server to subpoena, no server to outage |
| **Key recovery** | Server-stored recoveryKey (encrypted with masterKey). Forgot password? Use the printed recoveryKey on file with the service. Trusted-contact recovery available. | Paper-mnemonic only in v0. No server fallback. Lose the paper and your master key together = data lost. |
| **Multi-device unlock** | Server delivers encryptedMasterKey to new device; user enters password to derive KEK; masterKey decrypted locally. | New device pairs with existing device via QR code (Signal-style); encryptedMasterKey transferred over the paired channel. |
| **Sharing** | Wrap collectionKey to recipient's public key (lookup via Proton's user directory). Send sharing URL. | Wrap collectionKey to recipient's public key. **Key discovery is harder** — no central directory; v0 punts (only share with paired devices, no cross-user sharing). |
| **Web access** | Yes — server delivers JS that does crypto in-browser. Trust: the JS you got from the server this session. | No (v0). Web access is structurally hard for true P2P (the browser would need to be a peer). Likely never. |
| **Mobile bandwidth** | Server-mediated — phone fetches what it needs from Proton; bandwidth comes from Proton. | Peer-mediated — phone fetches from your other devices or your NUC. Bandwidth comes from your other devices or your home/cloud peer. |
| **Reliability for "laptop closed, phone wants the file"** | Solved by server — server is always on. | Solved by you running a third always-on peer (NUC, VPS, etc.). If you don't, the file isn't reachable. |
| **Operator failure modes** | Proton goes down → you can't access (until back up). Proton legally compelled → ciphertext available but useless without your password. Proton hacked → ciphertext leaked, math still protects content. Proton company death → ??? (likely data recoverable via clients, but recovery flow uncertain). | No operator. You go down (lose all devices + paper) → data lost. Otherwise unaffected. |
| **Governance / ownership** | Proton AG controls roadmap, pricing, ToS. You're a customer. | You control everything. No one else has a say. |
| **Cost** | Subscription (free tier; ~$5/mo for 200GB; ~$10/mo for 500GB). | Your hardware + electricity + (optional) $5/mo VPS for the third node. |
| **UX polish** | High — decade of product development, native apps on every platform, web access, search, sharing, file recovery, version history. | Low (v0) — macOS-only, no search, no sharing-with-non-paired-users, no web. |
| **Trust model** | "Trust the math, trust that Proton ships honest code (now verifiable — open source), trust Swiss law." | "Trust the math, trust that you handle your paper mnemonic, trust that you ran a third always-on node." |

**The strategic point**: Proton Drive is *better* for most consumers on most
axes today. They've earned that with a decade of work. Alt.Drive's only
genuine advantages are:

1. **No operator to trust** — there is no third party who could be compelled,
   compromised, or could change terms.
2. **No operator failure mode** — Proton can disappear; your data can't, as
   long as one of your devices + your paper is intact.
3. **The substrate runs your other software** — your personal AI's memory
   lives in the vault. Proton Drive can't serve that role because their
   model assumes file-as-blob, not state-of-an-application.
4. **No subscription** — your costs are hardware you already own.

For most consumers, those advantages don't justify the UX hit. For the kind
of person Chase is (and his first 100 users would be), they do.

**The thing to study deeply from Proton Drive's open source**: their crypto
primitives, their multi-recipient sharing, their per-platform native UX
patterns, their session/auth flow (even though we won't have a server).
Their decade of UX polish on native apps is the actual moat — and now it's
open source.

---

## Reference projects (what we take, what we leave)

### [Ente](https://github.com/ente-io/ente) — **the closest design reference**

What we take:
- The full key hierarchy (masterKey → collectionKey → fileKey)
- libsodium primitives (`crypto_secretstream_*` for chunked file encryption,
  `crypto_box_seal` for sharing key-wrap, Argon2id for KEK derivation)
- The recovery-key-as-paper-mnemonic model
- The verification-ID-as-BIP39-mnemonic UX for trust establishment

What we leave: their server-of-record model. Ente is photo-storage with a
server; we're files-substrate with no server.

### [Cryptomator](https://docs.cryptomator.org/security/architecture/) — **the vault format reference**

What we take:
- The encrypt-on-the-fly mount pattern (Cryptomator → WinFsp/macFUSE/FUSE)
- The vault config file pattern (`vault.cryptomator` as JWT signed by master)
- File chunk + auth tag design
- The deterministic-filename-encryption-per-vault approach (or learn from
  their evolution toward better unlinkability)

What we leave: Cryptomator's "encryption layer on top of Dropbox" model.
They're not a sync system; we are.

### [Syncthing](https://docs.syncthing.net/specs/bep-v1.html) — **the P2P-with-explicit-peers reference**

What we take:
- The device-ID-based trust model (their device IDs ≈ our iroh NodeIds)
- The three sharing modes (Trusted / SendOnly / ReceiveOnly) for the taint table
- Block-Exchange Protocol's version-vector approach for conflict resolution
- 128KB-16MB block sizes scaled by file size

What we leave: Syncthing doesn't encrypt at rest by default. We add that.
Also Syncthing's index exchange is plaintext-folder-shape; ours is
ciphertext-blob-shape.

### [iroh](https://docs.iroh.computer/) and [iroh-blobs](https://docs.rs/iroh-blobs/) — **the transport**

What we take:
- The entire transport layer (QUIC, NAT traversal, NodeIds, relays)
- iroh-blobs for content-addressed BLAKE3-streamed sync
- iroh-docs as a candidate for the vault manifest (CRDT-shaped)

What we leave: not much. iroh is the right substrate.

### [Proton Drive](https://github.com/ProtonDriveApps) — **the polish reference**

What we take:
- Native-app UX patterns per platform (just opened up 2025)
- Multi-recipient sharing implementation details (in their SDK)
- Browser-side crypto patterns (for if we ever do web access)

What we leave: the server-of-record model. We're rebuilding the *trust
model* even while reusing their UX primitives.

### [Tahoe-LAFS](https://tahoe-lafs.org/) — **the foundational research**

What we take: the conceptual rigor (capability strings, immutable vs mutable
files, the provider-independent encryption model). Read their docs to
understand the design space.

What we leave: 20-year-old UX. Their architectural ideas are sound; their
product never crossed the chasm.

---

## v0 phase plan

### Phase 0 — research week (1 week)

- Read Proton Drive SDK source code (now open) — understand their crypto
  and multi-recipient sharing in code form
- Read Ente architecture doc + key code paths in their iOS/Android repos
- Read Cryptomator vault format spec in detail
- Read iroh + iroh-blobs guides; build a hello-world that syncs a single
  blob between two iroh nodes on the same machine
- Outcome: detailed design doc with vault format spec, decided cipher modes,
  decided sync protocol (iroh-docs vs custom version vectors)

### Phase 1 — Rust core (~4 weeks)

- Crate `altdrive-core`: vault format, key hierarchy, libsodium plumbing
- Crate `altdrive-store`: local SQLite + content-addressed blob store
- Crate `altdrive-sync`: iroh integration, taint table, sync engine
- Tests: round-trip encryption, multi-device sync between two CLI nodes
- Outcome: CLI that creates a vault, adds files, syncs between two CLI
  processes on different machines

### Phase 2 — macOS mount (~3 weeks)

- macFUSE mount that transparently reads/writes through `altdrive-core`
- Swift menubar app using UniFFI bindings for status/control
- Pairing flow (QR code with NodeId + masterKey-handoff over paired channel)
- Outcome: `/Volumes/AltDrive/<vault>/` works in Finder; daemon syncs with
  paired peers

### Phase 3 — real-world use (~2 weeks)

- Set up the trio: Mac laptop + iPhone (placeholder — iOS Files provider
  is v0.5) + NUC at home as always-on peer
- Migrate one personal use case onto Alt.Drive (e.g., Obsidian vault, or
  the actual Vivian transcript library)
- Daily-drive for two weeks
- Outcome: a vault Chase actually uses, sync works under real network
  conditions

### Phase 4 — write up + decide what's next

- What broke? What surprised? What scope was wrong?
- Decide: pursue iOS next? Or sharing UX? Or the cooperative substrate?
- Outcome: a v0.5 plan grounded in real usage data

**Total v0**: ~10 weeks of focused work. Adjust for reality.

---

## Open questions / decisions to make in Phase 0

1. **iroh-docs vs custom version vectors for the manifest?** iroh-docs gives
   CRDT semantics for free; custom gives full control. Default: try iroh-docs
   first, fall back if it doesn't fit.

2. **Per-vault deterministic filenames or per-vault randomized?**
   Deterministic = simpler lookup; randomized = better unlinkability. Default:
   deterministic for v0, mark for review.

3. **Filename encryption: Cryptomator-style (one filename per ciphertext) or
   merge-into-manifest (no filenames on disk, only blob hashes)?** Default:
   merge-into-manifest for simplicity — the manifest is the canonical name
   table; blobs are content-addressed.

4. **iCloud-style or Dropbox-style mount semantics?** iCloud lazy-fetches;
   Dropbox eager-syncs by default. Default: eager-sync for v0 (assumes you
   have disk space); selective sync is a v0.5 feature.

5. **Recovery: paper mnemonic only, or also Shamir social split?** Default:
   paper-only for v0 simplicity. Shamir is real engineering and real UX
   design.

6. **Pairing UX: QR code, short-code, or both?** Default: both — QR for
   in-person, short-code for remote pairing over a side channel.

7. **What's the conflict resolution policy when two devices edit the same
   file?** Syncthing keeps both with timestamp suffix. Default: same.

8. **Encrypted index, or unencrypted index?** Default: encrypted-at-rest
   (SQLCipher) so that loss of the device store doesn't leak metadata even
   if the masterKey isn't compromised.

---

## What this isn't doing yet (and might never)

- **The cooperative legal structure.** Out of scope for v0. Becomes a real
  question if/when there are real users to govern.
- **Anchor-peer-as-service.** Out of scope for v0. Becomes a real question
  if/when there are users without their own always-on node.
- **Federation (ActivityPub / ATProto bridges).** Out of scope for v0.
  Federation runs *on top of* the substrate, not inside it.
- **The "referenceable but not public" privacy category.** Real idea from
  transcript 262; doesn't apply to v0 because v0 has no cross-user sharing.
- **Public discovery / DHT.** Out of scope for v0. Hardcoded NodeId list.
- **A search index.** Rely on macOS Spotlight indexing the decrypted mount
  for v0.
- **Web access.** Out of scope, probably forever.
- **Mobile clients.** macOS first; iOS is v0.5 at earliest.

---

## Linkage back to the Vivian / Mycelium project

Alt.Drive is the substrate Vivian wants. Once Alt.Drive ships, Vivian's
memory, models, configs, conversation history all live in a vault — encrypted,
yours, synced across your devices, with the cooperative substrate question
decoupled from the "your AI's memory needs to live somewhere" question.

From transcript 262's framing: Alt.Drive is the **Vault** layer of Project
Sovereign Commons, stripped of the federation/cooperative/anchor-peer
material so it can ship independently. The rest of 262's design becomes
optional layers on top of a working substrate.

---

## Repository structure (proposed)

```
alt-drive/
├── README.md           # this file
├── DESIGN.md           # detailed vault format / sync protocol (Phase 0 output)
├── crates/
│   ├── altdrive-core/  # crypto, key hierarchy, vault format
│   ├── altdrive-store/ # local SQLite + blob store
│   ├── altdrive-sync/  # iroh integration, taint table
│   └── altdrive-cli/   # CLI for testing
├── macos/
│   ├── AltDrive.app/   # Swift menubar app
│   └── AltDriveFS/     # macFUSE mount helper
└── docs/
    ├── threat-model.md
    ├── vault-format.md
    └── sync-protocol.md
```

---

## Provenance

This document is the v0 spec extracted from a working session 2026-05-28.
Architectural context lives in the parent project's transcript library:

- `../vivian-main/transcripts/raw/262-project-sovereign-commons-cooperative-altis-vault-protocol-bluesky-vetting-gemini.md` — the cooperative blueprint conversation where the vault model crystallized
- `../vivian-main/transcripts/raw/255-spritely-lemmer-webber-ocap-petnames-research-gemini.md` — the capability-security landscape research
- `../vivian-main/transcripts/raw/257-bluesky-atproto-protocols-not-platforms-kleppmann-lemmer-webber-critique-gemini.md` — the shared-heap-vs-message-passing critique that shaped the P2P choice
- `../vivian-main/transcripts/topics/cross-cutting-themes-254-263.md` §I — consolidated platform-vs-product spine

The Claude-era derivation of the **upper layers** Alt.Drive is the substrate for
(messaging, social, economic, cooperative), and the transport/iOS design session:

- `../vivian-main/transcripts/raw/269-p2p-chat-architecture-iroh-willow-coop-claude.md` — the full upper-layer stack (chat → social → economic → cooperative); independently converged on Alt.Drive's substrate primitives
- `../vivian-main/transcripts/raw/270-p2p-founder-motivations-adoption-maintenance-quote-claude.md` — founder-motivation / adoption research; the chasm hypothesis and "the co-op is the maintenance plan"
- `../vivian-main/transcripts/raw/271-altdrive-vs-269-veilid-unified-substrate-ios-feasibility-claude.md` — the design session: Alt.Drive ↔ 269/270, Veilid evaluation, the unified-substrate move, the iOS feasibility verdict, and the transport-port/breakpoint approach
- `../vivian-main/transcripts/topics/sovereign-p2p-social-coop-stack.md` — the de-duplicated synthesis of the above

### Planning docs in this repo

- [`docs/roadmap.md`](docs/roadmap.md) — Now / Next / After + the open-decisions register
- [`docs/transport-layers.md`](docs/transport-layers.md) — the iroh-vs-Veilid layer map, the breakpoint, and the narrow-port guardrail
