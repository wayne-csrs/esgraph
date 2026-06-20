# esgraph

A **research project** exploring how macOS [Endpoint Security Framework (ESF)](https://developer.apple.com/documentation/endpointsecurity) telemetry can be subscribed to, normalised, and ingested into a **graph-shaped database** for security analysis.

The goal is to turn live system activity — process execution, file access, UNIX socket binds — into queryable **nodes** and **edges** so you can hunt behaviour chains (discovery → staging → exfiltration, and similar) without standing up a full SIEM stack.

```
ESF events  →  normalise  →  LadybugDB (nodes + edges)  →  Cypher hunts
```

This is experimental tooling for isolated lab environments, not a production endpoint agent.

## What it does

1. **Subscribe** to a configurable set of ESF `notify_*` event types (exec, fork, file write, UIPC bind, and others).
2. **Normalise** each message into a small graph model: processes, files, and sockets as nodes; actions as timestamped edges.
3. **Ingest** batches into embedded **LadybugDB** (`Process`, `File`, `Socket` nodes; typed relationships).
4. **Query** with `esgraphd query` — Cypher hunt patterns, including multi-hop paths.

On the host you can develop and replay JSON fixtures without ESF. On a dedicated VM you can run the live collector (`esgraphd run`) with the Endpoint Security entitlement, root, and Full Disk Access.

## Recommended setup

Use a **two-machine workflow**: your everyday Mac as the **host**, and a **dedicated macOS VM** as the instrumented guest.

| | Host Mac | Dedicated VM |
|---|----------|--------------|
| Edit code, `cargo build`, unit tests | Yes | Optional |
| `esgraphd replay` / fixture ingest | Yes | Yes |
| Live `esgraphd run` (ESF subscription) | No | **Yes** |
| Ad-hoc codesign, SIP/AMFI trade-offs, FDA, `sudo` | Avoid | **Yes** |

**Why a VM?** Live ESF collection requires root, Full Disk Access, and a signed binary with `com.apple.developer.endpoint-security.client`. Research setups often also disable SIP on the guest. That combination is a poor fit for a daily-use machine but is reasonable on an isolated VM.

**Requirements:**

- macOS host + macOS VM (11+, 12+ preferred), **same CPU architecture** (`arm64` or `x86_64`)
- [Rust](https://rustup.rs), Xcode Command Line Tools, and `cmake` on the host (first build compiles LadybugDB from source; may take several minutes)
- SSH key access from host to VM — see [docs/vm-setup.md](docs/vm-setup.md)
- Optional passwordless `sudo` on the VM for automated simulations — see [docs/vm-setup.md](docs/vm-setup.md#25-optional-passwordless-sudo-for-automation)

Full checklist (SIP, entitlement, FDA, install layout): **[docs/vm-setup.md](docs/vm-setup.md)**.

## Quick start (host — no ESF)

Build and replay fixtures into a local LadybugDB file. No VM or entitlements required.

```bash
cargo build -p esgraphd

./target/debug/esgraphd replay --config config/default.toml fixtures/*.json
./target/debug/esgraphd status --config config/default.toml
./target/debug/esgraphd query --config config/default.toml \
  "MATCH (p:Process)-[r:WROTE]->(f:File) RETURN p.path, f.path LIMIT 20"
```

Graph path defaults to `data/events.lbug` in [`config/default.toml`](config/default.toml).

## Live collection on the VM

### 1. Configure SSH

```bash
cp config/vm.env.example config/vm.env
# Edit ESGRAPH_VM_HOST, ESGRAPH_VM_USER, ESGRAPH_INSTALL_PATH
```

### 2. Prepare the VM

Work through [docs/vm-setup.md](docs/vm-setup.md): SSH, deploy/sign, Full Disk Access, passwordless sudo (for automation).

### 3. Deploy from the host

```bash
./scripts/deploy-vm.sh
```

Installs a codesigned `esgraphd` to `/opt/esgraph/` on the VM.

### 4. Run the collector

On the VM (or via simulation script below):

```bash
sudo /opt/esgraph/esgraphd run --config /opt/esgraph/config/default.toml
```

Stop with **Ctrl+C** — the writer flushes pending events before exit.

`esgraphd status` only **reads** the graph; it is not the live collector. To check whether `run` is active: `ps -ax | grep '[/]opt/esgraph/esgraphd run'`.

### 5. Debug

```bash
./scripts/debug-vm.sh          # lldb on the VM over SSH
./scripts/stop-vm-collector.sh # stop a stuck esgraphd run
```

## Attack simulations

Non-destructive threat-actor scenarios under [`scripts/attack-scenarios/`](scripts/attack-scenarios/) drive lab behaviour on the VM while the collector runs. Each scenario has an execution script and a cleanup script.

```bash
./scripts/simulate-vm.sh --list-scenarios
./scripts/simulate-vm.sh --scenario apt29/apt29_discovery
```

This orchestrates: start collector → run scenario → stop collector → copy LadybugDB database and logs to `artefacts/simulations/` on the host → cleanup VM staging. Details: [docs/deployment.md](docs/deployment.md).

## Project layout

```
esgraph/
├── crates/
│   ├── esgraph-core/   # config, event names, graph model
│   ├── esgraph-store/  # LadybugDB schema and ingest
│   ├── esgraph-esf/    # live ESF client (macOS)
│   └── esgraphd/       # CLI: replay, query, status, run
├── scripts/            # deploy, debug, simulate, stop collector
├── config/             # default.toml (host), vm.default.toml (VM)
├── fixtures/           # JSON replay samples
└── docs/               # architecture and setup guides
```

## Documentation

| Document | Description |
|----------|-------------|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System overview and crate index |
| [docs/core.md](docs/core.md) | Config, events, graph model |
| [docs/store.md](docs/store.md) | LadybugDB schema, ingest, Ladybug Explorer |
| [docs/esf.md](docs/esf.md) | ESF collector and normalisation |
| [docs/cli.md](docs/cli.md) | `esgraphd` commands |
| [docs/config.md](docs/config.md) | TOML configuration |
| [docs/vm-setup.md](docs/vm-setup.md) | VM rationale, checklist, SSH, sudo |
| [docs/deployment.md](docs/deployment.md) | Deploy, debug, and simulation scripts |

## Tests

```bash
cargo test --workspace
```

## Licence and scope

Use only on systems you own or are authorised to instrument. Attack scenarios are for **isolated research VMs** — do not run them against production hosts.
