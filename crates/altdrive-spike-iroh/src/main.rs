//! Throwaway Phase 0 Spike 2 — iroh-blobs hello-world between two nodes.
//!
//! Pinned to UDP 2112 on `0.0.0.0` (IPv4) and `[::]` (IPv6) so AWS SG
//! rules can be tight. Validates that iroh-blobs round-trips a content-
//! addressed blob across two iroh endpoints over QUIC.
//!
//! Usage:
//!   $ cargo run -p altdrive-spike-iroh -- provide
//!     prints a `BlobTicket`, serves until Ctrl-C.
//!   $ cargo run -p altdrive-spike-iroh -- fetch <ticket>
//!     downloads the blob, re-hashes locally, prints byte count + preview.
//!
//! Not production code — see `docs/phase-0-spikes.md` (Spike 2) and
//! `crates/altdrive-spike-iroh/Cargo.toml` (`publish = false`).

use anyhow::{anyhow, Context, Result};
use iroh::{endpoint::presets, protocol::Router, Endpoint};
use iroh_blobs::{store::mem::MemStore, ticket::BlobTicket, BlobsProtocol, Hash};

/// Fixed blob payload — deterministic so both sides can verify byte-for-byte.
const BLOB_PAYLOAD: &[u8] = b"alt.drive iroh spike 2 - hello-world payload (Phase 0)";

/// UDP port both nodes bind to. Matches the SG rule in `docs/phase-0-spikes.md`.
const SPIKE_PORT: u16 = 2112;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    match arg_refs.as_slice() {
        ["provide"] => provide().await,
        ["fetch", ticket] => fetch(ticket).await,
        _ => {
            eprintln!("usage:");
            eprintln!("    altdrive-spike-iroh provide");
            eprintln!("    altdrive-spike-iroh fetch <ticket>");
            std::process::exit(2);
        }
    }
}

async fn build_endpoint() -> Result<Endpoint> {
    Endpoint::builder(presets::N0)
        .alpns(vec![iroh_blobs::ALPN.to_vec()])
        .clear_ip_transports()
        .bind_addr(format!("0.0.0.0:{SPIKE_PORT}"))
        .context("bind UDP/v4 2112")?
        .bind_addr(format!("[::]:{SPIKE_PORT}"))
        .context("bind UDP/v6 2112")?
        .bind()
        .await
        .map_err(|e| anyhow!("endpoint bind failed: {e}"))
}

async fn provide() -> Result<()> {
    let endpoint = build_endpoint().await?;
    let store = MemStore::new();
    let blobs = BlobsProtocol::new(&store, None);

    let tag = store.blobs().add_bytes(BLOB_PAYLOAD).await?;
    let ticket = BlobTicket::new(endpoint.id().into(), tag.hash, tag.format);

    let router = Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs)
        .spawn();

    println!("Node ID:    {}", router.endpoint().id());
    println!("Bound:      {:?}", router.endpoint().bound_sockets());
    println!("Blob hash:  {}", tag.hash);
    println!("Blob bytes: {}", BLOB_PAYLOAD.len());
    println!();
    println!("From the peer:");
    println!("    cargo run -p altdrive-spike-iroh -- fetch {ticket}");
    println!();
    println!("Serving until Ctrl-C...");

    tokio::signal::ctrl_c().await.context("ctrl-c handler")?;
    println!("Shutting down.");
    router
        .shutdown()
        .await
        .map_err(|e| anyhow!("router shutdown: {e}"))?;
    Ok(())
}

async fn fetch(ticket_str: &str) -> Result<()> {
    let ticket: BlobTicket = ticket_str.parse().context("parse ticket")?;
    let endpoint = build_endpoint().await?;
    let store = MemStore::new();
    let downloader = store.downloader(&endpoint);

    println!("Fetching {} from {}", ticket.hash(), ticket.addr().id);
    downloader
        .download(ticket.hash(), Some(ticket.addr().id))
        .await
        .map_err(|e| anyhow!("download failed: {e}"))?;

    let bytes = store
        .blobs()
        .get_bytes(ticket.hash())
        .await
        .map_err(|e| anyhow!("read back: {e}"))?;

    let recomputed = Hash::new(&bytes);
    if recomputed != ticket.hash() {
        return Err(anyhow!(
            "BLAKE3 mismatch — expected {}, got {}",
            ticket.hash(),
            recomputed
        ));
    }

    println!("Got {} bytes (BLAKE3 verified).", bytes.len());
    let preview_len = bytes.len().min(64);
    println!("Preview:    {}", String::from_utf8_lossy(&bytes[..preview_len]));

    endpoint.close().await;
    Ok(())
}
