//! [`GraphStore`] — owns the LadybugDB handle and schema lifecycle.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use esgraph_core::{NormalisedEvent, StoreConfig};
use lbug::{Connection, Database, SystemConfig};
use tracing::{debug, info};

use crate::cypher::parse_count_value;
use crate::error::StoreError;
use crate::ingest::{load_next_ingest_id, IngestStats, ingest_batch};
use crate::schema::INIT_SCHEMA;

/// Options for opening a [`GraphStore`] (reserved for future use).
#[derive(Debug, Clone, Default)]
pub struct GraphStoreOptions {}

/// Embedded LadybugDB graph for esgraph.
///
/// Single-threaded writer in `esgraphd run`; readers may open separate connections.
pub struct GraphStore {
    db: Database,
    path: PathBuf,
    next_ingest_id: AtomicU64,
}

impl GraphStore {
    /// Open (or create) a database file and initialise schema tables.
    pub fn open(path: impl AsRef<Path>, _options: GraphStoreOptions) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        info!(path = %path.display(), "opening LadybugDB database");
        let db = Database::new(&path, SystemConfig::default())?;
        let mut store = Self {
            db,
            path,
            next_ingest_id: AtomicU64::new(1),
        };
        store.init_schema()?;
        store.next_ingest_id = AtomicU64::new(load_next_ingest_id(&store.db)?);
        Ok(store)
    }

    /// Open using paths from [`StoreConfig`].
    pub fn from_config(config: &StoreConfig, options: GraphStoreOptions) -> Result<Self, StoreError> {
        Self::open(&config.path, options)
    }

    /// Database file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create node/relationship tables (idempotent).
    pub fn init_schema(&mut self) -> Result<(), StoreError> {
        let conn = Connection::new(&self.db)?;
        for ddl in INIT_SCHEMA {
            match conn.query(ddl) {
                Ok(_) => {}
                Err(e) => {
                    debug!(ddl, error = %e, "schema init skipped");
                }
            }
        }
        info!("graph schema initialised");
        Ok(())
    }

    /// Ingest a batch of normalised events.
    pub fn ingest(&self, events: &[NormalisedEvent]) -> Result<IngestStats, StoreError> {
        ingest_batch(&self.db, &self.next_ingest_id, events)
    }

    /// Count nodes with the given label (`Process`, `File`, …).
    pub fn count_label(&self, label: &str) -> Result<u64, StoreError> {
        let cypher = format!("MATCH (n:{label}) RETURN count(n)");
        let conn = Connection::new(&self.db)?;
        let mut result = conn.query(&cypher)?;
        let count = result
            .next()
            .and_then(|row| row.first().map(parse_count_value))
            .flatten()
            .unwrap_or(0);
        Ok(count)
    }

    /// Count relationships of the given type.
    pub fn count_relationship(&self, rel_type: &str) -> Result<u64, StoreError> {
        let cypher = format!("MATCH ()-[r:{rel_type}]->() RETURN count(r)");
        let conn = Connection::new(&self.db)?;
        let mut result = conn.query(&cypher)?;
        let count = result
            .next()
            .and_then(|row| row.first().map(parse_count_value))
            .flatten()
            .unwrap_or(0);
        Ok(count)
    }

    pub(crate) fn db(&self) -> &Database {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::sample_write_event;
    use crate::schema::EXAMPLE_HUNT_CYPHER;
    use tempfile::tempdir;

    #[test]
    fn ingest_and_hunt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.lbug");
        let store = GraphStore::open(&path, GraphStoreOptions::default()).unwrap();
        let event = sample_write_event();
        let stats = store.ingest(&[event]).unwrap();
        assert_eq!(stats.events, 1);
        assert_eq!(stats.edges_inserted, 1);

        assert_eq!(store.count_label("Process").unwrap(), 1);
        assert_eq!(store.count_label("File").unwrap(), 1);
        assert_eq!(store.count_relationship("WROTE").unwrap(), 1);

        let result = store.query_tabular(EXAMPLE_HUNT_CYPHER).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], "/bin/zsh");
        assert_eq!(result.rows[0][1], "/tmp/research.txt");
    }

    #[test]
    fn file_backed_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.lbug");

        {
            let store = GraphStore::open(&path, GraphStoreOptions::default()).unwrap();
            store.ingest(&[sample_write_event()]).unwrap();
        }

        let store = GraphStore::open(&path, GraphStoreOptions::default()).unwrap();
        assert_eq!(store.count_relationship("WROTE").unwrap(), 1);
    }
}
