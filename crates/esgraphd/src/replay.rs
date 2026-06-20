//! JSON fixture loader for [`esgraphd replay`].
//!
//! Fixtures let us exercise the full ingest → LadybugDB → hunt-query path on the host Mac
//! without ESF entitlements, root, or a VM.
//!
//! ## File formats
//!
//! A fixture file may contain either:
//!
//! - A single [`NormalisedEvent`] JSON object
//! - An array of [`NormalisedEvent`] objects
//!
//! Field names match `esgraph-core` serde conventions (`event_name: "notify_write"`, etc.).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use esgraph_core::NormalisedEvent;

/// Load one or more events from a JSON fixture path.
pub fn load_fixture(path: &Path) -> Result<Vec<NormalisedEvent>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read fixture {}", path.display()))?;

    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        let events: Vec<NormalisedEvent> = serde_json::from_str(trimmed)
            .with_context(|| format!("parse fixture array {}", path.display()))?;
        Ok(events)
    } else {
        let event: NormalisedEvent = serde_json::from_str(trimmed)
            .with_context(|| format!("parse fixture object {}", path.display()))?;
        Ok(vec![event])
    }
}

/// Load multiple fixture files in order.
pub fn load_fixtures(paths: &[impl AsRef<Path>]) -> Result<Vec<NormalisedEvent>> {
    let mut all = Vec::new();
    for path in paths {
        let mut events = load_fixture(path.as_ref())?;
        all.append(&mut events);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_single_object_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/write_event.json");
        let events = load_fixture(&path).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name.to_string(), "notify_write");
    }

    #[test]
    fn loads_array_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/exec_chain.json");
        let events = load_fixture(&path).unwrap();
        assert_eq!(events.len(), 2);
    }
}
