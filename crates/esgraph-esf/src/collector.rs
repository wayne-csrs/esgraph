//! Live ESF client loop — subscribe, mute paths, forward normalised events.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Duration;

use endpoint_sec::{Action, Client, Message};
use endpoint_sec::sys::{es_auth_result_t, es_mute_path_type_t};
use esgraph_core::{Config, NormalisedEvent};
use tracing::{info, warn};

use crate::error::EsfError;
use crate::normalise::normalise_message;
use crate::subscribe::event_names_to_es_types;

/// Create an ES client, subscribe per config, and dispatch until `shutdown` is set.
///
/// Blocks the calling thread until `shutdown` becomes true. The `Client` is dropped when this
/// function returns.
pub fn run_collector(
    config: &Config,
    tx: SyncSender<NormalisedEvent>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), EsfError> {
    init_runtime_version();

    let event_names = config
        .resolved_event_names()
        .map_err(|e| EsfError::Subscription(e.to_string()))?;
    let es_types = event_names_to_es_types(&event_names)?;

    info!(
        count = event_names.len(),
        "subscribing to configured ESF events"
    );

    // Client must be created and dropped on this thread (not Send).
    let mut client = Client::new(move |client, msg: Message| {
        handle_message(client, &msg, &tx);
    })
    .map_err(|e| EsfError::Client(format_client_error(&e)))?;

    apply_path_mutes(&mut client, config)?;

    client
        .subscribe(&es_types)
        .map_err(|e| EsfError::Client(e.to_string()))?;

    info!("ESF client active — press Ctrl+C to stop");
    for name in &event_names {
        info!(event = %name, "subscribed");
    }

    while !shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
    }

    info!("shutting down ESF client");
    Ok(())
}

fn handle_message(client: &mut Client<'_>, msg: &Message, tx: &SyncSender<NormalisedEvent>) {
    // AUTH events must be answered or macOS may kill the client.
    if matches!(msg.action(), Some(Action::Auth(_))) {
        if let Err(e) = client.respond_auth_result(msg, es_auth_result_t::ES_AUTH_RESULT_ALLOW, false) {
            warn!(error = %e, "failed to respond ALLOW to AUTH event");
        }
    }

    match normalise_message(msg) {
        Ok(Some(ev)) => {
            if tx.send(ev).is_err() {
                warn!("writer channel closed — events will be dropped");
            }
        }
        Ok(None) => {}
        Err(e) => {
            warn!(error = %e, "skipping message after normalisation error");
        }
    }
}

fn apply_path_mutes(client: &mut Client<'_>, config: &Config) -> Result<(), EsfError> {
    let mut muted: Vec<String> = config.mute.paths.clone();

    if let Some(store_prefix) = store_mute_prefix(&config.store.path) {
        if !muted.iter().any(|p| p == &store_prefix) {
            muted.push(store_prefix);
        }
    }

    for path in &muted {
        client
            .mute_path(OsStr::new(path), es_mute_path_type_t::ES_MUTE_PATH_TYPE_PREFIX)
            .map_err(|e| EsfError::Client(format!("mute_path {path}: {e}")))?;
        info!(path, "muted path prefix via es_mute_path");
    }
    Ok(())
}

/// Resolve the store directory to an absolute prefix suitable for `es_mute_path`.
fn store_mute_prefix(store_path: &str) -> Option<String> {
    let path = Path::new(store_path.trim());
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;

    let absolute = if parent.is_absolute() {
        parent.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(parent)
    } else {
        parent.to_path_buf()
    };

    canonicalize_prefix(absolute).and_then(|p| p.to_str().map(str::to_owned))
}

fn canonicalize_prefix(path: PathBuf) -> Option<PathBuf> {
    if path.exists() {
        path.canonicalize().ok().or(Some(path))
    } else {
        path.parent()
            .and_then(|p| p.canonicalize().ok())
            .and_then(|p| {
                path.file_name()
                    .map(|name| p.join(name))
            })
            .or(Some(path))
    }
}

fn init_runtime_version() {
    // endpoint-sec defaults to 10.15.0; set the real host version so newer APIs work.
    let Ok(out) = std::process::Command::new("sw_vers")
        .args(["-productVersion"])
        .output()
    else {
        return;
    };
    let Ok(version) = String::from_utf8(out.stdout) else {
        return;
    };
    let parts: Vec<u64> = version
        .trim()
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect();
    let major = parts.first().copied().unwrap_or(10);
    let minor = parts.get(1).copied().unwrap_or(15);
    let patch = parts.get(2).copied().unwrap_or(0);
    endpoint_sec::version::set_runtime_version(major, minor, patch);
}

fn format_client_error(e: &endpoint_sec::sys::NewClientError) -> String {
    let msg = e.to_string();
    if msg.contains("NOT_ENTITLED") || msg.contains("NotEntitled") {
        format!(
            "{msg}\nhint: embed com.apple.developer.endpoint-security.client and ad-hoc sign (see docs/vm-setup.md)"
        )
    } else if msg.contains("NOT_PERMITTED") || msg.contains("NotPermitted") {
        format!(
            "{msg}\nhint: grant Full Disk Access to esgraphd and Terminal in System Settings"
        )
    } else if msg.contains("NOT_PRIVILEGED") || msg.contains("NotPrivileged") {
        format!("{msg}\nhint: run with sudo")
    } else {
        msg
    }
}
