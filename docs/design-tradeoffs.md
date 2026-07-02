# Design trade-offs and production limitations

esgraph is a **research project**: it prioritises a simple, inspectable pipeline and a queryable graph over throughput, durability, and inline response. This page explains the approach taken for ESF subscription and LadybugDB ingest, why it is a poor fit for a production endpoint agent as-is, and how the architecture could evolve.

## The approach taken

### Subscribing to ESF events

The collector ([esgraph-esf](esf.md)) registers a single `endpoint_sec::Client` and subscribes to a **curated, config-driven** set of `notify_*` event types (`[events]` in TOML). Three deliberate choices keep volume manageable:

1. **NOTIFY-only telemetry.** AUTH events are answered `ALLOW` immediately and never gate anything — the client observes, it does not authorise.
2. **High-volume types are excluded by default.** `notify_open` / `notify_close` are omitted because they can generate thousands of events per second on a desktop system and overwhelm the ingest path.
3. **Kernel-side muting.** `[mute].paths` prefixes (plus the LadybugDB data directory, to avoid self-ingest feedback) are passed to `es_mute_path`, so noisy subtrees are filtered before the handler ever runs.

Each message is normalised **synchronously inside the ESF callback** into a `NormalisedEvent` (nodes + timestamped edges) and sent to the writer over a **bounded** `sync_channel` (capacity = `max(batch_size, 64)`).

### Writing to LadybugDB

A dedicated writer thread owns the embedded LadybugDB database (single-writer constraint) and batches events:

| Trigger | Default |
|---------|---------|
| Batch full (`batch_size`) | 500 events |
| Interval elapsed (`flush_interval_ms`) | 1000 ms |
| Shutdown | drain channel, final flush |

Each flush is one **multi-statement Cypher string**: per event, a `MERGE` for the `IngestEvent` audit node, `MERGE … ON CREATE / ON MATCH SET` upserts for `Process` / `File` / `Socket` nodes, and `MATCH` + `CREATE` for each typed relationship. Edges are **append-only**; nothing is ever pruned. Details: [store](store.md).

### Why this fits the research goal

- **Two threads, no async runtime** — easy to reason about and debug; the ES client is not `Send`, so the split falls out naturally.
- **Bounded channel with blocking backpressure** — memory use is capped without a drop policy to design.
- **Graph-native storage with Cypher** — hunt queries (multi-hop execution chains, staging-directory writes) are the point of the project, and an embedded `.lbug` file copies cleanly from VM to host for offline analysis in Ladybug Explorer.
- **Finite collection windows** — simulations run for minutes, so unbounded growth and long-run durability never bite.

## Drawbacks and limitations in production

### Blocking backpressure stalls the ESF callback

When the channel fills, `tx.send()` **blocks inside the ES message handler**. ESF delivery to the client stalls, and if AUTH subscriptions were ever enabled, blocked responses would freeze the affected operations system-wide and risk macOS killing the client on deadline. A production agent must never block the callback — it needs a non-blocking handoff with an explicit drop/degradation policy.

### The write path is the bottleneck

- **String-built multi-statement Cypher** is the slowest available ingest route: no prepared statements, no parameter binding, no bulk loader. A 500-event batch is one very large query string.
- **No intra-batch deduplication** — a process writing 200 files in one batch emits 200 near-identical `Process` MERGEs.
- **Edge inserts do endpoint lookups** (`MATCH` both nodes), so per-edge cost grows with graph size.
- **Single serialised writer**, with LadybugDB running on `SystemConfig::default()` — no tuning surface is exposed.

A realistic sustained ceiling is on the order of hundreds of events per second; desktop ESF firehose rates (with file open/close) exceed that by an order of magnitude.

### No durability between kernel and graph

Events live only in the in-memory channel and batch buffer until a flush commits. A crash loses up to `batch_size` events plus the channel contents, with no journal to replay.

### Unbounded growth, no retention

Append-only edges and the WAL grow without limit. There is no time-windowing, pruning, rotation, or archival — acceptable for a 10-minute simulation, not for an always-on agent.

### Purely passive: no detection or response path

The pipeline is collect-then-hunt. There is no rule evaluation during collection, no verdict mechanism, and the allow-all AUTH handling means the architecture cannot block anything. Cypher queries against a growing embedded graph take tens to hundreds of milliseconds with unpredictable tails — far outside AUTH deadline budgets, so inline blocking could never be bolted onto the current query path.

### Operational gaps

No metrics (queue depth, drop counts, flush latency), no watchdog or restart-on-kill, no config reload, no health reporting. Deployment assumes a lab VM: SIP disabled, ad-hoc signing, and no Apple-granted endpoint-security **distribution** entitlement — the latter is a real approval bar for shipping to machines you do not control.

## How this could be improved (high level)

Roughly in order of leverage:

1. **Non-blocking handoff with a drop policy.** Replace blocking `send` with `try_send` into a larger ring buffer; under pressure, drop lowest-value events first (file writes before execs), count and surface drops. Losing telemetry beats stalling the machine.
2. **Durable event log before the graph.** Append normalised events to a simple on-disk log; make LadybugDB ingest an asynchronous consumer. This decouples ESF from write latency, survives crashes, and allows re-ingest after schema changes.
3. **Faster ingest.** Prepared/parameterised statements or a bulk-load API instead of string-built Cypher; deduplicate node upserts within each batch.
4. **Retention and windowing.** Keep a bounded "hot" graph for recent activity; prune or rotate older data, archiving off-box if needed.
5. **Split AUTH and NOTIFY clients, add a verdict store.** For any future blocking capability: a minimal AUTH client answers from an in-memory verdict table (signing IDs, flagged audit tokens, path rules) in microseconds, while a detection engine runs scoped Cypher rules asynchronously against the graph and publishes verdicts back — containment and inoculation rather than inline graph queries.
6. **Operational hardening.** Metrics, watchdog, structured health output, and exposure of LadybugDB tuning knobs.

Items 1–4 harden the existing telemetry pipeline; item 5 is the architectural step that would turn a hunting tool into something resembling a response-capable agent. The graph model, schema, and Cypher rule corpus all carry over — it is the transport and write path that would be rebuilt.
