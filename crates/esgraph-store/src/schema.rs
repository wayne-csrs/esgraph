//! LadybugDB schema DDL and example hunt queries.

/// Node and relationship table definitions (idempotent via IF NOT EXISTS).
pub const INIT_SCHEMA: &[&str] = &[
    r#"CREATE NODE TABLE IF NOT EXISTS Process (
        id STRING PRIMARY KEY,
        pid INT64,
        path STRING,
        last_seen_unix_ns INT64,
        signing_id STRING,
        team_id STRING,
        cdhash STRING,
        ppid INT64,
        euid INT64,
        egid INT64,
        ruid INT64,
        rgid INT64,
        session_id INT64,
        is_platform_binary BOOLEAN,
        parent_audit_token_hex STRING,
        args_json STRING,
        exit_status INT64
    )"#,
    r#"CREATE NODE TABLE IF NOT EXISTS File (
        id STRING PRIMARY KEY,
        path STRING,
        last_seen_unix_ns INT64,
        path_truncated BOOLEAN,
        inode INT64,
        mode INT64,
        owner_uid INT64,
        owner_gid INT64
    )"#,
    r#"CREATE NODE TABLE IF NOT EXISTS Socket (
        id STRING PRIMARY KEY,
        path STRING,
        last_seen_unix_ns INT64
    )"#,
    r#"CREATE NODE TABLE IF NOT EXISTS IngestEvent (
        id INT64 PRIMARY KEY,
        event_name STRING,
        timestamp_unix_ns INT64,
        context_json STRING,
        raw_json STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS EXECUTED (
        FROM Process TO Process,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS FORKED (
        FROM Process TO Process,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS EXITED (
        FROM Process TO Process,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS REMOTE_THREAD_CREATED (
        FROM Process TO Process,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS GOT_TASK (
        FROM Process TO Process,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS CREATED (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS WROTE (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS UNLINKED (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS RENAMED (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS OPENED (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS CLOSED (
        FROM Process TO File,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS UIPC_BOUND (
        FROM Process TO Socket,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
    r#"CREATE REL TABLE IF NOT EXISTS UIPC_CONNECTED (
        FROM Process TO Socket,
        timestamp_unix_ns INT64,
        event_name STRING,
        ingest_event_id INT64,
        metadata STRING
    )"#,
];

/// Example hunt: processes that wrote files.
pub const EXAMPLE_HUNT_CYPHER: &str = r#"
MATCH (p:Process)-[r:WROTE]->(f:File)
RETURN p.path, f.path
ORDER BY r.timestamp_unix_ns DESC
LIMIT 100
"#;

/// Example hunt: process executions with command-line arguments.
pub const EXAMPLE_EXEC_HUNT_CYPHER: &str = r#"
MATCH (p:Process)
WHERE p.args_json IS NOT NULL
RETURN p.path, p.args_json
LIMIT 100
"#;
