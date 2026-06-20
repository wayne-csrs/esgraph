//! # esgraph-esf
//!
//! Live macOS Endpoint Security Framework (ESF) collector.
//!
//! Subscribes to configured NOTIFY (and optional AUTH) events, normalises each `es_message_t`
//! into [`NormalisedEvent`](esgraph_core::NormalisedEvent), and sends them to a channel for
//! the LadybugDB writer thread in `esgraphd run`.
//!
//! ## Platform
//!
//! Full implementation is **macOS only** (`endpoint-sec` crate). On other targets,
//! [`run_collector`] returns [`EsfError::UnsupportedPlatform`].
//!
//! ## Threading
//!
//! `endpoint_sec::Client` is neither `Send` nor `Sync`. The client and its handler run on the
//! thread that calls [`run_collector`]. Normalised events cross to the writer via `std::sync::mpsc`.
//!
//! ## References
//!
//! - <https://developer.apple.com/documentation/endpointsecurity>
//! - <https://docs.rs/endpoint-sec>

#![warn(missing_docs)]

mod error;

#[cfg(target_os = "macos")]
mod collector;
#[cfg(target_os = "macos")]
mod normalise;
#[cfg(target_os = "macos")]
mod subscribe;

pub use error::EsfError;

#[cfg(target_os = "macos")]
pub use collector::run_collector;

#[cfg(not(target_os = "macos"))]
use esgraph_core::Config;
#[cfg(not(target_os = "macos"))]
use esgraph_core::NormalisedEvent;
#[cfg(not(target_os = "macos"))]
use std::sync::atomic::AtomicBool;
#[cfg(not(target_os = "macos"))]
use std::sync::mpsc::SyncSender;
#[cfg(not(target_os = "macos"))]
use std::sync::Arc;

/// Run the ESF collector until `shutdown` is set.
#[cfg(not(target_os = "macos"))]
pub fn run_collector(
    _config: &Config,
    _tx: SyncSender<NormalisedEvent>,
    _shutdown: Arc<AtomicBool>,
) -> Result<(), EsfError> {
    Err(EsfError::UnsupportedPlatform)
}
