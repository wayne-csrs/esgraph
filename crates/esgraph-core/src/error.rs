//! Error types for configuration loading and core validation.

use thiserror::Error;

/// Errors that can occur while loading or validating [`crate::Config`].
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config file could not be read from disk.
    #[error("failed to read config file {path}: {source}")]
    Read {
        /// Path that was attempted.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// The file contents are not valid TOML.
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        /// Path that was parsed.
        path: String,
        /// Underlying TOML parse error.
        source: toml::de::Error,
    },

    /// A config value failed semantic validation (e.g. unknown event name).
    #[error("invalid config: {0}")]
    Validation(String),
}

/// Top-level error enum for `esgraph-core` operations.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Configuration-related failure.
    #[error(transparent)]
    Config(#[from] ConfigError),
}
