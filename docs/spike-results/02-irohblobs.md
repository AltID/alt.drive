# Spike 2 results — iroh-blobs blob sync

**Status (2026-06-05):** PARTIAL PASS.
Hello-world round-trip (54 bytes, BLAKE3-verified) works between two EC2
hosts in the same VPC. The bigger acceptance criteria from
`docs/phase-0-spikes.md` §Spike 2 (5 GB transfer, resume-after-disconnect,
multi-source download) are **not yet tested** in this iteration.

**Recommendation:** continue with iroh-blobs as the transport layer for
content-addressed blobs. Nothing in the hello-world results suggests a
need to revisit the choice. Run the scale tests before declaring full
PASS.

---

## Setup

| | |
|---|---|
| iroh version | `1.0.0-rc.1` |
| iroh-blobs version | `0.102.0` |
| Rust toolchain | `1.96.0` stable |
| Spike crate | `crates/altdrive-spike-iroh/` (binary, `publish = false`) |
| Pinned UDP port | `2112` on both `0.0.0.0` and `[::]` |
| Endpoint preset | `presets::N0` (n0 discovery + relays enabled) |
| Store | `iroh_blobs::store::mem::MemStore` (in-memory, both sides) |

### Hosts

| Role | Hostname | Private IP | AZ |
|---|---|---|---|
| Provider | `secroute-testing-two` | `172.31.19.13` | `us-east-1b` |
| Fetcher | `secroute-testing-one` | `172.31.43.122` | `us-east-1c` |

Both AWS EC2, same VPC. Security group rule on each: inbound UDP/2112 from
the peer's SG (only). IPv6 not configured globally on either interface
(only `fe80::/10` link-local); iroh's address discovery published v4 only,
so the `[::]:2112` bind was effectively a no-op for traffic.

---

## What was tested

A fixed 54-byte payload (`alt.drive iroh spike 2 - hello-world payload
(Phase 0)`) was added to the provider's in-memory store, the resulting
`BlobTicket` (encoding `NodeAddr` + `Hash` + `BlobFormat::Raw`) was
copy-pasted to the fetcher, and the fetcher downloaded via
`MemStore::downloader().download(hash, Some(node_id))`. After download,
the fetcher recomputed BLAKE3 locally over the received bytes and
compared to `ticket.hash()`.

## Results

- **Transfer:** completed in well under a second wall-clock (no
  user-visible delay). Throughput numbers are not meaningful at 54 bytes.
- **BLAKE3 verification:** hash `01c5d388bb24f67f1c8355fc5310af5a63cc2b8b45d50ba171f8f31730fcec24`
  computed on the fetcher matched `ticket.hash()` exactly. The spike
  binary refuses to claim success unless this match holds.
- **Cross-machine determinism:** the same payload produces the same
  BLAKE3 on both nodes (already established by `altdrive-core`'s Argon2id
  KAT, but reconfirmed here for BLAKE3 specifically).
- **Pinned UDP 2112:** `endpoint.bound_sockets()` printed
  `[0.0.0.0:2112, [::]:2112]` on the provider, confirming the explicit
  bind worked and no ephemeral fallback was needed.
- **Direct VPC path:** [UNVERIFIED] without packet capture. We did not
  observe relay traffic in stderr, but did not actively verify the
  direct UDP path was used vs. a relay fallback. The latency was sub-
  second so likely direct, but a proper confirmation would run
  `endpoint.net_report()` or `tcpdump -n udp port 2112`.

## What is NOT yet tested (Spike 2 full acceptance)

Per `docs/phase-0-spikes.md` §Spike 2 acceptance criteria:

- [ ] **5 GB single-blob transfer** completes in < 60 seconds between
      two same-machine processes — not run; the spike payload is hard-
      coded to 54 bytes.
- [ ] **Resume after forced disconnect** with no data loss — not run;
      no kill-mid-transfer harness in the spike binary.
- [ ] **Multi-source download** verifiably parallelizes across two
      providers — not run; only one provider node in this test.
- [ ] **Memory footprint** of an idle iroh node — not measured.

These are the load-bearing claims that justify "iroh-blobs is the right
substrate for vault file sync." The hello-world result is encouraging
but does not retire those questions.

## Notes for future spikers

- The `BlobTicket` encoding is opaque from the application's perspective —
  base32, 113 chars for this test (one direct addr + one relay url + 32-byte
  hash + format byte), copy-pastes cleanly. Suitable as a Spike 4 pairing
  token model (the dumbpipe-shape ticket pattern in DESIGN.md §8 uses
  exactly this).
- `presets::N0` reaches out to n0.computer DNS for PKARR publishing even
  when the direct VPC path is the one actually used. Switching to
  `presets::N0DisableRelay` (or `presets::Minimal` with manual addr
  setting) is the right next move once we want to verify the spike runs
  fully air-gapped from public infrastructure.
- iroh's debug build is ~285 MB on disk (with debug symbols) for the
  spike binary. Default AWS AMI root volumes (8 GB) will not survive the
  full dependency compile — both nodes needed a separate EBS volume
  mounted at `/mnt/data` with `~/.cargo` and the workspace target
  symlinked over. See the disk layout memory for the procedure.
- `tokio` needed the `signal` feature explicitly enabled for
  `tokio::signal::ctrl_c()`. If the spike grows other tokio
  dependencies, watch for similar gated-by-feature surprises.

## Decision queue this informs

| Question | Answered by hello-world? | Still open |
|---|---|---|
| iroh transport viable at all? | ✓ Yes — basic bind+connect+blob works | — |
| iroh-blobs round-trips a blob? | ✓ Yes | — |
| BLAKE3 content-addressing matches between nodes? | ✓ Yes | — |
| Pinned UDP port works for tight SG rules? | ✓ Yes | — |
| Scales to vault-realistic file sizes (multi-GB)? | — | needs full spike |
| Resumes cleanly across network disconnects? | — | needs full spike |
| Idle-node memory cost fits on a NUC? | — | needs measurement |

---

## Replay

To reproduce the hello-world result:

```bash
# On the provider host (us-east-1b in this test):
cd ~/alt.drive && cargo run -p altdrive-spike-iroh -- provide
# Copy the printed BlobTicket.

# On the fetcher host (us-east-1c in this test):
cd ~/alt.drive && cargo run -p altdrive-spike-iroh -- fetch <PASTED_TICKET>
# Expect: "Got 54 bytes (BLAKE3 verified)." with the preview line.
```

If the fetcher hangs more than ~30s without progress, check that:

1. The provider is still running and bound to UDP 2112.
2. The Security Group on the provider allows inbound UDP/2112 from the
   fetcher's SG or private IP.
3. The NodeId in the fetcher's terminal matches the one the provider
   printed (sanity check that no stale ticket was reused).
