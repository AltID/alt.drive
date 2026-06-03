# Phase 0 Spike Plan

Six spikes (~1 week of focused work) to validate the design decisions in
`DESIGN.md` before writing production code. Each spike has a tight time-box
and explicit acceptance criteria — the goal is *learning*, not building.

If a spike misses its time-box twice, stop and rethink the design. Phase 0
is the only chance to course-correct cheaply.

There is also a seventh, **deferred** spike (Spike 7 — iOS-iroh-blob runtime)
that cannot run in the early days for lack of a physical iOS device. It is
parked behind the transport port (`transport-layers.md`) rather than gating
Phase 0. See its entry below and `roadmap.md` for why the layer map stands in
for it in the meantime.

---

## Spike 1 — iroh-docs trivial manifest sync

**Time-box**: 2 days
**Owner**: TBD

### Goal

Validate that iroh-docs is fit for purpose as the manifest sync layer. We're
betting the architecture on iroh-docs giving us a replicated key-value
store with last-writer-wins per key. If it doesn't, we fall back to
version-vector sync (DESIGN.md §6.4).

### Question to answer

*Can we sync a 10,000-entry replicated map between two iroh nodes with
acceptable performance and predictable conflict semantics?*

Sub-questions:
- What's the wire format for an iroh-docs replica?
- What happens when two nodes write to the same key concurrently?
- What's the memory footprint of a 10K-entry replica?
- Does the API support the deterministic-key model from DESIGN.md §6.1?

### Approach

1. Spin up two iroh nodes in the same process (different ports). Each gets
   a NodeId. Both join a shared iroh-docs author with read/write
   permissions.
2. Insert 1, 10, 100, 1000, 10000 key-value pairs from node A. Time the
   inserts.
3. Wait for node B to sync. Verify it has all the entries. Time the sync.
4. From both nodes simultaneously: update the same key with different
   values. Resolve. Confirm last-writer-wins behavior matches expectation.
5. Restart node B with a fresh data dir. Re-sync from scratch. Time it.

### Acceptance criteria

- 10K-entry insert completes in < 10 seconds locally on a 2020-era laptop
- Full sync of 10K entries between two same-machine processes < 30 seconds
- Concurrent writes to the same key are deterministically resolved (and
  the resolution rule is documented)
- The deterministic-nonce key encryption from DESIGN.md §6.1 works — same
  input always produces same iroh-docs key (so updates are updates, not
  inserts)

### Deliverables

- `crates/altdrive-spike-irohdocs/` (separate crate; throwaway code)
- A 1-page memo at `docs/spike-results/01-irohdocs.md` with:
  - Numbers (timings, memory)
  - The conflict-resolution rule iroh-docs actually implements
  - Recommendation: commit to iroh-docs, or fall back to version vectors

### Risks

- iroh-docs may be too immature for our scale (10K entries)
- The conflict semantics may not match "last-writer-wins with `(ts, node_id)`
  tiebreak" — could be something weirder
- The replica file format may be unstable across iroh-docs versions

### If this fails

Drop to the version-vector fallback (DESIGN.md §6.4). Adds ~1 week of
Phase 1 work to implement the custom ALPN protocol and the version-vector
state machine. Not catastrophic; iroh-docs was the convenience, not the
load-bearing decision.

---

## Spike 2 — iroh-blobs trivial blob sync

**Time-box**: 1 day
**Owner**: TBD

### Goal

Validate that iroh-blobs handles our content-addressed encrypted blobs at
realistic file sizes. Confirm partial-range support and multi-source
download behavior.

### Question to answer

*Can we transfer a 5 GB encrypted blob between two iroh nodes with
acceptable throughput and resume-after-disconnect behavior?*

### Approach

1. Generate a 5 GB random file. Encrypt it into 1 MiB chunks per DESIGN.md
   §5.6 (use a temporary chunk-encryption function — proper streaming
   construction is Phase 1).
2. Hash the ciphertext with BLAKE3, store as a content-addressed blob in
   iroh-blobs on node A.
3. From node B, request the blob by hash. Time the transfer.
4. Disconnect node B mid-transfer (kill TCP/QUIC). Reconnect. Verify the
   transfer resumes from where it left off.
5. Add a third node (node C) that also has the blob. From node B, request
   the blob with two providers. Verify it downloads chunks from both.
6. Verify the BLAKE3 hash on receipt matches.

### Acceptance criteria

- 5 GB transfer completes between two same-machine processes in < 60
  seconds (limited by disk + crypto, not iroh)
- Transfer resumes after a forced disconnect with no data loss
- Multi-source download visibly fetches from two providers in parallel
- The receiving node verifies BLAKE3 hash matches before marking the blob
  complete

### Deliverables

- `crates/altdrive-spike-irohblobs/` (throwaway)
- `docs/spike-results/02-irohblobs.md`:
  - Throughput numbers
  - Confirmation that resume works
  - Multi-source behavior verified

### Risks

- The "5 GB single blob" model may be wrong — files of that size might
  need to be split at the application layer rather than treated as one
  iroh-blob. (Note: iroh-blobs uses chunked transfer under the hood; this
  spike confirms whether one 5GB blob is OK or we should split.)

### If this fails

Reconsider the "blob = entire encrypted file" assumption. May need to
split large files into multiple smaller blobs at the application layer,
which changes the manifest schema (one file → list of blob hashes
instead of one).

---

## Spike 3 — macFUSE hello-world mount

**Time-box**: 1 day
**Owner**: TBD

### Goal

Validate that we can mount a userland filesystem on macOS via macFUSE
that transparently de/encrypts on read/write, with acceptable performance
and no fundamental UX rough edges.

### Question to answer

*Can macFUSE deliver a Finder-mounted folder where reads go through our
decryption pipeline and writes go through our encryption pipeline, with
acceptable performance for daily use?*

### Approach

1. Install macFUSE (manually for the spike; productionization is later).
2. Implement a minimal FUSE filesystem in Rust using the `fuser` crate.
3. Backing store: in-memory `HashMap<PathBuf, Vec<u8>>` of "plaintext"
   files.
4. On read: serve from the HashMap. On write: store in the HashMap.
5. Test from Finder, Terminal, and a couple of common apps (TextEdit,
   VSCode-style editors that watch for file changes).
6. Then: replace the HashMap with a content-addressed encrypted blob
   store. On read: decrypt blob → return plaintext. On write: encrypt
   plaintext → store blob.
7. Test the same scenarios with the encryption layer.

### Acceptance criteria

- Finder displays the mount point as a normal folder
- Read/write throughput is at least 100 MB/s (sequential)
- TextEdit can save a file, close it, reopen it — no glitches
- VSCode-style file-watching works
- No kernel panics, no zombie mount points after `umount`

### Deliverables

- `crates/altdrive-spike-fuse/` (throwaway)
- `docs/spike-results/03-fuse.md`:
  - Throughput numbers
  - List of any UX rough edges (e.g., specific apps that misbehave)
  - Recommendation: macFUSE for v0, FileProvider for v0.5 — confirmed or
    changed

### Risks

- macFUSE on Apple Silicon requires kext approval (System Integrity
  Protection). May require user steps that hurt v0 UX.
- macFUSE's writeback semantics may not match what we need — specifically,
  apps that do "open, write, close" may not flush as we expect.
- macFUSE is a kernel extension and Apple has been deprecating those.
  Future-proofing may push us to FileProvider sooner than expected.

### If this fails

The first fallback is FileProvider (Apple's first-class API for userland
file systems). FileProvider is harder to develop against (requires app
sandboxing, specific entitlements) but is the only future-safe path on
macOS. Adds ~2 weeks to v0 if we have to start there.

---

## Spike 4 — dumbpipe-shape ticket pairing

**Time-box**: 1 day
**Owner**: TBD

### Goal

Validate the pairing protocol from DESIGN.md §8 end-to-end. Two iroh nodes
exchange a ticket out-of-band, perform an ECDH-derived session, and
transfer a small payload that demonstrates the key-handoff pattern.

### Question to answer

*Can we implement a dumbpipe-shape ticket flow with a 6-digit pairing
code and visual-phrase confirmation that's usable in practice?*

### Approach

1. Implement the ticket encoder/decoder (DESIGN.md §8.1). Base32-encode
   the `{node_id, relay_url, ephemeral_pub, expires_at}` struct.
2. Existing device: generate ephemeral keypair, generate pairing code,
   open iroh listener with ALPN `"altdrive/pair/v1"`, print ticket as
   string + a QR code (use any QR crate).
3. New device: parse ticket, connect to existing device via iroh, perform
   ECDH, derive session key.
4. Both sides: derive a 4-word BIP39 phrase from session key, display it.
5. Operator confirms the phrases match (manual step in the spike).
6. Send a 1 KB test payload from existing device to new device, encrypted
   with session key. Verify decryption.

### Acceptance criteria

- Ticket round-trips correctly (encode → decode → use)
- ECDH session establishes between the two nodes
- 4-word verification phrase is identical on both sides
- Test payload decrypts cleanly
- Manual MitM test: a third node tries to connect using the same ticket
  but a different pairing code → fails

### Deliverables

- `crates/altdrive-spike-pairing/` (throwaway)
- `docs/spike-results/04-pairing.md`:
  - Confirmation that the protocol works
  - Notes on UX rough edges (QR code size, ticket string length, etc.)
  - Recommendation: any adjustments to DESIGN.md §8

### Risks

- The 6-digit pairing code may be too short (10^6 ≈ 1M space — brute force
  feasible over the 10-minute window if the attacker is on-path). May
  need to increase or rate-limit.
- Out-of-band visual confirmation may be poorly accepted by users (extra
  step on top of the pairing code). UX testing needed.

### If this fails

Fall back to a simpler model: ticket + long-form secret embedded in the
ticket (no separate pairing code, no visual phrase). Weaker against
shoulder-surfing the QR but simpler. Or escalate to a more complex
protocol with hardware-backed key attestation. Defer to discussion.

---

## Spike 5 — Decision write-up: iroh-docs vs version-vector fallback

**Time-box**: 1 day (after Spike 1)
**Owner**: TBD

### Goal

Produce the definitive decision document on whether iroh-docs is the
manifest sync layer for Alt.Drive, or whether we implement the
version-vector fallback.

### Question to answer

*Given what Spike 1 revealed, which path takes us to a working v0 with
less risk and less Phase 1 work?*

### Approach

Pure design work — no code. Based on Spike 1's measurements and observed
semantics:

1. Score iroh-docs against required properties:
   - Last-writer-wins semantics matching DESIGN.md §7
   - 10K-entry scale
   - Stable wire format
   - Working multi-peer sync
   - Acceptable memory/disk footprint
2. Score the version-vector fallback against the same:
   - Custom ALPN implementation effort (Phase 1 estimate)
   - Conflict semantics control (we own them entirely)
   - No external dependency for the sync state machine
3. List the unknowns each path carries forward into Phase 1.
4. Make the call.

### Acceptance criteria

- A clear go/no-go on iroh-docs
- If no-go: a specific Phase 1 plan for the version-vector implementation
- If go: the specific iroh-docs version we're targeting and any
  workarounds for known limitations

### Deliverables

- `docs/decisions/01-manifest-sync.md` — ADR-style decision record
- Update to `DESIGN.md` §6 reflecting the final decision

### Risks

- The decision is between two paths neither of which is obviously right.
  May need a tiebreaker conversation with someone experienced in iroh
  (Discord, n0 team).

---

## Spike 6 — DESIGN.md update from spike outcomes

**Time-box**: ongoing through Phase 0, ~0.5 day to close out
**Owner**: TBD

### Goal

Ensure `DESIGN.md` reflects the reality discovered in Spikes 1-5. Phase 1
implementation should start from a design doc that matches the spikes,
not the original pre-spike speculation.

### Approach

Per spike, write changes to DESIGN.md as a numbered changelog at the
bottom of the doc. Once Phase 0 closes, consolidate into the body.

### Acceptance criteria

- Every DESIGN.md decision either survives the spikes unchanged or has
  an explicit "amended after Spike N" note
- The "Open questions" section (§14) is empty or reduced to questions
  that genuinely require Phase 1 work to answer
- The Phase 1 estimate at the end of DESIGN.md is updated based on
  observed spike velocity

### Deliverables

- Updated `DESIGN.md`
- A short retrospective at `docs/spike-results/retro.md`: what we
  learned, what surprised us, what we'd change about the Phase 0
  approach next time

---

## Spike 7 — iOS-iroh-blob runtime feasibility (DEFERRED)

**Time-box**: ~1 week — **when a physical iOS device is available**
**Owner**: TBD
**Status**: DEFERRED. No iOS device in the early days. Parked behind the
transport port; the layer map (`transport-layers.md`) is the interim
de-risking mechanism. Runs as a checkpoint during Phase 3 (or whenever
hardware appears), not as a Phase 0 gate.

### Goal

Establish whether the iroh-backed Rust core survives iOS's lifecycle well
enough to sync blobs from an always-on peer. This is the single genuinely
unproven variable in the transport choice — and the trigger for the
`transport-layers.md` breakpoint (iroh → Veilid) if it fails.

### Question to answer

*Can a Rust + iroh core, wrapped via UniFFI, fetch a content-addressed blob
on a real iPhone with acceptable background / battery / cellular-NAT
behavior, woken by an APNs push from the always-on peer?*

### Approach

1. Rust lib (iroh + iroh-blobs) → UniFFI → bare Swift iOS app that connects
   to a NUC/desktop peer and fetches one content-addressed blob. (No full
   FileProvider extension needed for the spike — `BGTaskScheduler` + a
   manually-fired APNs push from the NUC is enough to measure runtime.)
2. Run on a **physical iPhone, not the Simulator** — UDP/QUIC, carrier NAT,
   background, and battery need a real device on real networks.

### Acceptance criteria

- Builds for `aarch64-apple-ios` and runs on-device.
- Foreground blob fetch from the peer over home wifi **and** over cellular
  (proves hole-punch/relay from a carrier-NAT'd iPhone).
- **wifi ↔ cellular mid-transfer handoff** recovers (the Berty-killer).
- APNs-woken background sync completes inside the background window.
- Battery cost of a connected-idle period + a sync burst is "acceptable,"
  not uninstall-grade.

### Deliverables

- `crates/altdrive-spike-ios/` + a minimal Swift app (throwaway).
- `docs/spike-results/07-ios.md`: the five measurements + a go/no-go.

### Risks

- iOS forbids arbitrary background daemons; the feasible model is
  FileProvider + always-on-peer + APNs-wake, **not** a full always-on peer.
- No documented iroh-on-iOS-in-production reference exists (Veilid's
  VeilidChat shipping on iOS is *suggestive* that a Rust P2P stack can, but
  Veilid ≠ iroh).
- Simulator / cloud device farms confirm build + foreground only; they do
  not retire the background/battery/NAT risk.

### If this fails

Trigger the `transport-layers.md` breakpoint: either (a) drop to a
FileProvider-only opportunistic model (no live connection; "syncs when
foregrounded/refreshed" — still a fine vault, weaker messaging), or
(b) swap the transport adapter to Veilid (which has the iOS existence
proof) and accept reimplementing blob transfer.

---

## Phase 0 schedule (illustrative)

| Day | Activity |
|---|---|
| 1 | Spike 1 day 1 (iroh-docs) |
| 2 | Spike 1 day 2 (iroh-docs continued) |
| 3 | Spike 2 (iroh-blobs) |
| 4 | Spike 3 (macFUSE hello-world) |
| 5 | Spike 4 (pairing) |
| 6 | Spike 5 (decision write-up) |
| 7 | Spike 6 (DESIGN.md update) + Phase 0 retro |

Total: 7 working days. Buffer + actual personal calendar realities likely
push to ~2 calendar weeks.

---

## Phase 0 exit checklist

Before declaring Phase 0 done and moving to Phase 1:

- [ ] All 6 spike memos exist under `docs/spike-results/`
- [ ] `docs/decisions/01-manifest-sync.md` written and committed
- [ ] `DESIGN.md` updated to reflect spike outcomes
- [ ] `docs/threat-model.md` reviewed against any new attack surface the
      spikes revealed (e.g., new ALPN protocols introduce DoS surface)
- [ ] A Phase 1 plan exists at `docs/phase-1-plan.md` with week-by-week
      breakdown
- [ ] The `crates/altdrive-core/` scaffolding compiles + tests pass
- [ ] At least one design assumption has been falsified by a spike (if
      every spike confirmed every assumption, the design didn't go deep
      enough — be suspicious)

The last item is the most important and the most easily overlooked. Phase
0's value is partly in *what we learn we were wrong about*. If we sail
through every spike, we should worry that we weren't really testing the
design, just rubber-stamping it.
