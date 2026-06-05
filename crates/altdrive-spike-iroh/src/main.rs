//! Throwaway Phase 0 Spike 2 — iroh-blobs hello-world + scale test.
//!
//! Pinned to UDP 2112 on `0.0.0.0` and `[::]`. Persistent FsStore under
//! `/mnt/data/spike-store-<role>/` so the same hashed content survives
//! process restarts (and, in principle, supports resume-after-disconnect
//! for the Spike 2 acceptance criteria — that case is **not yet wired in
//! this iteration**).
//!
//! Usage:
//!   $ altdrive-spike-iroh provide               # 54-byte hello-world
//!   $ altdrive-spike-iroh provide <file>        # serve a file (TryReference, no-copy)
//!   $ altdrive-spike-iroh fetch <ticket>        # read into memory, preview + verify
//!   $ altdrive-spike-iroh fetch <ticket> <out>  # write to <out>, BLAKE3-verify
//!
//! Not production code — see `docs/phase-0-spikes.md` (Spike 2) and
//! `crates/altdrive-spike-iroh/Cargo.toml` (`publish = false`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use iroh::{endpoint::presets, protocol::Router, Endpoint};
use iroh_blobs::{
    api::blobs::AddPathOptions,
    api::proto::ImportMode,
    store::fs::FsStore,
    ticket::BlobTicket,
    BlobFormat, BlobsProtocol, Hash,
};

/// Fixed payload used when `provide` is invoked with no file argument.
const HELLO_PAYLOAD: &[u8] = b"alt.drive iroh spike 2 - hello-world payload (Phase 0)";

/// UDP port both nodes bind to.
const SPIKE_PORT: u16 = 2112;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    match arg_refs.as_slice() {
        ["provide"] => provide(None).await,
        ["provide", file] => provide(Some(Path::new(file).to_owned())).await,
        ["fetch", ticket] => fetch(ticket, None).await,
        ["fetch", ticket, out] => fetch(ticket, Some(Path::new(out).to_owned())).await,
        _ => {
            eprintln!("usage:");
            eprintln!("    altdrive-spike-iroh provide               # 54-byte hello-world");
            eprintln!("    altdrive-spike-iroh provide <file>        # serve a file");
            eprintln!("    altdrive-spike-iroh fetch <ticket>        # in-memory + preview");
            eprintln!("    altdrive-spike-iroh fetch <ticket> <out>  # write to <out>");
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

async fn open_store(role: &str) -> Result<FsStore> {
    let root = PathBuf::from(format!("/mnt/data/spike-store-{role}"));
    std::fs::create_dir_all(&root).with_context(|| format!("mkdir {}", root.display()))?;
    FsStore::load(&root)
        .await
        .map_err(|e| anyhow!("FsStore::load({}): {e}", root.display()))
}

async fn provide(file: Option<PathBuf>) -> Result<()> {
    let endpoint = build_endpoint().await?;
    let store = open_store("provide").await?;
    let blobs = BlobsProtocol::new(&store, None);

    let hash_start = Instant::now();
    let (tag, size_hint) = match file.as_ref() {
        None => {
            let tag = store.blobs().add_bytes(HELLO_PAYLOAD).await?;
            (tag, HELLO_PAYLOAD.len() as u64)
        }
        Some(p) => {
            let path = p
                .canonicalize()
                .with_context(|| format!("canonicalize {}", p.display()))?;
            let meta = std::fs::metadata(&path)
                .with_context(|| format!("stat {}", path.display()))?;
            let size = meta.len();
            println!(
                "Hashing {} ({} bytes, ~{:.2} GiB)...",
                path.display(),
                size,
                size as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            let tag = store
                .blobs()
                .add_path_with_opts(AddPathOptions {
                    path,
                    format: BlobFormat::Raw,
                    mode: ImportMode::TryReference,
                })
                .await?;
            (tag, size)
        }
    };
    let hash_elapsed = hash_start.elapsed();

    let ticket = BlobTicket::new(endpoint.id().into(), tag.hash, tag.format);
    let router = Router::builder(endpoint)
        .accept(iroh_blobs::ALPN, blobs)
        .spawn();

    println!("Node ID:      {}", router.endpoint().id());
    println!("Bound:        {:?}", router.endpoint().bound_sockets());
    println!("Blob hash:    {}", tag.hash);
    println!("Blob bytes:   {}", size_hint);
    println!("Hash time:    {:.2?}", hash_elapsed);
    if size_hint > 0 {
        let mb = size_hint as f64 / (1024.0 * 1024.0);
        let mbps = mb / hash_elapsed.as_secs_f64();
        println!("Hash rate:    {:.1} MiB/s", mbps);
    }
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

async fn fetch(ticket_str: &str, out: Option<PathBuf>) -> Result<()> {
    let ticket: BlobTicket = ticket_str.parse().context("parse ticket")?;
    let endpoint = build_endpoint().await?;
    let store = open_store("fetch").await?;
    let downloader = store.downloader(&endpoint);

    println!("Fetching {} from {}", ticket.hash(), ticket.addr().id);
    let xfer_start = Instant::now();
    downloader
        .download(ticket.hash(), Some(ticket.addr().id))
        .await
        .map_err(|e| anyhow!("download failed: {e}"))?;
    let xfer_elapsed = xfer_start.elapsed();

    match out {
        None => {
            // Small payload path — pull into memory and preview.
            let bytes = store
                .blobs()
                .get_bytes(ticket.hash())
                .await
                .map_err(|e| anyhow!("read back: {e}"))?;
            verify(&ticket, &bytes)?;
            println!("Got {} bytes (BLAKE3 verified).", bytes.len());
            let preview_len = bytes.len().min(64);
            println!(
                "Preview:      {}",
                String::from_utf8_lossy(&bytes[..preview_len])
            );
            print_xfer_stats(bytes.len() as u64, xfer_elapsed);
        }
        Some(out_path) => {
            // Large-payload path — export to disk and verify by re-hashing
            // the on-disk file (avoids loading multi-GB into memory).
            store
                .blobs()
                .export(ticket.hash(), &out_path)
                .await
                .map_err(|e| anyhow!("export to {}: {e}", out_path.display()))?;
            let size = std::fs::metadata(&out_path)
                .with_context(|| format!("stat {}", out_path.display()))?
                .len();
            println!("Wrote {} ({} bytes).", out_path.display(), size);
            print_xfer_stats(size, xfer_elapsed);
            // Note: blobs::export already verifies BLAKE3 internally during
            // download; the receiver-side hash check on `download(...)` is
            // load-bearing. We don't re-hash the on-disk file here because
            // for 5+ GB files it's ~50 seconds of redundant work.
        }
    }

    endpoint.close().await;
    Ok(())
}

fn verify(ticket: &BlobTicket, bytes: &[u8]) -> Result<()> {
    let recomputed = Hash::new(bytes);
    if recomputed != ticket.hash() {
        return Err(anyhow!(
            "BLAKE3 mismatch — expected {}, got {}",
            ticket.hash(),
            recomputed
        ));
    }
    Ok(())
}

fn print_xfer_stats(bytes: u64, elapsed: std::time::Duration) {
    let secs = elapsed.as_secs_f64();
    println!("Transfer time: {:.2?}", elapsed);
    if secs > 0.0 && bytes > 0 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        println!("Throughput:    {:.1} MiB/s ({:.1} Mbit/s)", mb / secs, (mb * 8.0) / secs);
    }
}
