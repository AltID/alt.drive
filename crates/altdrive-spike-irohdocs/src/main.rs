//! Throwaway Phase 0 Spike 1 — iroh-docs manifest sync between two nodes.
//!
//! Spins up two iroh endpoints in one process (different ports). Node A
//! creates an iroh-docs document and shares the ticket to node B. Node B
//! imports the ticket (which auto-starts sync per `DocsApi::import`).
//! Then A inserts N entries, and we wait for B to converge to N entries,
//! timing both halves.
//!
//! Not production code — see `docs/phase-0-spikes.md` §Spike 1 for the
//! design questions this is meant to answer.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bytes::Bytes;
use iroh::{endpoint::presets, protocol::Router, Endpoint};
use iroh_blobs::{store::mem::MemStore, BlobsProtocol, ALPN as BLOBS_ALPN};
use iroh_docs::{
    api::protocol::{AddrInfoOptions, ShareMode},
    protocol::Docs,
    store::Query,
    ALPN as DOCS_ALPN,
};
use iroh_gossip::{net::Gossip, ALPN as GOSSIP_ALPN};
use n0_future::StreamExt;

/// How many entries to insert on node A.
const N: usize = 10;

/// Timeout for node B's sync convergence.
const SYNC_TIMEOUT: Duration = Duration::from_secs(60);

/// Two same-process nodes — A binds this port, B binds +1.
/// 3112/3113 to avoid collision with altdrive-spike-iroh on 2112.
const PORT_A: u16 = 3112;
const PORT_B: u16 = 3113;

struct Node {
    endpoint: Endpoint,
    docs: Docs,
    /// Hold the router so the protocols stay alive for the duration of the test.
    _router: Router,
}

async fn build_node(port: u16) -> Result<Node> {
    let endpoint = Endpoint::builder(presets::N0)
        .clear_ip_transports()
        .bind_addr(format!("0.0.0.0:{port}"))
        .with_context(|| format!("bind UDP/v4 {port}"))?
        .bind()
        .await
        .map_err(|e| anyhow::anyhow!("endpoint bind on {port}: {e}"))?;

    let blobs = MemStore::default();
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let docs = Docs::memory()
        .spawn(endpoint.clone(), (*blobs).clone(), gossip.clone())
        .await?;

    let router = Router::builder(endpoint.clone())
        .accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
        .accept(GOSSIP_ALPN, gossip)
        .accept(DOCS_ALPN, docs.clone())
        .spawn();

    Ok(Node {
        endpoint,
        docs,
        _router: router,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Building two same-process iroh-docs nodes...");
    let node_a = build_node(PORT_A).await?;
    let node_b = build_node(PORT_B).await?;
    println!("Node A: {} on port {}", node_a.endpoint.id(), PORT_A);
    println!("Node B: {} on port {}", node_b.endpoint.id(), PORT_B);
    println!();

    let docs_a = node_a.docs.api();
    let docs_b = node_b.docs.api();

    let author_a = docs_a.author_create().await?;
    let doc_a = docs_a.create().await?;
    println!("Doc created on A: {}", doc_a.id());

    let ticket = doc_a
        .share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
        .await?;
    println!("Share ticket generated; importing on B...");

    let doc_b = docs_b.import(ticket).await?;
    println!("Doc imported on B: {} (sync started by import)", doc_b.id());
    assert_eq!(doc_a.id(), doc_b.id(), "namespace ID must match");
    println!();

    // Insert N entries on A.
    println!("Inserting {N} entries on A...");
    let insert_start = Instant::now();
    for i in 0..N {
        let key = format!("key-{i:06}");
        let value = format!("value-from-A-{i:06}");
        doc_a
            .set_bytes(
                author_a,
                Bytes::from(key.into_bytes()),
                Bytes::from(value.into_bytes()),
            )
            .await?;
    }
    let insert_elapsed = insert_start.elapsed();
    let insert_rate = N as f64 / insert_elapsed.as_secs_f64();
    println!(
        "Inserts on A: {:.2?} ({:.0} entries/s)",
        insert_elapsed, insert_rate
    );

    // Sanity: A should see all N immediately.
    let count_a = count_entries(&doc_a).await?;
    println!("Local count on A: {count_a}");
    anyhow::ensure!(
        count_a == N,
        "A's local count {count_a} != inserted N={N} — something is wrong with the writer"
    );

    // Wait for B to converge.
    println!("Waiting for B to sync (timeout {:?})...", SYNC_TIMEOUT);
    let sync_start = Instant::now();
    loop {
        let count = count_entries(&doc_b).await?;
        if count == N {
            break;
        }
        if sync_start.elapsed() > SYNC_TIMEOUT {
            anyhow::bail!("Sync timed out: B has {count} of {N} entries");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let sync_elapsed = sync_start.elapsed();
    let sync_rate = N as f64 / sync_elapsed.as_secs_f64();
    println!(
        "Sync done: {:.2?} ({:.0} entries/s)",
        sync_elapsed, sync_rate
    );
    println!();

    // Spot check the first entry exists on B with a non-empty content hash.
    // Reading the actual bytes goes via the blobs store (entry.content_hash()
    // → blobs.get_bytes); skipping that here keeps the spike narrow.
    let entry_opt = doc_b.get_one(Query::key_exact(b"key-000000")).await?;
    match entry_opt {
        Some(entry) => println!(
            "Spot check: B sees key={:?} content_hash={}",
            std::str::from_utf8(entry.key()).unwrap_or("<non-utf8>"),
            entry.content_hash()
        ),
        None => anyhow::bail!("B has no entry for key-000000 despite count match"),
    }

    println!();
    println!("Spike 1 hello-world PASS for N={N}.");
    Ok(())
}

async fn count_entries(doc: &iroh_docs::api::Doc) -> Result<usize> {
    // get_many returns `impl Stream` without an Unpin bound, so box-pin
    // before driving it with StreamExt::next.
    let mut stream = Box::pin(doc.get_many(Query::all()).await?);
    let mut n = 0usize;
    while let Some(item) = stream.next().await {
        let _entry = item?;
        n += 1;
    }
    Ok(n)
}
