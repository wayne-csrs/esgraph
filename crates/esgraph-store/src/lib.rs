//! # esgraph-store
//!
//! LadybugDB persistence for normalised ESF events.
//!
//! ## Design
//!
//! Property-graph storage with Cypher:
//!
//! 1. **Node labels** (`Process`, `File`, `Socket`, `IngestEvent`) — upserted on each event
//! 2. **Typed relationships** (`EXECUTED`, `WROTE`, …) — append-only edge log
//! 3. **`IngestEvent` nodes** — audit trail per ingested `NormalisedEvent`
//!
//! ## Threading
//!
//! LadybugDB allows one writer at a time; the live pipeline uses a dedicated writer thread.
//! CLI `query` / `status` open separate read connections.
//!
//! ## References
//!
//! - LadybugDB: <https://ladybugdb.com/>

#![warn(missing_docs)]

mod cypher;
mod error;
mod ingest;
mod query;
mod schema;
mod store;

pub use error::StoreError;
pub use ingest::{IngestStats, ingest_batch, sample_write_event};
pub use query::QueryResult;
pub use schema::{EXAMPLE_EXEC_HUNT_CYPHER, EXAMPLE_HUNT_CYPHER};
pub use store::{GraphStore, GraphStoreOptions};
