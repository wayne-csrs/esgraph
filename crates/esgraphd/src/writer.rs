//! Batched LadybugDB writer thread for live ESF ingest.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use esgraph_core::{NormalisedEvent, StoreConfig};
use esgraph_store::{GraphStore, GraphStoreOptions, IngestStats};
use tracing::{debug, info};

/// Spawn a background thread that batches [`NormalisedEvent`] values into LadybugDB.
pub fn spawn_writer(
    store_config: StoreConfig,
    options: GraphStoreOptions,
    rx: Receiver<NormalisedEvent>,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<Result<IngestStats>> {
    thread::spawn(move || writer_loop(store_config, options, rx, shutdown))
}

fn writer_loop(
    store_config: StoreConfig,
    options: GraphStoreOptions,
    rx: Receiver<NormalisedEvent>,
    shutdown: Arc<AtomicBool>,
) -> Result<IngestStats> {
    let store = GraphStore::from_config(&store_config, options).map_err(|e| anyhow::anyhow!("{e}"))?;
    info!(path = %store_config.path, "writer thread opened LadybugDB database");

    let batch_size = store_config.batch_size.max(1);
    let flush_interval = Duration::from_millis(store_config.flush_interval_ms.max(1));

    let mut batch = Vec::with_capacity(batch_size);
    let mut total = IngestStats::default();
    let mut last_flush = Instant::now();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            while let Ok(ev) = rx.try_recv() {
                batch.push(ev);
            }
            if batch.is_empty() {
                break;
            }
            total = flush_batch(&store, &mut batch, total)?;
            break;
        }

        let timeout = flush_interval.saturating_sub(last_flush.elapsed());
        match rx.recv_timeout(timeout) {
            Ok(ev) => batch.push(ev),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !batch.is_empty() && last_flush.elapsed() >= flush_interval {
                    total = flush_batch(&store, &mut batch, total)?;
                    last_flush = Instant::now();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if batch.len() >= batch_size {
            total = flush_batch(&store, &mut batch, total)?;
            last_flush = Instant::now();
        }
    }

    if !batch.is_empty() {
        total = flush_batch(&store, &mut batch, total)?;
    }

    info!(?total, "writer thread finished");
    Ok(total)
}

fn flush_batch(
    store: &GraphStore,
    batch: &mut Vec<NormalisedEvent>,
    mut total: IngestStats,
) -> Result<IngestStats> {
    let stats = store
        .ingest(batch)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    debug!(?stats, "flushed batch to LadybugDB");
    total.events += stats.events;
    total.node_upserts += stats.node_upserts;
    total.edges_inserted += stats.edges_inserted;
    batch.clear();
    Ok(total)
}
