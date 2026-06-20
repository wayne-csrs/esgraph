//! ESF collector errors.

use thiserror::Error;

/// Errors from live Endpoint Security collection.
#[derive(Debug, Error)]
pub enum EsfError {
    /// This build target does not include the macOS ESF implementation.
    #[error("live ESF collection requires macOS")]
    UnsupportedPlatform,

    /// Failed to create or operate the ES client.
    #[cfg(target_os = "macos")]
    #[error("endpoint security client error: {0}")]
    Client(String),

    /// Event normalisation failed for a single message (logged and skipped).
    #[cfg(target_os = "macos")]
    #[error("normalisation error: {0}")]
    Normalise(String),

    /// Configuration could not be turned into ES event type constants.
    #[error("subscription error: {0}")]
    Subscription(String),
}
