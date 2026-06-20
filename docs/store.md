# esgraph-store

`esgraph-store` persists `[NormalisedEvent](../crates/esgraph-core/src/model.rs)` values in embedded [LadybugDB](https://ladybugdb.com/) via the Rust `[lbug](https://crates.io/crates/lbug)` crate. The design is **graph-native**: labelled nodes (`Process`, `File`, `Socket`, `IngestEvent`) and typed relationships (`EXECUTED`, `WROTE`, …). Hunt queries use Cypher, including multi-hop patterns.

## `GraphStore`

`GraphStore` owns an `lbug::Database` handle opened on a **database file** path (e.g. `data/events.lbug`). Ladybug also writes a sibling `events.lbug.wal` write-ahead log while the database is in use.

Ladybug allows one writer at a time; the live pipeline owns one writer thread. CLI `query` / `status` open separate read connections.

### Opening a database

```rust
GraphStore::open("data/events.lbug", GraphStoreOptions::default())?;
GraphStore::from_config(&config.store, GraphStoreOptions::default())?;
```

On open:

1. Parent directories for `store.path` are created if needed
2. Node and relationship tables are created idempotently (`CREATE NODE TABLE IF NOT EXISTS …`, `CREATE REL TABLE IF NOT EXISTS …`)
3. The next `IngestEvent` id is loaded from the graph (`max(id) + 1`)

## Graph schema

Defined in `src/schema.rs` as Ladybug DDL.

### Node labels


| Label         | `id` property     | Notes                                                                                       |
| ------------- | ----------------- | ------------------------------------------------------------------------------------------- |
| `Process`     | `audit_token_hex` | `pid`, `path`, signing fields, UIDs, `args_json`, `exit_status`, `last_seen_unix_ns`        |
| `File`        | path string       | `inode`, `mode`, `owner_uid`, `owner_gid`, `path_truncated`                                 |
| `Socket`      | path string       | UNIX socket path                                                                            |
| `IngestEvent` | numeric `id`      | Audit row per ingested event; `event_name`, `timestamp_unix_ns`, `context_json`, `raw_json` |


### Relationship types

Mapped from `[EdgeKind](../crates/esgraph-core/src/model.rs)`: `EXECUTED`, `FORKED`, `EXITED`, `WROTE`, `CREATED`, `UNLINKED`, `RENAMED`, `UIPC_BOUND`, `UIPC_CONNECTED`, etc.

Relationship properties: `timestamp_unix_ns`, `event_name`, `ingest_event_id`, optional `metadata`.

## Ingest pipeline (`ingest_batch`)

Each batch runs as one multi-statement Cypher write:

```
for each NormalisedEvent:
  MERGE IngestEvent node
  MERGE Process / File / Socket nodes (ON CREATE / ON MATCH SET)
  MATCH endpoints, CREATE typed relationship edges
```

`MERGE` upserts match on `id`. `ON MATCH SET` only updates properties present in the current event so `args_json` from a prior `notify_exec` is preserved on later non-exec upserts.

`IngestStats` returns counts: `events`, `node_upserts`, `edges_inserted`.

### Upsert behaviour

- **Process** — matched on `id` (`audit_token_hex`); `args_json` is only set when the event supplies it
- **File / socket** — matched on path `id`

Empty `audit_token_hex` on a process node is rejected with `StoreError::InvalidEvent`.

## Querying

### `query_tabular`

Used by `esgraphd query`. Runs a Cypher `RETURN` query and stringifies cell values for tab-separated CLI output.

### Example hunt queries

Process → file writes:

```cypher
MATCH (p:Process)-[r:WROTE]->(f:File)
RETURN p.path, f.path
ORDER BY r.timestamp_unix_ns DESC
LIMIT 20
```

Multi-hop execution chain:

```cypher
MATCH path = (root:Process)-[:EXECUTED*1..3]->(leaf:Process)-[:WROTE]->(f:File)
RETURN root.path, leaf.path, f.path
LIMIT 50
```

Also available as `EXAMPLE_HUNT_CYPHER` and `EXAMPLE_EXEC_HUNT_CYPHER` in `schema.rs`.

### `count_label` / `count_relationship`

Cypher `MATCH … RETURN count(…)` helpers used by `esgraphd status`.

## Visualisation (Ladybug Explorer)

[Ladybug Explorer](https://github.com/LadybugDB/explorer) is the browser UI for LadybugDB — open `.lbug` files directly, no Bolt server required.

1. Build or replay a graph: `data/events.lbug`, `artefacts/simulations/latest/events.lbug` (updated after each VM simulation), or a specific run under `artefacts/simulations/<run>/events.lbug`
2. Launch Explorer (Docker) against that directory:

```bash
docker run -p 8000:8000 \
  -v "$(pwd)/artefacts/simulations/latest:/database" \
  -e LBUG_FILE=events.lbug \
  -e MODE=READ_ONLY \
  --rm ghcr.io/ladybugdb/explorer:latest
```

For local replay data, mount `data/` instead. For a specific simulation run, mount that run directory (e.g. `artefacts/simulations/<run-id>-<scenario>/`) and keep `LBUG_FILE=events.lbug`.

3. Open [http://localhost:8000](http://localhost:8000) and run Cypher hunt queries in the shell

Use `MODE=READ_ONLY` while inspecting graphs produced by esgraph; do not open the database for writing in Explorer while `esgraphd run` is active (single-writer). See the [Explorer README](https://github.com/LadybugDB/explorer) for SSH remote mode, WASM, and other launch options.

### Process identity in the UI

`Process.id` is the hex-encoded `audit_token_t` for that process instance — **not** PID. Relationship endpoints (`EXECUTED`, `FORKED`, …) are keyed on `Process.id`. Use `context_json` on `IngestEvent` nodes for analyst fields (`instigator_audit_token_hex`, `target_audit_token_hex`, `exec_args`, …).

### Exec-chain views (simulation runs)

Simulation captures many `WROTE` edges alongside `EXECUTED`. To focus on process execution chains in Ladybug Explorer:

```cypher
MATCH path = (root:Process)-[:EXECUTED*1..5]->(leaf:Process)
RETURN path
LIMIT 50
```

Parent → child execution with paths and argv:

```cypher
MATCH (parent:Process)-[r:EXECUTED]->(child:Process)
RETURN parent.id, parent.path, child.id, child.path, child.args_json, r.timestamp_unix_ns
ORDER BY r.timestamp_unix_ns
LIMIT 100
```

Filter to a simulation staging directory (paths include the run id):

```cypher
MATCH (p:Process)-[:EXECUTED*1..5]->(leaf:Process)-[:WROTE]->(f:File)
WHERE f.path CONTAINS '/tmp/apt29_chain_'
RETURN p.path, leaf.path, f.path
LIMIT 50
```

## Errors (`StoreError`)

- LadybugDB driver errors (`lbug::Error`)
- Filesystem I/O when creating database parent directories
- `InvalidEvent` — validation failures (missing process id, etc.)

## Build note

The `lbug` crate (v0.15.4) compiles Ladybug from source on first build (requires `cmake` on `PATH`, e.g. Homebrew). Rust ≥ 1.81 is required.