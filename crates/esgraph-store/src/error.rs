//! Storage-layer error types.

use thiserror::Error;

/// Errors from LadybugDB operations and ingest validation.
#[derive(Debug, Error)]
pub enum StoreError {
    /// LadybugDB driver error.
    #[error("ladybug error: {0}")]
    Ladybug(#[from] lbug::Error),

    /// I/O error creating database parent directories.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Semantic validation of a [`esgraph_core::NormalisedEvent`].
    #[error("invalid event for ingest: {0}")]
    InvalidEvent(String),
}
