//! Batch ingest: upsert nodes, append edges, record ingest audit rows.

use std::sync::atomic::{AtomicU64, Ordering};

use esgraph_core::{
    EdgeKind, EventDetails, FileNode, GraphEdge, GraphNode, NormalisedEvent, ProcessNode,
    SocketNode,
};
use lbug::{Connection, Database};
use tracing::debug;

use crate::cypher::{
    cypher_bool, cypher_i64, cypher_str, join_set_clauses, opt_bool_assign, opt_i64_assign,
    opt_str_assign, parse_count_value,
};
use crate::error::StoreError;

/// Counters returned after a successful batch ingest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestStats {
    /// Number of normalised events ingested.
    pub events: usize,
    /// Number of node upserts performed.
    pub node_upserts: usize,
    /// Number of edges appended.
    pub edges_inserted: usize,
}

fn details_to_json(details: &Option<EventDetails>) -> Option<String> {
    let d = details.as_ref()?;
    if d.is_empty() {
        return None;
    }
    serde_json::to_string(d).ok()
}

fn edge_kind_name(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Executed => "EXECUTED",
        EdgeKind::Forked => "FORKED",
        EdgeKind::Exited => "EXITED",
        EdgeKind::RemoteThreadCreated => "REMOTE_THREAD_CREATED",
        EdgeKind::GotTask => "GOT_TASK",
        EdgeKind::Created => "CREATED",
        EdgeKind::Wrote => "WROTE",
        EdgeKind::Unlinked => "UNLINKED",
        EdgeKind::Renamed => "RENAMED",
        EdgeKind::Opened => "OPENED",
        EdgeKind::Closed => "CLOSED",
        EdgeKind::UipcBound => "UIPC_BOUND",
        EdgeKind::UipcConnected => "UIPC_CONNECTED",
    }
}

fn node_label(kind: &str) -> &str {
    match kind {
        "process" => "Process",
        "file" => "File",
        "socket" => "Socket",
        other => panic!("unknown node kind: {other}"),
    }
}

fn process_set_clauses(p: &ProcessNode, seen_ns: i64, include_args: bool) -> Vec<String> {
    let n = "n";
    let mut clauses = vec![
        format!("{n}.pid = {}", cypher_i64(i64::from(p.identity.pid))),
        format!("{n}.path = {}", cypher_str(&p.path)),
        format!("{n}.last_seen_unix_ns = {}", cypher_i64(seen_ns)),
    ];
    clauses.extend(opt_str_assign(n, "signing_id", p.signing_id.as_deref()));
    clauses.extend(opt_str_assign(n, "team_id", p.team_id.as_deref()));
    clauses.extend(opt_str_assign(n, "cdhash", p.cdhash.as_deref()));
    clauses.extend(opt_i64_assign(n, "ppid", p.ppid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "euid", p.euid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "egid", p.egid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "ruid", p.ruid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "rgid", p.rgid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "session_id", p.session_id.map(i64::from)));
    clauses.extend(opt_bool_assign(n, "is_platform_binary", p.is_platform_binary));
    clauses.extend(opt_str_assign(
        n,
        "parent_audit_token_hex",
        p.parent_audit_token_hex.as_deref(),
    ));
    if include_args {
        if let Some(args) = p.args_json.as_deref() {
            clauses.push(format!("{n}.args_json = {}", cypher_str(args)));
        }
    }
    if let Some(status) = p.exit_status {
        clauses.push(format!("{n}.exit_status = {}", cypher_i64(i64::from(status))));
    }
    clauses
}

fn merge_process_stmt(p: &ProcessNode, seen_ns: i64) -> Result<String, StoreError> {
    let id = p.identity.audit_token_hex.as_str();
    if id.is_empty() {
        return Err(StoreError::InvalidEvent(
            "process node missing audit_token_hex".into(),
        ));
    }
    let create = join_set_clauses(&process_set_clauses(p, seen_ns, true));
    let on_match = join_set_clauses(&process_set_clauses(
        p,
        seen_ns,
        p.args_json.is_some(),
    ));
    Ok(format!(
        "MERGE (n:Process {{id: {}}}) ON CREATE SET {create} ON MATCH SET {on_match}",
        cypher_str(id)
    ))
}

fn file_set_clauses(f: &FileNode, seen_ns: i64) -> Vec<String> {
    let n = "n";
    let mut clauses = vec![
        format!("{n}.path = {}", cypher_str(&f.path)),
        format!("{n}.last_seen_unix_ns = {}", cypher_i64(seen_ns)),
        format!(
            "{n}.path_truncated = {}",
            cypher_bool(f.path_truncated)
        ),
    ];
    clauses.extend(opt_i64_assign(n, "inode", f.inode.map(|v| v as i64)));
    clauses.extend(opt_i64_assign(n, "mode", f.mode.map(|m| i64::from(u32::from(m)))));
    clauses.extend(opt_i64_assign(n, "owner_uid", f.owner_uid.map(i64::from)));
    clauses.extend(opt_i64_assign(n, "owner_gid", f.owner_gid.map(i64::from)));
    clauses
}

fn merge_file_stmt(f: &FileNode, seen_ns: i64) -> String {
    let sets = join_set_clauses(&file_set_clauses(f, seen_ns));
    format!(
        "MERGE (n:File {{id: {}}}) ON CREATE SET {sets} ON MATCH SET {sets}",
        cypher_str(&f.path)
    )
}

fn merge_socket_stmt(s: &SocketNode, seen_ns: i64) -> String {
    let sets = join_set_clauses(&[
        format!("n.path = {}", cypher_str(&s.path)),
        format!("n.last_seen_unix_ns = {}", cypher_i64(seen_ns)),
    ]);
    format!(
        "MERGE (n:Socket {{id: {}}}) ON CREATE SET {sets} ON MATCH SET {sets}",
        cypher_str(&s.path)
    )
}

fn merge_node_stmt(node: &GraphNode, seen_ns: i64) -> Result<String, StoreError> {
    match node {
        GraphNode::Process(p) => merge_process_stmt(p, seen_ns),
        GraphNode::File(f) => Ok(merge_file_stmt(f, seen_ns)),
        GraphNode::Socket(s) => Ok(merge_socket_stmt(s, seen_ns)),
    }
}

fn merge_stub_node_stmt(label: &str, id: &str) -> String {
    format!("MERGE (n:{label} {{id: {}}})", cypher_str(id))
}

fn insert_ingest_event_stmt(ingest_id: u64, event: &NormalisedEvent) -> String {
    let id = i64::try_from(ingest_id).unwrap_or(i64::MAX);
    let mut sets = vec![
        format!("n.event_name = {}", cypher_str(&event.event_name)),
        format!(
            "n.timestamp_unix_ns = {}",
            cypher_i64(event.timestamp_unix_ns)
        ),
    ];
    if let Some(ctx) = details_to_json(&event.details) {
        sets.push(format!("n.context_json = {}", cypher_str(&ctx)));
    }
    if let Some(raw) = event.raw_json.as_deref() {
        sets.push(format!("n.raw_json = {}", cypher_str(raw)));
    }
    let set_clause = join_set_clauses(&sets);
    format!(
        "MERGE (n:IngestEvent {{id: {}}}) ON CREATE SET {set_clause} ON MATCH SET {set_clause}",
        cypher_i64(id)
    )
}

fn insert_edge_stmt(
    edge: &GraphEdge,
    event_name: &str,
    ingest_event_id: u64,
) -> String {
    let rel = edge_kind_name(edge.kind);
    let src_label = node_label(&edge.src_kind);
    let dst_label = node_label(&edge.dst_kind);
    let ingest_id = i64::try_from(ingest_event_id).unwrap_or(i64::MAX);

    let mut props = vec![
        format!(
            "timestamp_unix_ns: {}",
            cypher_i64(edge.timestamp_unix_ns)
        ),
        format!("event_name: {}", cypher_str(event_name)),
        format!("ingest_event_id: {}", cypher_i64(ingest_id)),
    ];
    if let Some(meta) = edge.metadata.as_deref() {
        props.push(format!("metadata: {}", cypher_str(meta)));
    }
    let props_body = props.join(", ");

    format!(
        "MATCH (a:{src_label} {{id: {}}}), (b:{dst_label} {{id: {}}}) \
         CREATE (a)-[:{rel} {{{props_body}}}]->(b)",
        cypher_str(&edge.src_id),
        cypher_str(&edge.dst_id),
    )
}

fn ensure_edge_endpoints(edge: &GraphEdge) -> [String; 2] {
    let src_label = node_label(&edge.src_kind);
    let dst_label = node_label(&edge.dst_kind);
    [
        merge_stub_node_stmt(src_label, &edge.src_id),
        merge_stub_node_stmt(dst_label, &edge.dst_id),
    ]
}

/// Ingest a slice of normalised events in one Ladybug write batch.
pub fn ingest_batch(
    db: &Database,
    next_ingest_id: &AtomicU64,
    events: &[NormalisedEvent],
) -> Result<IngestStats, StoreError> {
    let mut stats = IngestStats::default();
    if events.is_empty() {
        return Ok(stats);
    }

    let conn = Connection::new(db)?;
    let mut statements = Vec::new();

    for event in events {
        let ingest_id = next_ingest_id.fetch_add(1, Ordering::SeqCst);
        statements.push(insert_ingest_event_stmt(ingest_id, event));

        for node in &event.nodes {
            statements.push(merge_node_stmt(node, event.timestamp_unix_ns)?);
            stats.node_upserts += 1;
        }

        for edge in &event.edges {
            for stub in ensure_edge_endpoints(edge) {
                statements.push(stub);
            }
            statements.push(insert_edge_stmt(edge, &event.event_name, ingest_id));
            stats.edges_inserted += 1;
        }

        stats.events += 1;
    }

    let batch = statements.join(";\n");
    conn.query(&batch)?;

    debug!(
        events = stats.events,
        node_upserts = stats.node_upserts,
        edges = stats.edges_inserted,
        "ingest batch committed"
    );

    Ok(stats)
}

/// Load the next ingest-event id from the graph (max existing id + 1).
pub fn load_next_ingest_id(db: &Database) -> Result<u64, StoreError> {
    let conn = Connection::new(db)?;
    let mut result = conn.query(
        "MATCH (e:IngestEvent) RETURN e.id AS id ORDER BY e.id DESC LIMIT 1",
    )?;
    if let Some(row) = result.next() {
        if let Some(id) = row.first().and_then(parse_count_value) {
            return Ok(id.saturating_add(1));
        }
    }
    Ok(1)
}

/// Sample event used in unit tests (notify_write).
pub fn sample_write_event() -> NormalisedEvent {
    use esgraph_core::{GraphNode, ProcessIdentity};

    let token = "aabbccdd0011".to_string();
    NormalisedEvent {
        event_name: "notify_write".into(),
        timestamp_unix_ns: 1_700_000_000_000_000_000,
        details: Some(EventDetails {
            instigator_pid: Some(4242),
            instigator_euid: Some(501),
            instigator_egid: Some(20),
            instigator_path: Some("/bin/zsh".into()),
            file_mode: Some(0o100644),
            file_inode: Some(12345),
            file_owner_uid: Some(501),
            file_owner_gid: Some(20),
            path_truncated: Some(false),
            ..Default::default()
        }),
        nodes: vec![
            GraphNode::Process(ProcessNode {
                identity: ProcessIdentity {
                    audit_token_hex: token.clone(),
                    pid: 4242,
                },
                path: "/bin/zsh".into(),
                signing_id: None,
                team_id: None,
                cdhash: None,
                ppid: Some(1),
                euid: Some(501),
                egid: Some(20),
                ruid: Some(501),
                rgid: Some(20),
                session_id: Some(100),
                is_platform_binary: Some(true),
                parent_audit_token_hex: None,
                args_json: None,
                exit_status: None,
            }),
            GraphNode::File(FileNode {
                path: "/tmp/research.txt".into(),
                inode: Some(12345),
                mode: Some(0o100644),
                owner_uid: Some(501),
                owner_gid: Some(20),
                path_truncated: false,
            }),
        ],
        edges: vec![GraphEdge {
            kind: EdgeKind::Wrote,
            src_id: token,
            src_kind: "process".into(),
            dst_id: "/tmp/research.txt".into(),
            dst_kind: "file".into(),
            timestamp_unix_ns: 1_700_000_000_000_000_000,
            metadata: None,
        }],
        raw_json: Some(r#"{"fixture":"write_event"}"#.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cypher::value_to_string;
    use esgraph_core::{GraphNode, ProcessIdentity, ProcessNode};
    use lbug::{Database, SystemConfig};
    use tempfile::tempdir;

    fn open_test_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("events.lbug");
        let db = Database::new(path, SystemConfig::default()).unwrap();
        {
            let conn = Connection::new(&db).unwrap();
            for ddl in crate::schema::INIT_SCHEMA {
                let _ = conn.query(ddl);
            }
        }
        (dir, db)
    }

    #[test]
    fn exec_args_preserved_on_non_exec_upsert() {
        let (_dir, db) = open_test_db();
        let next_id = AtomicU64::new(1);

        let token = "execchild".to_string();
        let exec_event = NormalisedEvent {
            event_name: "notify_exec".into(),
            timestamp_unix_ns: 1,
            details: Some(EventDetails {
                exec_args: Some(vec!["/bin/sh".into(), "-c".into(), "id".into()]),
                ..Default::default()
            }),
            nodes: vec![GraphNode::Process(ProcessNode {
                identity: ProcessIdentity {
                    audit_token_hex: token.clone(),
                    pid: 100,
                },
                path: "/bin/sh".into(),
                signing_id: None,
                team_id: None,
                cdhash: None,
                ppid: Some(99),
                euid: None,
                egid: None,
                ruid: None,
                rgid: None,
                session_id: None,
                is_platform_binary: None,
                parent_audit_token_hex: None,
                args_json: Some(r#"["/bin/sh","-c","id"]"#.into()),
                exit_status: None,
            })],
            edges: vec![GraphEdge {
                kind: EdgeKind::Executed,
                src_id: "parent".into(),
                src_kind: "process".into(),
                dst_id: token.clone(),
                dst_kind: "process".into(),
                timestamp_unix_ns: 1,
                metadata: None,
            }],
            raw_json: None,
        };

        let write_event = NormalisedEvent {
            event_name: "notify_write".into(),
            timestamp_unix_ns: 2,
            details: None,
            nodes: vec![GraphNode::Process(ProcessNode {
                identity: ProcessIdentity {
                    audit_token_hex: token.clone(),
                    pid: 100,
                },
                path: "/bin/sh".into(),
                signing_id: None,
                team_id: None,
                cdhash: None,
                ppid: Some(99),
                euid: None,
                egid: None,
                ruid: None,
                rgid: None,
                session_id: None,
                is_platform_binary: None,
                parent_audit_token_hex: None,
                args_json: None,
                exit_status: None,
            })],
            edges: vec![],
            raw_json: None,
        };

        let parent = NormalisedEvent {
            event_name: "notify_exec".into(),
            timestamp_unix_ns: 0,
            details: None,
            nodes: vec![GraphNode::Process(ProcessNode {
                identity: ProcessIdentity {
                    audit_token_hex: "parent".into(),
                    pid: 99,
                },
                path: "/bin/bash".into(),
                signing_id: None,
                team_id: None,
                cdhash: None,
                ppid: Some(1),
                euid: None,
                egid: None,
                ruid: None,
                rgid: None,
                session_id: None,
                is_platform_binary: None,
                parent_audit_token_hex: None,
                args_json: None,
                exit_status: None,
            })],
            edges: vec![],
            raw_json: None,
        };

        ingest_batch(&db, &next_id, &[parent, exec_event, write_event]).unwrap();

        let conn = Connection::new(&db).unwrap();
        let mut result = conn
            .query(&format!(
                "MATCH (p:Process {{id: {}}}) RETURN p.args_json",
                cypher_str(&token)
            ))
            .unwrap();
        let args = value_to_string(&result.next().unwrap()[0]);
        assert_eq!(args, r#"["/bin/sh","-c","id"]"#);
    }

    #[test]
    fn exec_chain_fixture_uses_audit_token_edges() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/exec_chain.json");
        let text = std::fs::read_to_string(&fixture).unwrap();
        let events: Vec<NormalisedEvent> = serde_json::from_str(&text).unwrap();
        assert_eq!(events.len(), 2);

        let exec = &events[0];
        assert_eq!(exec.event_name, "notify_exec");
        assert_eq!(exec.nodes.len(), 2);
        assert_eq!(exec.edges.len(), 1);

        let parent = exec.nodes.iter().find_map(|n| match n {
            GraphNode::Process(p) if p.path == "/bin/bash" => Some(p),
            _ => None,
        });
        let child = exec.nodes.iter().find_map(|n| match n {
            GraphNode::Process(p) if p.path == "/usr/bin/python3" => Some(p),
            _ => None,
        });
        let parent = parent.expect("parent process node");
        let child = child.expect("child process node from exec.target");

        assert_eq!(parent.identity.audit_token_hex, "parent0001");
        assert_eq!(child.identity.audit_token_hex, "child0002");
        assert_ne!(child.identity.audit_token_hex, parent.identity.audit_token_hex);
        assert_eq!(
            child.args_json.as_deref(),
            Some(r#"["/usr/bin/python3","-c","import os; os.system('id')"]"#)
        );

        let edge = &exec.edges[0];
        assert_eq!(edge.kind, EdgeKind::Executed);
        assert_eq!(edge.src_id, "parent0001");
        assert_eq!(edge.dst_id, "child0002");
        assert_eq!(edge.src_kind, "process");
        assert_eq!(edge.dst_kind, "process");

        let details = exec.details.as_ref().expect("exec event details");
        assert_eq!(details.instigator_audit_token_hex.as_deref(), Some("parent0001"));
        assert_eq!(details.target_audit_token_hex.as_deref(), Some("child0002"));

        let (_dir, db) = open_test_db();
        let next_id = AtomicU64::new(1);
        ingest_batch(&db, &next_id, &events).unwrap();

        let conn = Connection::new(&db).unwrap();
        let mut result = conn
            .query(
                "MATCH (parent:Process {id: 'parent0001'})-[r:EXECUTED]->(child:Process {id: 'child0002'}) \
                 RETURN child.path, child.args_json, r.event_name",
            )
            .unwrap();
        let row = result.next().expect("EXECUTED edge in graph");
        assert_eq!(value_to_string(&row[0]), "/usr/bin/python3");
        assert_eq!(
            value_to_string(&row[1]),
            r#"["/usr/bin/python3","-c","import os; os.system('id')"]"#
        );
        assert_eq!(value_to_string(&row[2]), "notify_exec");
    }
}
