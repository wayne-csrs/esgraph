# esgraphd CLI

`esgraphd` is the command-line entry point. It loads TOML config, opens LadybugDB via `esgraph-store`, and exposes four subcommands.

## Global flags

| Flag | Default | Purpose |
|------|---------|---------|
| `--config` / `-c` | `config/default.toml` | TOML configuration path |

Logging uses `tracing` with filter `esgraph=info` unless `RUST_LOG` is set.

## `replay`

Ingest JSON fixtures without live ESF — useful on any host that builds LadybugDB bindings.

```bash
esgraphd replay --config config/default.toml fixtures/*.json
```

### Fixture format

A file may contain:

- A single `NormalisedEvent` JSON object
- An array of `NormalisedEvent` objects

Field names follow `esgraph-core` serde conventions (`event_name: "notify_write"`, `nodes`, `edges`, etc.). See [`fixtures/`](../fixtures/).

Loads all files in order, opens the graph, runs one `ingest` batch, prints stats.

## `query`

Run Cypher and print tab-separated output.

```bash
esgraphd query --config config/default.toml \
  "MATCH (p:Process)-[r:WROTE]->(f:File) RETURN p.path, f.path LIMIT 20"
```

Output: header row, data rows, then `(N rows)`.

## `status`

Print graph path, node counts per label, relationship counts, and configured ESF subscription names.

```bash
esgraphd status --config config/default.toml
```

## `run` (live ESF)

Subscribe to ESF and stream events into LadybugDB until Ctrl+C.

```bash
sudo esgraphd run --config /opt/esgraph/config/default.toml
```

Requirements: macOS, root, entitlement, Full Disk Access (see [VM setup](vm-setup.md)).

### Threading model

```
Main thread                          Writer thread
────────────                         ─────────────
cmd_run()
  shutdown = AtomicBool
  ctrlc → shutdown = true
  sync_channel(capacity = max(batch_size, 64))
  spawn_writer(rx, shutdown)  ──────► writer_loop()
  run_collector(tx, shutdown)            GraphStore::from_config
    ES Client handler                    recv_timeout loop
    normalise → tx.send()                batch → ingest → flush
  join writer
  print stats
```

**Backpressure** — the channel is bounded. If the writer falls behind, the ES handler blocks on `send` until space is available.

**Shutdown** — Ctrl+C sets `shutdown`. The collector exits its loop and drops the client. The writer drains remaining events with `try_recv`, flushes the final batch, and joins.

### Writer batching (`writer.rs`)

| Trigger | Action |
|---------|--------|
| `batch.len() >= batch_size` | Flush to LadybugDB |
| `flush_interval_ms` elapsed with pending events | Flush |
| `shutdown` and channel empty | Final flush, exit |

`IngestStats` from the writer thread is printed on exit.

## Source layout

| File | Role |
|------|------|
| `main.rs` | Clap CLI, subcommand dispatch |
| `replay.rs` | JSON fixture loader |
| `writer.rs` | Background LadybugDB writer for `run` |
