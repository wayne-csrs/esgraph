# esgraph-core

`esgraph-core` is the shared foundation crate. Every other crate depends on it for configuration, event naming, and the graph data model. It has no dependency on `endpoint-sec` or LadybugDB, so it compiles on any host and is easy to unit test in isolation.

## Modules

| Module | File | Responsibility |
|--------|------|----------------|
| `config` | `src/config.rs` | TOML loading and validation |
| `events` | `src/events.rs` | `EsEventName` registry and NOTIFY vs AUTH semantics |
| `model` | `src/model.rs` | Nodes, edges, `NormalisedEvent` |
| `error` | `src/error.rs` | `ConfigError`, `CoreError` |

## Configuration (`Config`)

Configuration is loaded from TOML via `Config::from_file` or `Config::from_str`. After deserialisation, `validate()` runs semantic checks:

- `store.path` must be non-empty
- `store.batch_size` must be greater than zero
- Every string in `[events]` must parse as a known `EsEventName`

### Sections

**`[events]`** — Three optional lists (`process`, `file`, `network`) of snake_case event names. `resolved_event_names()` flattens them in order (process → file → network) and deduplicates while preserving first occurrence.

**`[store]`** — LadybugDB settings:

| Field | Default | Meaning |
|-------|---------|---------|
| `path` | `data/events.lbug` | Database file path |
| `batch_size` | `500` | Max events per writer flush |
| `flush_interval_ms` | `1000` | Max time between flushes |

When running under `sudo`, avoid `~` in `store.path` — it expands to `/var/root`. Use an absolute path such as `/opt/esgraph/data/events.lbug`.

**`[mute]`** — Path prefixes passed to ESF `es_mute_path` before subscribe (see [ESF collector](esf.md)).

## Event names (`EsEventName`)

Apple defines event types as the C enum `es_event_type_t`. esgraph uses human-readable snake_case strings in TOML (`notify_exec`) and maps them in `EsEventName`. The macOS-specific `es_event_type_t` conversion lives in `esgraph-esf`.

### NOTIFY vs AUTH

| Class | Behaviour | Default in config |
|-------|----------|-------------------|
| **NOTIFY** | Async telemetry; the kernel does not wait for a response | Yes |
| **AUTH** | Synchronous gate; the client must respond before a deadline or macOS may kill the client | Opt-in only |

AUTH types in the registry: `auth_exec`, `auth_open`. The default TOML sets use NOTIFY-only names.

### Network events

ESF does not expose general TCP/UDP flow events. The `network` group maps to **UIPC** (UNIX domain socket) `notify_uipc_bind` and `notify_uipc_connect` only.

### Supported names

```
notify_exec, notify_fork, notify_exit, notify_remote_thread_create, notify_get_task,
auth_exec, notify_create, notify_write, notify_unlink, notify_rename,
notify_open, notify_close, auth_open, notify_uipc_bind, notify_uipc_connect
```

`notify_open` is omitted from defaults because it generates very high volume on desktop systems.

## Graph model

ESF delivers `es_message_t` structures. Before storage, messages become a `NormalisedEvent` containing nodes to upsert and edges to append.

### Process identity

`ProcessNode` is keyed by `ProcessIdentity::audit_token_hex`, not PID. The kernel reuses PIDs after exit; `audit_token_t` is unique for one process instance and appears on every `es_process_t` in ESF messages.

Fields captured on process nodes:

- `audit_token_hex` — primary graph key (hex-encoded audit token)
- `pid`, `ppid` — informational at event time
- `path` — executable path
- `signing_id`, `team_id`, `cdhash` — code signing metadata for reputation hunting
- `euid`, `egid`, `ruid`, `rgid` — user/group IDs from the audit token
- `session_id`, `is_platform_binary`, `parent_audit_token_hex` — session and lineage context
- `args_json` — full argv on `notify_exec` (JSON array string; preserved across later upserts)
- `exit_status` — last known exit code on `notify_exit`

File nodes also capture `inode`, `mode`, `owner_uid`, `owner_gid`, and `path_truncated` from `stat` at event time.

### Event details (`EventDetails`)

Each `NormalisedEvent` may include a `details` object with analyst-facing fields stored on `IngestEvent` nodes as `context_json`: instigator identity, `exec_args`, `exit_status`, and target file metadata. Timestamps appear on the event (`timestamp_unix_ns`), every relationship, and every `IngestEvent` node.

### Node types

| Type | Key (`id` property) | Source |
|------|----------------------|--------|
| `ProcessNode` | `audit_token_hex` | `es_process_t` |
| `FileNode` | `path` | `es_file_t` / path fields |
| `SocketNode` | `path` | UIPC bind/connect paths |

### Edge kinds (`EdgeKind`)

Edges are directed and timestamped (`timestamp_unix_ns`). Labels map to ESF semantics:

| `EdgeKind` | Typical ESF source |
|------------|-------------------|
| `EXECUTED` | `notify_exec` |
| `FORKED` | `notify_fork` |
| `EXITED` | `notify_exit` |
| `CREATED`, `WROTE`, `DELETED`, `RENAMED`, `OPENED`, `CLOSED` | File events |
| `BOUND`, `CONNECTED` | UIPC events |
| `GOT_TASK` | `notify_get_task` |
| `INJECTED_THREAD` | `notify_remote_thread_create` |

Rename edges store the destination path in `metadata` as JSON: `{"destination":"/new/path"}`.

### `NormalisedEvent`

```json
{
  "event_name": "notify_write",
  "timestamp_unix_ns": 1710000000000000000,
  "nodes": [ { "kind": "process", ... }, { "kind": "file", ... } ],
  "edges": [ { "kind": "WROTE", "src_id": "...", "dst_id": "/tmp/x", ... } ],
  "raw_json": null
}
```

Producers:

- `esgraph-esf` — live ESF normalisation
- `esgraphd replay` — JSON fixtures

Consumers:

- `esgraph-store` — LadybugDB ingest

## Errors

- `ConfigError::Read` / `Parse` — file I/O or TOML syntax
- `ConfigError::Validation` — unknown event name, empty path, zero batch size
