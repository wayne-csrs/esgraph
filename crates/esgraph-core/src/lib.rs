//! # esgraph-core
//!
//! Shared foundation for **esgraph**.
//!
//! ## Purpose
//!
//! This crate holds types and configuration that every other crate depends on:
//!
//! - **[`Config`]** — TOML configuration (which ESF events to subscribe to, LadybugDB paths, etc.)
//! - **[`EsEventName`]** — human-readable names for ESF event types (mapped to `es_event_type_t` in `esgraph-esf`)
//! - **[`NormalisedEvent`]** — platform-neutral representation of an ESF message ready for graph ingest
//!
//! ## Why a separate core crate?
//!
//! We deliberately keep **no** dependencies on:
//!
//! - `endpoint-sec` (macOS-only, requires entitlements)
//! - `lbug` (in esgraph-store only)
//!
//! That means `esgraph-core` compiles on any machine and can be unit-tested without a live ESF client.
//! Live ESF collection (`esgraph-esf`) and storage (`esgraph-store`) depend on this crate.
//!
//! ## References
//!
//! - Apple Endpoint Security overview: <https://developer.apple.com/documentation/endpointsecurity>
//! - `es_event_type_t` enum: <https://developer.apple.com/documentation/endpointsecurity/es_event_type_t>

#![warn(missing_docs)]

mod config;
mod error;
mod events;
mod model;

pub use config::{Config, EventsConfig, MuteConfig, StoreConfig};
pub use error::{ConfigError, CoreError};
pub use events::{EsEventCategory, EsEventName};
pub use model::{
    EdgeKind, EventDetails, FileNode, GraphEdge, GraphNode, NormalisedEvent, NodeKind,
    ProcessIdentity, ProcessNode, SocketNode,
};
