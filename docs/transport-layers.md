# Alt.Drive — Transport Layer Map & Breakpoint

**Status**: living document. Update as design decisions land.

This is the discipline that lets us defer the iroh-vs-Veilid transport decision
instead of betting it up front. We design behind one narrow port, track which
layers the transport actually touches, and know in advance where switching gets
expensive.

Provenance: transcripts `../../vivian-main/transcripts/raw/269-…`, `270-…`, `271-…`
(the P2P-substrate design sessions). The layer thinking is deliberately the
IPFS/libp2p approach *without* the IPFS mistake — see the guardrail below.

---

## The principle

Layered thinking is correct (it's what IPFS got right). IPFS/libp2p got
*complicated* by making everything swappable behind generic interfaces. The
discipline here is the opposite: **one narrow port at the blob + manifest +
connection altitude, nothing more.** The crypto, vault, mount, and app layers
are ours and stable — they are not abstracted, because optionality-for-its-own-
sake is the trap.

Test: if adding the port makes a simple read-a-file path harder to follow, the
port is at the wrong altitude.

---

## Layer map

### Transport-touched layers (the swappable zone — keep behind the port)

| Layer | iroh provides | Veilid equivalent? | Swap cost |
|---|---|---|---|
| Device identity | NodeId = Ed25519 pubkey | Yes — 256-bit Ed25519/x25519 | Trivial (just keys; crypto set already matches) |
| Secure connection | Authenticated QUIC | Yes — authenticated UDP/TCP/WS(+WSS) | Low (the port's main job) |
| NAT traversal | Hole-punch → relay (DERP-derived) | Yes — hole-punch + reverse connections + in/out relays + network classification | Low (part of transport) |
| **Content-addressed blobs** | **iroh-blobs** — BLAKE3 verified streaming, partial-range, multi-source | **No** — Veilid DHT is small mutable records only | **HIGH — the breakpoint layer** |
| Mutable manifest sync | iroh-docs (LWW) | Partial — DHT records, or build version-vectors over routing | Medium (DESIGN §6.4 already specs a VV fallback) |
| Discovery | DNS/PKARR, mDNS, DHT (v0 = hardcoded NodeId list) | DHT | Low (v0 defers it either way) |
| Private routing / metadata resistance | Relay model (relay sees NodeIds, not content) | **Stronger** — source-address-free, onion-style private routing | The *inverse* pull toward Veilid (see below) |

### Our layers (transport-agnostic — the port does not touch these)

Crypto + key hierarchy (libsodium; **both transports use the same primitives**,
so even this is portable), vault format, encryption-at-rest, mount
(macFUSE/FileProvider), the Automerge interactive layer, app/UI. Pairing is
mostly ours but its ticket format is iroh-flavored, so it touches the port
lightly.

---

## The breakpoint

Two layers define it, in opposite directions:

1. **iroh-blobs is the commit point.** As long as the port exposes only
   `blob get/put/has` + `manifest replicate`, switching away from iroh is "write
   a Veilid blob adapter" — expensive but contained. The real danger is
   **leakage**: the moment vault/manifest/sync code *above* the port assumes
   iroh-blobs specifics, switching cost balloons. Discipline: **nothing above
   the port may know it's iroh.**

2. **Private routing is the inverse pull.** It is the one thing Veilid does that
   iroh structurally can't. The vault doesn't need it; the future messaging
   layer (transcript 269) might.

### Triggers to actually change transports

Stay on iroh until one of these fires:

- **iroh's iOS runtime proves unworkable** (the deferred iOS spike fails once a
  device is available) → swap the transport adapter; accept reimplementing
  blobs, or
- **the messaging layer needs source-address-free metadata resistance** iroh's
  relay model can't provide → bring Veilid in *for that layer*, or as a hybrid.

Until then, do not pay the abstraction tax beyond the one narrow port.

---

## Why this is the "now" discipline (no iOS device yet)

We can't run the physical-iPhone spike in the early days. So the layer map +
narrow port *replace* the spike as the near-term de-risking mechanism: build the
macOS substrate behind the port, keep this map current, and the eventual iOS
result costs an adapter, not a rewrite. Simulator / cloud device farms can
confirm *build + foreground* but not background/battery/cellular-NAT, so they do
not retire the runtime risk — the real spike waits for hardware.

See `roadmap.md` for how this maps onto Now / Next / After.
