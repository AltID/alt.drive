# Alt.Drive — DESIGN (Phase 0)

Detailed design doc for the v0 spec laid out in `README.md`. Where the
README is strategic ("here's what we're building and why"), this doc is
operational ("here are the bytes, the algorithms, the protocol messages,
and the decisions we've made"). Phase 1 implementation should be able to
start from this doc.

**Status**: draft, Phase 0 in progress. Decisions are marked **DECIDED**
or **OPEN**.

---

## 1. Scope

This document specifies:

- The threat model
- Cryptographic primitives (algorithms + parameters)
- The key hierarchy (precise sizes + wrapping relationships)
- On-disk vault format (file by file)
- Manifest format and the iroh-docs vs custom-version-vector decision
- Sync protocol (which iroh primitives, what messages, when)
- Conflict resolution rules
- Taint table schema
- Pairing protocol (dumbpipe-shape ticket flow)
- Recovery flow (paper mnemonic mechanics)

Out of scope for v0 (covered in README, listed here for clarity):
sharing across non-paired users, federation, web access, mobile, search.

---

## 2. Threat model

**We protect against:**

1. Loss or theft of a single device — the on-disk vault is unusable
   without the password.
2. Network observers — including ISPs, relay operators, anyone on a coffee
   shop wifi. They see encrypted QUIC with iroh NodeIds; no content, no
   filenames.
3. Compromise of a relay server — iroh's relays see ciphertext-over-QUIC
   only; they cannot read traffic content.
4. A peer in the taint table going rogue — they can only access vaults
   we've granted them and only via the keys we wrapped to their pubkey.
5. Future improvements in cryptanalysis to the extent practical (libsodium
   primitives, AEAD, modern KDFs).

**We do NOT protect against:**

1. Loss of the password AND the recovery mnemonic AND all devices.
   No recovery possible. This is by design — adding a server-side recovery
   path defeats the trust model.
2. Compromise of the user's password (we have only one factor unless the
   user opts into hardware-backed Touch ID / Secure Enclave wrapping in
   v0.5).
3. Malware on the user's machine that captures the unwrapped masterKey in
   memory.
4. Sophisticated traffic analysis at the iroh layer (timing, volume) that
   could correlate which peer is sharing with which.
5. Quantum adversaries (we use Curve25519; not PQ-resistant; v0 punts on
   this until libsodium ships PQ primitives).
6. Hostile updates to the Alt.Drive binary itself. Code-signing the
   release is v0.5; reproducible builds are v1.

**Not threats** (worth noting):

1. There is no operator to compel or compromise. There is no central
   service whose subpoena could yield ciphertext.
2. There is no central directory of users, so there is no list to leak.

---

## 3. Cryptographic primitives — DECIDED

All primitives via [libsodium](https://libsodium.gitbook.io/doc/) (Rust
binding: [sodiumoxide](https://crates.io/crates/sodiumoxide) or directly
[libsodium-sys](https://crates.io/crates/libsodium-sys)). libsodium is the
right call because Ente, Standard Notes, Element/Matrix, Signal, and Wire
all use it; the primitives are audited and stable.

| Purpose | Algorithm | libsodium API | Key/Output Size |
|---|---|---|---|
| File content (streaming AEAD) | XChaCha20-Poly1305 | `crypto_secretstream_xchacha20poly1305_*` | 32-byte key, per-chunk 16-byte tag, 24-byte header |
| Small-field AEAD (manifest values, filenames) | XSalsa20-Poly1305 | `crypto_secretbox_easy` | 32-byte key, 24-byte nonce, 16-byte tag |
| Key wrapping (sharing → recipient pubkey) | X25519 + XSalsa20-Poly1305 (sealed box) | `crypto_box_seal` | 32-byte X25519 pubkey, 48-byte overhead |
| Password → key derivation | Argon2id (SENSITIVE params) | `crypto_pwhash` (ALG=2, ops=8, mem=512MiB) | 32-byte output, 16-byte salt |
| Mnemonic → key derivation | BIP39 mnemonic decoding (24 words → 256-bit entropy + 8-bit checksum) → use entropy directly as 32-byte key | (custom; uses `tiny-bip39` or `bip39` crate) | 32-byte key |
| Content hash (blob addressing) | BLAKE3 | (iroh-blobs uses natively) | 32-byte hash |
| Device identity / signatures | Ed25519 | `crypto_sign_*` | 32-byte privkey, 32-byte pubkey, 64-byte signature |
| Device transport identity | iroh NodeId = Ed25519 pubkey | (handled by iroh) | 32 bytes |
| Random | OS CSPRNG | `randombytes_buf` | as needed |

**Notes:**

- XChaCha20 (24-byte nonce) over ChaCha20 (12-byte nonce): random nonces
  with 24-byte width have negligible collision probability across the
  lifetime of any plausible vault. Removes a class of nonce-reuse bugs.
- Argon2id SENSITIVE parameters (ops=8, mem=512MiB) target ~2-second
  derivation on a 2020-era laptop. Acceptable for vault unlock; not
  per-file.
- Ed25519 for device signatures on the manifest. The device's iroh NodeId
  IS its Ed25519 pubkey (iroh uses Ed25519 for NodeIds), so the signing
  key and the transport key are the same key. Simplifies the model.

---

## 4. Key hierarchy — DECIDED

Following Ente's well-documented design, adapted for files-as-substrate.

```
┌──────────────────────────────────────────────────────────────────┐
│  Password                                                         │
│      │                                                            │
│      │ Argon2id(password, salt, SENSITIVE)                        │
│      ▼                                                            │
│  KEK (key-encryption-key, 32 bytes)                               │
│      │                                                            │
│      │ secretbox.decrypt(encryptedMasterKey)                      │
│      ▼                                                            │
│  masterKey (32 bytes)  ←──────── encryptedRecoveryWrap            │
│      │              ↑              decrypted by                   │
│      │              │              recoveryKey from mnemonic      │
│      │                                                            │
│      ├─ secretbox.decrypt(encryptedDevicePrivateKey) → devicePrivateKey (X25519 + Ed25519, 32+32 bytes)
│      │                                                            │
│      ├─ secretbox.decrypt(encryptedCollectionKey_v1) → collectionKey_v1 (32 bytes per vault)
│      │      │                                                     │
│      │      ├─ secretbox.decrypt(encryptedFileKey_f1) → fileKey_f1 (32 bytes per file)
│      │      │      │                                              │
│      │      │      └─ secretstream.decrypt(encryptedBlob_f1) → file content
│      │      │                                                     │
│      │      └─ (more files...)                                    │
│      │                                                            │
│      └─ (more vaults...)                                          │
└──────────────────────────────────────────────────────────────────┘
```

**Sharing path** (when a peer is granted access to a vault):

```
sender's collectionKey_vN
     │
     │ crypto_box_seal(collectionKey, recipient_X25519_pubkey)
     ▼
encryptedCollectionKey_for_recipient
     │
     │ (stored in the taint metadata, transferred via pairing)
     ▼
recipient's local store
     │
     │ crypto_box_seal_open with their privateKey
     ▼
collectionKey_vN (now usable to decrypt files in that vault)
```

---

## 5. On-disk vault format — DECIDED

Base path: `~/Library/Application Support/AltDrive/` on macOS, XDG-compliant
on Linux, `%APPDATA%\AltDrive\` on Windows.

```
~/Library/Application Support/AltDrive/
├── device.json                    # plaintext: NodeId, app version, device label
├── keys/
│   ├── master.enc                 # binary: encryptedMasterKey
│   ├── master.params              # binary: Argon2id salt + ops + mem params
│   ├── recovery.enc               # binary: encryptedRecoveryWrap
│   ├── device_private.enc         # binary: encryptedDevicePrivateKey
│   └── device_public.bin          # binary: device X25519 pubkey + Ed25519 pubkey
├── vaults/
│   └── <vault-id>/                # vault-id = 16 random bytes, hex-encoded
│       ├── vault.json             # encrypted vault metadata + collectionKey wrap
│       ├── manifest.iroh-doc      # iroh-docs replica file (manifest entries)
│       ├── blobs/
│       │   └── <ab>/              # sharded by first byte of BLAKE3 hash
│       │       └── <abcdef...>    # BLAKE3-named ciphertext file
│       └── index.db               # SQLite (SQLCipher-encrypted) local index
└── peers.db                       # SQLite: peers + taint table
```

### 5.1 `device.json` (plaintext)

```json
{
  "version": 1,
  "node_id": "k51qzi5uqu5...",
  "label": "chase-macbook-pro",
  "created_at": "2026-05-28T10:00:00Z"
}
```

### 5.2 `keys/master.enc` and `keys/master.params`

```
master.params (binary):
   u8  argon2id_version          (= 1)
   u8  ops_limit                 (= 8, SENSITIVE)
   u32 mem_limit_kib             (= 524288, i.e. 512 MiB)
   [16] salt                     (random, generated once at vault creation)

master.enc (binary):
   [24] nonce                    (random)
   [32+16] secretbox(masterKey, key=KEK)  // 32 plaintext + 16 auth tag
```

### 5.3 `keys/recovery.enc`

```
recovery.enc (binary):
   [24] nonce                    (random)
   [32+16] secretbox(masterKey, key=recoveryKey)
```

`recoveryKey` is the 256-bit entropy of the displayed BIP39 24-word mnemonic.

### 5.4 `vaults/<vault-id>/vault.json` (encrypted blob)

The file is binary, not JSON-on-disk despite the `.json` extension. Layout:

```
   [24] nonce
   [N+16] secretbox(serialize({
     vault_id,
     vault_label,
     created_at,
     collection_key,                  // 32 bytes
     last_modified_at,
     last_modified_by_node_id,
   }), key=masterKey)
```

### 5.5 `vaults/<vault-id>/manifest.iroh-doc`

The iroh-docs replica file. Managed by iroh; we put encrypted *values*
into it. See §6 for the manifest entry format.

### 5.6 `vaults/<vault-id>/blobs/<ab>/<full-hash>`

Content-addressed encrypted blobs. The filename IS the BLAKE3 hash of the
ciphertext (not the plaintext). Stored sharded by first byte to keep
directory sizes manageable.

Blob contents are libsodium's `crypto_secretstream` output:

```
   [24] header (from crypto_secretstream_init_push)
   [chunk1_ciphertext + 17] chunk 1 (16-byte auth tag + 1-byte tag)
   [chunk2_ciphertext + 17] chunk 2
   ...
   [final_chunk_ciphertext + 17] final chunk (with FINAL tag)
```

**Chunk size: DECIDED = 1 MiB plaintext per chunk.** Tradeoff: smaller chunks
mean more auth tags and more iroh-blobs sub-ranges. Larger chunks mean
more memory for streaming and worse partial-fetch granularity. 1 MiB is
the same default as Cryptomator and is what Ente uses for chunked uploads.

### 5.7 `peers.db` schema

```sql
CREATE TABLE peers (
    node_id BLOB PRIMARY KEY,           -- 32-byte iroh NodeId (= Ed25519 pubkey)
    label TEXT NOT NULL,                 -- human-readable name
    x25519_pubkey BLOB NOT NULL,         -- their X25519 pubkey (32 bytes) for sharing
    paired_at INTEGER NOT NULL,          -- unix timestamp
    last_seen INTEGER                    -- unix timestamp, nullable
);

CREATE TABLE vault_shares (
    node_id BLOB NOT NULL,
    vault_id BLOB NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('readwrite', 'read', 'none')),
    encrypted_collection_key BLOB NOT NULL,  -- collectionKey sealed to peer's X25519 pubkey
    granted_at INTEGER NOT NULL,
    PRIMARY KEY (node_id, vault_id),
    FOREIGN KEY (node_id) REFERENCES peers(node_id)
);

CREATE INDEX idx_vault_shares_vault ON vault_shares(vault_id);
```

### 5.8 `index.db` (per vault, SQLCipher-encrypted)

Local-only cache for fast filesystem operations. The manifest is the source
of truth (synced via iroh-docs); this is the speed layer.

```sql
CREATE TABLE files (
    path TEXT PRIMARY KEY,               -- plaintext path (local-only)
    filename_ct BLOB NOT NULL,           -- ciphertext filename (for manifest lookup)
    file_key BLOB NOT NULL,              -- unwrapped fileKey (in this index only)
    blob_hash BLOB NOT NULL,             -- BLAKE3 of ciphertext
    size INTEGER NOT NULL,               -- plaintext size
    modified_at INTEGER NOT NULL,
    version_vector BLOB                  -- serialized vector clock
);

CREATE TABLE folders (
    path TEXT PRIMARY KEY,
    folder_name_ct BLOB NOT NULL,
    version_vector BLOB
);
```

SQLCipher passphrase = first 32 bytes of `HKDF-SHA256(masterKey, "altdrive-index-db")`.

---

## 6. Manifest format and sync protocol — DECIDED (with fallback)

**Decision: iroh-docs for the manifest, iroh-blobs for file content.**

Rationale: iroh-docs provides a replicated key-value store with CRDT-flavored
conflict resolution (last-writer-wins per key, with timestamp + node-ID
tiebreak), automatic sync between peers, and is built by the same team as
iroh-blobs so the integration is clean. We don't need full CRDT semantics
(no concurrent edits to the same map entry need merging) — we need
last-writer-wins, which iroh-docs gives us.

**Fallback if iroh-docs proves unfit (e.g., maturity issues, semantics
mismatch): custom version-vector sync (Syncthing-shape).** That fallback is
specified in §6.4.

### 6.1 Manifest entries (the iroh-docs values)

The iroh-docs replica holds one key-value pair per file and per folder:

```
key   = encrypted filename (relative path within the vault), deterministic
        encryption per collection so that the same plaintext path within
        the same vault always produces the same ciphertext key
value = secretbox(serialize(ManifestEntry), key=collectionKey)
        (random nonce per value)

ManifestEntry {
    kind: "file" | "folder" | "tombstone",
    plaintext_filename: String,         // the user-visible filename
    plaintext_parent_path: String,      // the user-visible parent folder
    blob_hash: [u8; 32]?,               // BLAKE3 of ciphertext, file only
    file_key_wrapped: [u8; 48]?,        // secretbox(fileKey, key=collectionKey), file only
    plaintext_size: u64?,               // file only
    modified_at: u64,                   // unix timestamp
    modified_by_node_id: [u8; 32],      // last writer
    tombstone_for: [u8; 32]?,           // hash of the entry being deleted, tombstone only
}
```

**Deterministic key encryption rationale**: the iroh-docs key must be stable
across edits (otherwise an edit looks like a delete + add). We achieve
this with deterministic AEAD: nonce = HKDF(collectionKey, "manifest-key-nonce", plaintext_path).
This leaks "same path was encrypted twice" (which is fine — we want that)
but does not leak the path itself.

### 6.2 Sync triggers

- On vault open: poll iroh-docs for latest state, fetch missing blobs via
  iroh-blobs.
- On local file change (detected via macFUSE callbacks or filesystem events):
  encrypt blob, write to local store, upsert manifest entry into iroh-docs.
  iroh-docs replicates to peers automatically.
- On gossip notification (via iroh-gossip): peer pushed an update; poll
  immediately.
- Periodic poll: every 30 seconds when network is good, every 5 minutes
  on cellular/metered.

### 6.3 Blob sync

iroh-blobs handles content-addressed sync. When a manifest entry arrives
referencing a blob_hash we don't have:

1. Look up which peers in the taint table have access to this vault (mode != 'none').
2. Request the blob from those peers via iroh-blobs.
3. iroh-blobs streams the encrypted blob; we verify the BLAKE3 hash.
4. Write to `blobs/<ab>/<full-hash>` and decrypt on read via the mount layer.

Multi-source download (iroh-blobs supports this natively) means if your
laptop, NUC, and another paired device all have the blob, the requesting
device can fetch chunks from all three in parallel.

### 6.4 Custom version-vector fallback (if iroh-docs is unfit)

If iroh-docs doesn't work for us, the fallback is:

- Each manifest entry carries a version vector: `{node_id: u64}` map.
- Sync protocol: peer A sends "I have entries with these version vectors"
  to peer B; B replies with entries A is missing or has older versions of.
- Conflict resolution: last-writer-wins per entry, with `(modified_at, node_id)`
  tiebreak (lexicographically larger node_id wins ties).
- Implemented on top of iroh's QUIC connection with a custom ALPN protocol
  ("altdrive/manifest/v1") similar to how dumbpipe uses custom ALPNs.

This is Syncthing's BEP model adapted. Well-understood, but more work than
using iroh-docs.

**Phase 0 task**: spike both. Spend 2 days getting iroh-docs to sync a
trivial manifest between two CLI nodes; if it works smoothly, commit. If
it doesn't, do the same for the version-vector fallback.

---

## 7. Conflict resolution rules — DECIDED

iroh-docs (or the fallback) handles **manifest entry** conflicts. The
rules:

1. **Same path edited on two devices independently**: both devices write
   different manifest entries with overlapping version vectors. Last-writer-
   wins per `(modified_at, node_id)` tiebreak. **AND** the losing version
   is preserved as `<filename>.conflict-<short_node_id>-<timestamp>.<ext>`
   (Syncthing-style) so no data is silently lost.
2. **File deleted on one device, edited on another**: the edit wins (the
   delete is treated as one version, the edit is another, and they're
   compared by timestamp).
3. **Tombstones for deletions**: when a file is deleted, we write a
   tombstone manifest entry rather than removing the entry. Tombstones
   propagate. Garbage collection of tombstones after 90 days (sync-safe
   window).
4. **Move/rename**: handled as delete + add (with the same file_key and
   blob_hash retained). The local index keeps the linkage; remote peers
   see two entries (one tombstoned, one new) but the blob is reused.

**For the file content itself, there are no conflicts** — blobs are
content-addressed and immutable. The conflict, if any, is at the manifest
layer ("which blob does this path point to right now").

---

## 8. Pairing protocol — DECIDED (dumbpipe-shape)

The pairing flow uses iroh's ticket pattern (proven in dumbpipe):

### 8.1 Existing device (initiator)

```
1. Generate ephemeral X25519 keypair (ephemeral_priv, ephemeral_pub).
2. Generate 6-digit pairing code (random, displayed to user).
3. Compute pairing_secret = HKDF-SHA256(pairing_code, salt=device_node_id).
4. Open iroh listener with custom ALPN "altdrive/pair/v1".
5. Generate ticket:
     {
       node_id: existing_device.node_id,
       relay_url: existing_device.relay_url,
       ephemeral_pub: ephemeral_pub,
       expires_at: now + 600s,   // 10-minute window
     }
6. Encode ticket as base32 string. Display:
     - QR code containing ticket string
     - The 6-digit pairing code (user must type or confirm on new device)
```

### 8.2 New device (joiner)

```
1. Scan QR or paste ticket string. Decode.
2. User enters the 6-digit pairing code shown on existing device.
3. Compute pairing_secret = HKDF-SHA256(pairing_code, salt=ticket.node_id).
4. Connect to ticket.node_id over iroh with ALPN "altdrive/pair/v1".
5. Perform ECDH: shared = ECDH(new_device_ephemeral_priv, ticket.ephemeral_pub).
6. Derive session_key = HKDF-SHA256(shared || pairing_secret, "altdrive-pairing").
7. Send Hello frame with new_device.node_id + new_device.x25519_pubkey,
   authenticated with session_key.
```

### 8.3 Mutual confirmation

```
Both devices derive a 4-word verification phrase from session_key
(BIP39 4-word slice). Each displays its phrase. User must visually
confirm they match. (Defense against MitM where ephemeral keys were
swapped — pairing_code prevents this, but visual confirmation is
defense in depth.)
```

### 8.4 Key handoff

```
Once mutual confirmation succeeds:
1. Existing device sends (session_key-AEAD-encrypted):
   - The user's encryptedMasterKey + Argon2id params (so new device can
     unlock with the user's password)
   - encryptedDevicePrivateKey will NOT be transferred — each device has
     its own device identity. The new device generates its own.
   - The full peers.db and vault_shares contents
   - For each vault the user owns, the encrypted vault metadata
2. New device:
   - Decrypts encryptedMasterKey using the user's password
   - Generates its own device identity (X25519 + Ed25519 = NodeId)
   - Wraps its devicePrivateKey with the masterKey
   - Sends its NodeId + X25519 pubkey back to existing device
3. Existing device:
   - Adds new device to its peers.db
   - Re-wraps each collectionKey to the new device's X25519 pubkey
   - Sends the wrapped collectionKeys to the new device
4. Both devices update their taint tables: the new device gets readwrite
   access to all vaults the user owns. Sharing with other people is
   per-vault and explicit (later).
```

### 8.5 What this gives us

- **No central server required** for pairing (consistent with the rest of
  the design).
- **Out-of-band visual confirmation** prevents the MitM attack the
  pairing_code alone leaves room for.
- **Each device has its own identity** (separate NodeId). Compromise of
  one device doesn't extract the others' private keys.
- **Per-vault re-wrapping** preserves the property that the new device can
  only decrypt vaults it's been explicitly granted access to.

---

## 9. Recovery flow — DECIDED

### 9.1 At vault creation

```
1. Generate masterKey (32 random bytes).
2. Generate recoveryKey (32 random bytes).
3. Display BIP39 24-word mnemonic of recoveryKey to user.
4. Require user to type back 4 randomly-selected words to confirm they
   wrote it down (anti-skip safeguard).
5. Store encryptedRecoveryWrap = secretbox(masterKey, key=recoveryKey) in
   keys/recovery.enc.
```

### 9.2 Forgotten password recovery (single device, recovery mnemonic available)

```
1. User opens app, can't unlock with password.
2. App offers "I have my recovery phrase" flow.
3. User types 24-word BIP39 mnemonic.
4. Decode mnemonic → recoveryKey (32 bytes).
5. Decrypt keys/recovery.enc with recoveryKey → masterKey.
6. Prompt user to set a new password.
7. Re-derive KEK = Argon2id(new_password, new_salt, SENSITIVE).
8. Rewrite keys/master.enc with new wrapping.
9. The vaults are unchanged — only the password layer rotates.
```

### 9.3 Lost everything except mnemonic

```
User has only the BIP39 24-word mnemonic. No working device.
1. Install Alt.Drive on a new machine.
2. Recovery flow as in 9.2 — but the new device starts with no vaults.
3. Recovery without a peer to sync from = no data recovered. The
   recoveryKey unwraps the masterKey, but the vault content is on the
   other (lost) devices.
4. THIS IS THE HARD CASE. If all your devices and your data backups are
   gone, the mnemonic alone won't bring back your files. The mnemonic
   recovers the *key*, not the *data*.
```

**Mitigation**: always have at least one always-on third device (NUC, VPS)
in your trio. The mnemonic recovers your access to *that* device's stored
vault, which becomes your data backup.

### 9.4 Lost everything including mnemonic

Data is lost. By design — there is no operator-mediated recovery path. The
README's "What v0 IS NOT" section flagged this explicitly.

---

## 10. Edge cases and known limitations

1. **Race during pairing**: if existing device crashes during the
   collectionKey re-wrap step, new device ends up in a "paired but no
   vault access" state. v0 behavior: re-pair from existing device when
   it comes back. Better recovery is v0.5.

2. **Storage exhaustion on a peer**: the NUC fills up. v0 behavior: peer
   reports "out of space" via iroh-gossip; other peers stop pushing new
   blobs to it. User intervention required.

3. **Clock skew**: timestamps for conflict resolution. v0 uses local
   `now`; we don't try to do logical clocks. If two devices have wildly
   different clocks, the one with the later clock always wins. Document
   this; users typically have NTP-synced clocks.

4. **Manifest growth**: large vaults produce large iroh-docs replicas.
   Untested at scale. v0 caps: 10,000 files per vault (will be enforced
   by the app); larger is unsupported.

5. **Per-chunk auth tag overhead**: 1 MiB chunks with 17 bytes overhead =
   ~0.0016% overhead. Negligible.

6. **Filename length**: macOS allows up to 255 bytes for a path component.
   Encrypted filenames will exceed this for long names. We adopt
   Cryptomator's approach: filenames > 220 bytes get truncated and stored
   in a sidecar file. v0 deferred — assume short filenames; document the
   limit.

7. **Sync ordering and observability**: users expect to see "syncing..."
   indicators. v0 needs a basic status dump (count of in-flight blob
   transfers, last-sync-with-each-peer timestamps). UI gets it via UniFFI.

---

## 11. Architecture mapping (which crate owns what)

| Concern | Crate | Notes |
|---|---|---|
| Crypto primitives wrappers | `altdrive-core` | libsodium calls; key derivation |
| Key hierarchy management | `altdrive-core` | unwrap chains, key caching |
| Vault format read/write | `altdrive-core` | parsing/serialization |
| Mnemonic encode/decode | `altdrive-core` | BIP39 |
| Local blob store | `altdrive-store` | filesystem layout, hash verification |
| Local SQLite index | `altdrive-store` | SQLCipher integration |
| Manifest sync | `altdrive-sync` | iroh-docs integration |
| Blob sync | `altdrive-sync` | iroh-blobs integration |
| Taint table | `altdrive-sync` | peer + vault_shares queries |
| Pairing protocol | `altdrive-sync` | iroh listener + ALPN |
| Custom ALPN protocols | `altdrive-sync` | "altdrive/pair/v1", "altdrive/manifest/v1" (if fallback) |
| FUSE mount | `altdrive-mount` | macFUSE callbacks → core |
| Conflict file naming | `altdrive-mount` | `.conflict-<id>-<ts>.<ext>` policy |
| CLI | `altdrive-cli` | testing-only; for development |
| Swift app | `macos/AltDrive.app/` | menubar + setup wizard |
| UniFFI bindings | top-level | Rust ↔ Swift bridge |

---

## 12. Phase 0 deliverables and exit criteria

### Deliverables

- [x] This document (`DESIGN.md`)
- [ ] Spike: iroh-docs trivial sync between two CLI nodes (2 days)
- [ ] Spike: iroh-blobs trivial blob sync between two CLI nodes (1 day)
- [ ] Spike: macFUSE hello-world mount (1 day)
- [ ] Spike: dumbpipe-shape ticket+ALPN pairing demo (1 day)
- [ ] Decision write-up: iroh-docs commit vs version-vector fallback (1 day)
- [ ] Updated `DESIGN.md` reflecting spike outcomes

### Exit criteria

Phase 0 is done when:

1. We have a written design for every numbered section above.
2. The spikes have validated that iroh-docs + iroh-blobs + macFUSE all
   work as expected for our use case.
3. The conflict-resolution rules survive a manual walkthrough of the
   "edit on two devices offline, then sync" scenario.
4. The pairing protocol survives a manual walkthrough of MitM and lost-
   pairing-code scenarios.
5. We can answer "what does Phase 1 look like, week by week" with
   confidence.

---

## 13. Decisions log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-28 | libsodium primitives | Battle-tested; Ente/Signal/Wire all use; clean Rust bindings |
| 2026-05-28 | XChaCha20-Poly1305 streaming for file content | 24-byte nonce eliminates collision risk; same as Ente |
| 2026-05-28 | 1 MiB plaintext chunks | Cryptomator + Ente default; good partial-fetch granularity |
| 2026-05-28 | BIP39 24-word recovery mnemonic | Industry standard; familiar to users; 256 bits of entropy |
| 2026-05-28 | iroh-docs for manifest (with version-vector fallback) | Saves implementing custom sync; same vendor as iroh-blobs |
| 2026-05-28 | iroh-blobs for content sync | Native content-addressed BLAKE3 sync; partial-range fetch |
| 2026-05-28 | dumbpipe-shape ticket pairing | Proven pattern in n0's own ecosystem |
| 2026-05-28 | Per-device identity (separate NodeId per device) | Compromise of one device doesn't extract others' keys |
| 2026-05-28 | Deterministic key encryption for manifest keys | Required for stable iroh-docs keys; HKDF nonce derivation prevents leak beyond "same path encrypted twice" |
| 2026-05-28 | macFUSE for v0 mount | Simpler than FileProvider; well-trodden; FileProvider is v0.5 polish |
| 2026-05-28 | Rust core + Swift UniFFI for macOS app | Rust core stays cross-platform; Swift gives native macOS UX |
| 2026-05-28 | No server-side recovery | Trust-model alignment; document the failure mode loudly |
| 2026-05-28 | 6-digit pairing code + visual phrase confirmation | Defense in depth against MitM during pairing |

---

## 14. Open questions to resolve in spikes

| Q | Plan to answer |
|---|---|
| Does iroh-docs handle 10K-entry replicas without obvious problems? | Spike: load test in Phase 0 |
| Does iroh-blobs handle 1GB+ files smoothly with the 1 MiB chunking? | Spike: transfer a 5GB file between two CLI nodes |
| Does macFUSE's writeback semantics play nicely with our "encrypt on write" model? | Spike: macFUSE hello-world, then a real write |
| What's the minimum memory footprint of an idle iroh node? | Measure during spike |
| How do we handle the NodeId change if a user reinstalls (loses device identity)? | Probably: re-pair flow; document |
| Does SQLCipher add meaningful security beyond the OS-level encryption (FileVault)? | Threat-model walkthrough; defer if low value |

---

## 15. References (deep study targets)

| Project | URL | What to study |
|---|---|---|
| Ente architecture | https://ente.com/architecture/ | Key hierarchy in production E2EE |
| Cryptomator vault format | https://docs.cryptomator.org/security/architecture/ | On-disk format, filename encryption |
| Syncthing BEP v1 | https://docs.syncthing.net/specs/bep-v1.html | Version-vector sync, sharing modes (fallback ref) |
| iroh docs | https://docs.iroh.computer/ | Endpoints, discovery, NAT traversal |
| iroh-blobs | https://docs.rs/iroh-blobs/ | Content-addressed sync API |
| iroh-docs | https://docs.iroh.computer/docs/protocols/documents/ | Replicated K-V API |
| dumbpipe | https://github.com/n0-computer/dumbpipe | Ticket-based pairing, custom ALPN |
| Proton Drive SDK | https://github.com/ProtonDriveApps/sdk | Multi-recipient sharing, native UX |
| libsodium docs | https://libsodium.gitbook.io/doc/ | Algorithm parameters and security notes |
| BIP39 spec | https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki | Mnemonic encoding/decoding |
