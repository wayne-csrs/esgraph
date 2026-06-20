//! TOML configuration loader.
//!
//! Configuration is split into logical sections matching how operators think about ESF:
//!
//! - `[events]` — which event types to subscribe to, grouped by domain
//! - `[store]` — LadybugDB database file path and batching knobs
//! - `[mute]` — paths to silence via `es_mute_path`
//!
//! ## Example
//!
//! ```toml
//! [events]
//! process = ["notify_exec", "notify_fork", "notify_exit"]
//! file = ["notify_create", "notify_write"]
//! network = ["notify_uipc_bind"]
//!
//! [store]
//! path = "/opt/esgraph/data/events.lbug"
//! batch_size = 500
//! ```

use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::events::EsEventName;

/// Root configuration structure loaded from TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// ESF event subscription lists.
    pub events: EventsConfig,
    /// LadybugDB storage settings.
    pub store: StoreConfig,
    /// Optional ESF muting rules (reduces noise from system paths).
    #[serde(default)]
    pub mute: MuteConfig,
}

/// Event subscription groups from `[events]` in TOML.
///
/// Each field is a list of snake_case event names (see [`EsEventName`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsConfig {
    /// Process-related events (`notify_exec`, `notify_fork`, …).
    #[serde(default)]
    pub process: Vec<String>,
    /// File-related events (`notify_create`, `notify_write`, …).
    #[serde(default)]
    pub file: Vec<String>,
    /// UIPC / UNIX socket events — phase 1 stand-in for "network".
    #[serde(default)]
    pub network: Vec<String>,
}

/// LadybugDB persistence settings from `[store]` in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreConfig {
    /// Filesystem path to the LadybugDB database file (plus a `.wal` sibling at runtime).
    ///
    /// When running `sudo esgraphd`, avoid `~` — it expands to `/var/root`.
    /// Use an explicit path like `/opt/esgraph/data/events.lbug` on the VM.
    pub path: String,
    /// Max normalised events per writer batch before flush.
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Max milliseconds between flushes even if batch is not full.
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
}

/// Paths to mute via ESF `es_mute_path`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MuteConfig {
    /// Prefix paths to mute (e.g. `/System`, `/private/var/db`).
    #[serde(default)]
    pub paths: Vec<String>,
}

fn default_batch_size() -> usize {
    500
}

fn default_flush_interval_ms() -> u64 {
    1000
}

impl Default for Config {
    /// Defaults matching `config/default.toml`.
    fn default() -> Self {
        Self {
            events: EventsConfig {
                process: vec![
                    "notify_exec".into(),
                    "notify_fork".into(),
                    "notify_exit".into(),
                ],
                file: vec![
                    "notify_create".into(),
                    "notify_write".into(),
                    "notify_unlink".into(),
                    "notify_rename".into(),
                ],
                network: vec![
                    "notify_uipc_bind".into(),
                    "notify_uipc_connect".into(),
                ],
            },
            store: StoreConfig {
                path: "data/events.lbug".into(),
                batch_size: default_batch_size(),
                flush_interval_ms: default_flush_interval_ms(),
            },
            mute: MuteConfig {
                paths: vec![
                    "/System".into(),
                    "/private/var/db".into(),
                ],
            },
        }
    }
}

impl Config {
    /// Load and validate configuration from a TOML file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_str(&contents)
    }

    /// Parse and validate configuration from a TOML string.
    pub fn from_str(toml_str: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(toml_str).map_err(|source| ConfigError::Parse {
            path: "<string>".into(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Semantic validation after deserialisation.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.store.batch_size == 0 {
            return Err(ConfigError::Validation(
                "store.batch_size must be > 0".into(),
            ));
        }
        if self.store.path.trim().is_empty() {
            return Err(ConfigError::Validation("store.path must not be empty".into()));
        }

        self.resolved_event_names()?;
        Ok(())
    }

    /// Flatten `[events]` groups into a deduplicated list of [`EsEventName`].
    ///
    /// Order is preserved: process → file → network, first occurrence wins.
    pub fn resolved_event_names(&self) -> Result<Vec<EsEventName>, ConfigError> {
        let mut out = Vec::new();
        for raw in self
            .events
            .process
            .iter()
            .chain(self.events.file.iter())
            .chain(self.events.network.iter())
        {
            let name = EsEventName::from_str(raw)?;
            if !out.contains(&name) {
                out.push(name);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        let cfg = Config::default();
        cfg.validate().unwrap();
        let names = cfg.resolved_event_names().unwrap();
        assert!(!names.is_empty());
        assert!(!names.iter().any(|n| n.is_auth()));
    }

    #[test]
    fn rejects_unknown_event_name() {
        let toml = r#"
            [events]
            process = ["not_a_real_event"]
            [store]
            path = "/tmp/x.graph"
        "#;
        let err = Config::from_str(toml).unwrap_err();
        match err {
            ConfigError::Validation(msg) => assert!(msg.contains("not_a_real_event")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}
