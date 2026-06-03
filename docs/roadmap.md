# Alt.Drive — Roadmap (Now / Next / After)

**Status**: living document. Companion to `README.md` (strategic spec),
`DESIGN.md` (operational spec), `phase-0-spikes.md` (validation), and
`transport-layers.md` (the iroh-vs-Veilid layer map + breakpoint).

Provenance: transcripts `../../vivian-main/transcripts/raw/269-…`, `270-…`,
`271-…`. This roadmap folds the upper-layer vision (transcript 269) and the
design-session decisions (271) onto Alt.Drive's existing phase plan.

```
   NOW ──(layer map + port)──► NEXT ──(working vault)──► AFTER
 de-risk by design            the v0 macOS substrate    layers + open decisions
```

The earlier framing gated everything on a physical-iPhone "Spike 0." With no iOS
device in the early days, that gate is **deferred and parked behind the transport
port** — the layer map (`transport-layers.md`) becomes the "now" discipline, and
the iOS runtime question resolves when hardware is available.

---

## NOW — de-risk by design (low commitment, parallelizable)

- **Build behind the narrow transport port.** Define `BlobTransport` /
  `ManifestSync` traits; implement on iroh; keep everything above the port
  transport-agnostic so the iroh-vs-Veilid decision stays reversible.
- **Maintain the layer map** (`transport-layers.md`) as design decisions land —
  this is what we "keep an eye on through the design."
- **Run the transport-independent Phase-0 spikes** (validate existing DESIGN
  choices): iroh-docs manifest sync + 10K load test; iroh-blobs 5GB transfer;
  macFUSE hello-world; dumbpipe-shape pairing. See `phase-0-spikes.md`.
- **iOS-iroh-blob spike = DEFERRED** (Spike 7) — no device yet; parked behind the
  port; Simulator/cloud-farm give partial (build + foreground) signal only.

## NEXT — build the v0 macOS substrate (~10 weeks)

Alt.Drive's existing phase plan, with messaging-aware annotations baked in. Note:
**v0 dogfoods fully on Mac + NUC — no iPhone required.**

- **Phase 1 — Rust core (TDD):** `altdrive-core` (vault format, key hierarchy,
  libsodium), `altdrive-store` (SQLite + content-addressed blobs), `altdrive-sync`
  (iroh *behind the port*, taint table). CLI syncs between two nodes.
- **Phase 2 — macOS:** macFUSE mount + Swift menubar (UniFFI) + QR/short-code
  pairing.
- **Phase 3 — real use:** the trio (Mac + NUC always-on peer + iPhone placeholder);
  migrate a real workload (Obsidian vault or the Vivian transcript library);
  daily-drive two weeks.
- **Phase 4 — write-up + decide what's next.**
- **Carry-through annotations:** manifest designed Willow-shaped (so a later
  migration is feasible); the iroh-docs LWW rationale (DESIGN §6) is scoped to
  *static* artifacts only — interactive artifacts need merge (Automerge), which
  is an After-layer addition.
- **iOS spike runs here as a checkpoint** when a device becomes available — not a
  blocker.

## AFTER — upper layers + the tenuous register

### Layers that follow a working vault (transcript 269 vision on the substrate)

- **Messaging as vault artifacts** — one sync engine; a group conversation = a
  shared vault. Segment the conversation log (do **not** do one-blob-per-message
  — it blows the ~10K manifest cap).
- **Automerge for interactive artifacts only** (edits, reactions, read receipts,
  kudos) + an Automerge-over-iroh spike + the rule: declare the consistency model
  per artifact type so LWW never clobbers a CRDT doc.
- **E2E-encrypt Automerge ops above transport** for the content-blind "smart
  mule" HA peer (iroh TLS protects the hop, but the mule terminates it).
- **Shipped iOS client** — FileProvider extension + APNs push-relay (beyond the
  spike). The always-on peer is what makes this feasible.
- **The far layers** — PDS bridge, Lightning, reputation/countersigning, consumer
  ad exchange, the cooperative. Don't build until the substrate has real users.

### Open decisions / what's tenuous (call-outs)

| Open item | Status / what resolves it |
|---|---|
| iroh confirmed vs Veilid swap | Deferred iOS spike + the breakpoint in `transport-layers.md`. Fail → Veilid via the port (= reimplement blobs; maturity/velocity/audit unknown). |
| iroh on-device iOS runtime (battery/NAT/handoff) | **UNVERIFIED** until the deferred spike runs on real hardware. Don't call iOS "proven" before then. |
| Manifest at scale | iroh-docs ~10K cap; messaging pushes it. Open: how far iroh-docs stretches; when/whether to migrate to Willow (not shippable yet) or a custom version-vector store. |
| iroh-docs vs version-vector fallback | DESIGN §6 open question; the unified model weakens the "no merge needed" rationale. Resolved by Spike 1 + Spike 5. |
| APNs/FCM dependency | Reintroduces Apple/Google; dents "no operator" purity. Decide who runs the push-relay (HA node vs co-op service). |
| Veilid as a real fallback | Research gap: fresh commit-velocity check + DEVELOPMENT.md build matrix + DHT blob-throughput + audit status before it's credible. |
| Encrypted mule mode | No reference implementation — original crypto engineering. Tenuous. |
| Web access | README says "probably never"; transcript 269 says co-op bridge. Deferred. |
| PQ / quantum | Punted (Curve25519) until libsodium ships PQ. Future. |
| The whole economic/co-op layer | Most tenuous — depends on adoption; transcript 270's evidence is that only Signal has crossed the chasm. |

### The single load-bearing dependency

Almost everything in After is gated by the transport decision, which is itself
deferred behind the port and resolved by the iOS spike + the breakpoint triggers.
If iroh's iOS runtime works, the path is clear; if not, the port and Veilid become
live and the timeline shifts.
