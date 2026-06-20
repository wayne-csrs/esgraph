//! ESF event name registry.
//!
//! Apple defines event types as the C enum `es_event_type_t` (e.g. `ES_EVENT_TYPE_NOTIFY_EXEC`).
//! For configurability we use **snake_case strings** in TOML (`notify_exec`) and map
//! them to this enum here. The macOS-specific `es_event_type_t` conversion lives in `esgraph-esf`
//! so this crate stays platform-independent.
//!
//! ## NOTIFY vs AUTH
//!
//! ESF has two event classes:
//!
//! - **NOTIFY** — async telemetry; the operation proceeds immediately. Safe for graph building.
//! - **AUTH** — sync gate; the kernel blocks until we respond or a deadline expires. Slow handlers
//!   can get our client killed by macOS.
//!
//! Our defaults use NOTIFY-only names. AUTH variants exist in this enum for opt-in use.
//!
//! ## Network caveat
//!
//! ESF does **not** expose general TCP/UDP events. Phase 1 "network" means **UIPC** (UNIX domain
//! socket) bind/connect events only. Full IP connectivity requires Network Extension (phase 2).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::ConfigError;

/// High-level grouping used in [`crate::Config`] TOML (`[events].process`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EsEventCategory {
    /// Process lifecycle and task-port events (`notify_exec`, `notify_fork`, …).
    Process,
    /// File and filesystem events (`notify_create`, `notify_write`, …).
    File,
    /// UNIX-domain socket events (`notify_uipc_bind`, `notify_uipc_connect`).
    Network,
}

/// Human-readable name for a single ESF event type.
///
/// String form matches TOML config keys (snake_case, no `ES_EVENT_TYPE_` prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EsEventName {
    // --- Process (NOTIFY) ---
    /// A new process image is about to run (`ES_EVENT_TYPE_NOTIFY_EXEC`).
    NotifyExec,
    /// A process forked a child (`ES_EVENT_TYPE_NOTIFY_FORK`).
    NotifyFork,
    /// A process is exiting (`ES_EVENT_TYPE_NOTIFY_EXIT`).
    NotifyExit,
    /// Remote thread creation — injection precursor (`ES_EVENT_TYPE_NOTIFY_REMOTE_THREAD_CREATE`).
    NotifyRemoteThreadCreate,
    /// Task port access (`ES_EVENT_TYPE_NOTIFY_GET_TASK`).
    NotifyGetTask,

    // --- Process (AUTH) — opt-in only ---
    /// Block/allow exec (`ES_EVENT_TYPE_AUTH_EXEC`).
    AuthExec,

    // --- File (NOTIFY) ---
    /// File created (`ES_EVENT_TYPE_NOTIFY_CREATE`).
    NotifyCreate,
    /// File written (`ES_EVENT_TYPE_NOTIFY_WRITE`).
    NotifyWrite,
    /// File deleted (`ES_EVENT_TYPE_NOTIFY_UNLINK`).
    NotifyUnlink,
    /// File renamed (`ES_EVENT_TYPE_NOTIFY_RENAME`).
    NotifyRename,
    /// File opened (`ES_EVENT_TYPE_NOTIFY_OPEN`) — **very high volume** on desktop systems.
    NotifyOpen,
    /// File closed (`ES_EVENT_TYPE_NOTIFY_CLOSE`).
    NotifyClose,

    // --- File (AUTH) — opt-in only ---
    /// Block/allow file open (`ES_EVENT_TYPE_AUTH_OPEN`).
    AuthOpen,

    // --- UIPC / "network" phase 1 (NOTIFY) ---
    /// UNIX domain socket bind (`ES_EVENT_TYPE_NOTIFY_UIPC_BIND`).
    NotifyUipcBind,
    /// UNIX domain socket connect (`ES_EVENT_TYPE_NOTIFY_UIPC_CONNECT`).
    NotifyUipcConnect,
}

impl EsEventName {
    /// Whether this is an AUTH (synchronous) event type.
    ///
    /// AUTH handlers must respond before `msg.deadline` or macOS may terminate the client.
    pub fn is_auth(self) -> bool {
        matches!(self, Self::AuthExec | Self::AuthOpen)
    }

    /// Default NOTIFY-only subscription set; excludes high-volume `notify_open`.
    pub fn default_subscription_set() -> Vec<Self> {
        vec![
            Self::NotifyExec,
            Self::NotifyFork,
            Self::NotifyExit,
            Self::NotifyCreate,
            Self::NotifyWrite,
            Self::NotifyUnlink,
            Self::NotifyRename,
            Self::NotifyUipcBind,
            Self::NotifyUipcConnect,
        ]
    }

    /// All valid string names (for error messages and docs).
    pub fn all_names() -> &'static [&'static str] {
        &[
            "notify_exec",
            "notify_fork",
            "notify_exit",
            "notify_remote_thread_create",
            "notify_get_task",
            "auth_exec",
            "notify_create",
            "notify_write",
            "notify_unlink",
            "notify_rename",
            "notify_open",
            "notify_close",
            "auth_open",
            "notify_uipc_bind",
            "notify_uipc_connect",
        ]
    }
}

impl fmt::Display for EsEventName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NotifyExec => "notify_exec",
            Self::NotifyFork => "notify_fork",
            Self::NotifyExit => "notify_exit",
            Self::NotifyRemoteThreadCreate => "notify_remote_thread_create",
            Self::NotifyGetTask => "notify_get_task",
            Self::AuthExec => "auth_exec",
            Self::NotifyCreate => "notify_create",
            Self::NotifyWrite => "notify_write",
            Self::NotifyUnlink => "notify_unlink",
            Self::NotifyRename => "notify_rename",
            Self::NotifyOpen => "notify_open",
            Self::NotifyClose => "notify_close",
            Self::AuthOpen => "auth_open",
            Self::NotifyUipcBind => "notify_uipc_bind",
            Self::NotifyUipcConnect => "notify_uipc_connect",
        };
        write!(f, "{s}")
    }
}

impl FromStr for EsEventName {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let name = match s {
            "notify_exec" => Self::NotifyExec,
            "notify_fork" => Self::NotifyFork,
            "notify_exit" => Self::NotifyExit,
            "notify_remote_thread_create" => Self::NotifyRemoteThreadCreate,
            "notify_get_task" => Self::NotifyGetTask,
            "auth_exec" => Self::AuthExec,
            "notify_create" => Self::NotifyCreate,
            "notify_write" => Self::NotifyWrite,
            "notify_unlink" => Self::NotifyUnlink,
            "notify_rename" => Self::NotifyRename,
            "notify_open" => Self::NotifyOpen,
            "notify_close" => Self::NotifyClose,
            "auth_open" => Self::AuthOpen,
            "notify_uipc_bind" => Self::NotifyUipcBind,
            "notify_uipc_connect" => Self::NotifyUipcConnect,
            other => {
                return Err(ConfigError::Validation(format!(
                    "unknown ESF event name '{other}'; valid names: {}",
                    Self::all_names().join(", ")
                )));
            }
        };
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_names_roundtrip() {
        for &name in EsEventName::all_names() {
            let parsed: EsEventName = name.parse().unwrap();
            assert_eq!(parsed.to_string(), name);
        }
    }

    #[test]
    fn default_subscription_set_is_notify_only() {
        for ev in EsEventName::default_subscription_set() {
            assert!(!ev.is_auth(), "{ev} should not be AUTH in default set");
        }
    }
}
