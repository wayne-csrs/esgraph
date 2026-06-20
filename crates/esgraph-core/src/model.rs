//! Graph-oriented normalised representation of ESF events.
//!
//! ESF delivers low-level `es_message_t` structures. Before LadybugDB ingest we normalise each
//! message into nodes (process / file / socket) and typed edges so hunt queries can traverse
//! relationships without re-parsing raw ESF payloads.

use serde::{Deserialize, Serialize};

/// Stable key for a process node in the graph.
///
/// ESF reuses PIDs after exit; `audit_token_t` is unique per process instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessIdentity {
    /// Hex-encoded `audit_token_t` bytes (primary graph key).
    pub audit_token_hex: String,
    /// PID at event time (informational; may be reused later).
    pub pid: i32,
}

/// Process vertex — executable identity and signing metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessNode {
    pub identity: ProcessIdentity,
    /// Path to the executable image.
    pub path: String,
    pub signing_id: Option<String>,
    pub team_id: Option<String>,
    /// Code directory hash (hex), when present.
    pub cdhash: Option<String>,
    pub ppid: Option<i32>,
    /// Effective user ID from the audit token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub euid: Option<u32>,
    /// Effective group ID from the audit token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egid: Option<u32>,
    /// Real user ID from the audit token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruid: Option<u32>,
    /// Real group ID from the audit token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgid: Option<u32>,
    /// Process session ID (macOS `session_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<i32>,
    /// Whether the binary is signed with Apple platform certificates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_platform_binary: Option<bool>,
    /// Parent process audit token (hex), when available from ESF v4+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_audit_token_hex: Option<String>,
    /// Command-line argv as JSON array string (set on `notify_exec` for the child process).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_json: Option<String>,
    /// Last known exit status (set on `notify_exit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
}

/// File path vertex.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    /// Inode from `stat` at event time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inode: Option<u64>,
    /// File mode (`st_mode`) at event time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    /// Owner UID from `stat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_uid: Option<u32>,
    /// Owner GID from `stat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_gid: Option<u32>,
    /// True when ESF truncated the path string.
    #[serde(default, skip_serializing_if = "is_false")]
    pub path_truncated: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// UNIX domain socket path vertex (UIPC events).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SocketNode {
    pub path: String,
}

/// Discriminated union of graph node payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphNode {
    Process(ProcessNode),
    File(FileNode),
    Socket(SocketNode),
}

impl GraphNode {
    /// Stable string id used as `src_id` / `dst_id` on edges.
    pub fn id(&self) -> String {
        match self {
            GraphNode::Process(p) => p.identity.audit_token_hex.clone(),
            GraphNode::File(f) => f.path.clone(),
            GraphNode::Socket(s) => s.path.clone(),
        }
    }

    /// Node kind label stored on edges (`process`, `file`, `socket`).
    pub fn kind_label(&self) -> &'static str {
        match self {
            GraphNode::Process(_) => "process",
            GraphNode::File(_) => "file",
            GraphNode::Socket(_) => "socket",
        }
    }
}

/// Relationship type between two nodes (hunt-friendly names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeKind {
  /// Process executed another process (`notify_exec`).
  Executed,
  /// Parent forked child (`notify_fork`).
  Forked,
  /// Process exited (`notify_exit` — self-loop for lifecycle).
  Exited,
  /// Remote thread creation in another process.
  RemoteThreadCreated,
  /// Task port access (`notify_get_task`).
  GotTask,
  /// File created (`notify_create`).
  Created,
  /// File written (`notify_write`).
  Wrote,
  /// File unlinked (`notify_unlink`).
  Unlinked,
  /// File renamed (`notify_rename` — use metadata for destination).
  Renamed,
  /// File opened (`notify_open`).
  Opened,
  /// File closed (`notify_close`).
  Closed,
  /// UIPC bind (`notify_uipc_bind`).
  UipcBound,
  /// UIPC connect (`notify_uipc_connect`).
  UipcConnected,
}

/// Directed edge in the event graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
    pub src_id: String,
    pub src_kind: String,
    pub dst_id: String,
    pub dst_kind: String,
    pub timestamp_unix_ns: i64,
    /// Optional JSON metadata (e.g. rename destination path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

/// Event-type-specific fields useful for security hunting (stored as `context_json` on `IngestEvent`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EventDetails {
    /// Instigator process PID at event time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instigator_pid: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instigator_euid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instigator_egid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instigator_path: Option<String>,
    /// Instigator `audit_token_t` hex (stable process instance key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instigator_audit_token_hex: Option<String>,
    /// Target process `audit_token_t` hex (`notify_exec` child).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_audit_token_hex: Option<String>,
    /// Target executable path (`notify_exec` child).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    /// Full argv for `notify_exec`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec_args: Option<Vec<String>>,
    /// Exit status for `notify_exit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    /// Target file mode (`st_mode`) when a file is involved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_mode: Option<u32>,
    /// Target file inode when a file is involved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_inode: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_owner_uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_owner_gid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_truncated: Option<bool>,
}

impl EventDetails {
    /// True when no analyst-facing fields are set.
    pub fn is_empty(&self) -> bool {
        self.instigator_pid.is_none()
            && self.instigator_euid.is_none()
            && self.instigator_egid.is_none()
            && self.instigator_path.is_none()
            && self.instigator_audit_token_hex.is_none()
            && self.target_audit_token_hex.is_none()
            && self.target_path.is_none()
            && self.exec_args.is_none()
            && self.exit_status.is_none()
            && self.file_mode.is_none()
            && self.file_inode.is_none()
            && self.file_owner_uid.is_none()
            && self.file_owner_gid.is_none()
            && self.path_truncated.is_none()
    }
}

/// One ESF message reduced to graph primitives for storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalisedEvent {
    /// Config / registry name (e.g. `notify_write`).
    pub event_name: String,
    /// `es_message_t` time converted to Unix nanoseconds.
    pub timestamp_unix_ns: i64,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Event-type-specific analyst fields (serialized to `context_json` on ingest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<EventDetails>,
    /// Optional serialised raw payload for forensics (size-limited at collector).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_json: Option<String>,
}

/// Node kind for Cypher filters (mirrors `GraphNode` tags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Process,
    File,
    Socket,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Process => "process",
            NodeKind::File => "file",
            NodeKind::Socket => "socket",
        }
    }
}
