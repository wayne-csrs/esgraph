# Deployment

Host-to-VM deployment is handled by shell scripts in [`scripts/`](../scripts/). Configuration for SSH targets lives in `config/vm.env` (gitignored).

## `config/vm.env`

Copy from [`config/vm.env.example`](../config/vm.env.example):

```bash
ESGRAPH_VM_HOST=192.168.1.100
ESGRAPH_VM_USER=esgraph
ESGRAPH_INSTALL_PATH=/opt/esgraph
```

Scripts source this file and require `ESGRAPH_VM_HOST` and `ESGRAPH_VM_USER`.

## `deploy-vm.sh`

Build on the host, rsync to the VM, ad-hoc sign with ESF entitlement.

```bash
./scripts/deploy-vm.sh           # debug build (recommended for lldb)
./scripts/deploy-vm.sh --release   # release build
```

### Steps

1. `cargo build -p esgraphd` (debug or `--release`)
2. SSH: `sudo mkdir -p /opt/esgraph/{bin,config,data}` and `chown` to deploy user
3. `rsync` binary → `$ESGRAPH_INSTALL_PATH/esgraphd`
4. `rsync` `config/vm.default.toml` → `$ESGRAPH_INSTALL_PATH/config/default.toml`
5. `rsync` `esgraphd.entitlements`
6. On VM: `codesign --force --sign - --entitlements … --options runtime esgraphd`
7. Print `codesign -dv` snippet for verification

### Output paths

| Host | VM |
|------|-----|
| `target/debug/esgraphd` | `/opt/esgraph/esgraphd` |
| `config/vm.default.toml` | `/opt/esgraph/config/default.toml` |

## `debug-vm.sh`

Opens interactive lldb on the VM over SSH.

```bash
./scripts/debug-vm.sh
```

Runs: `ssh -t user@host sudo lldb /opt/esgraph/esgraphd`

### Typical lldb session

```
(lldb) run -- --config /opt/esgraph/config/default.toml
(lldb) process attach --name esgraphd
(lldb) bt
(lldb) thread list
```

Use a debug build from `deploy-vm.sh` (no `--release`) for symbols.

## Workflow

```
Host                          VM
────                          ──
edit source
cargo build
./scripts/deploy-vm.sh  ───►  codesigned esgraphd
./scripts/debug-vm.sh   ───►  lldb / sudo run
                              LadybugDB at /opt/esgraph/data/events.lbug
```

## Entitlements file

[`esgraphd.entitlements`](../esgraphd.entitlements) must be deployed alongside the binary for `codesign --entitlements`. See [VM setup](vm-setup.md).


## `simulate-vm.sh`

Run a complete capture cycle on the VM: start `esgraphd run`, execute an attack command, stop collection, copy artefacts back to host, run the scenario cleanup script, and remove copied collector files from the VM.

Named scenarios require both `<name>.scenario.sh` (execution) and `<name>.cleanup.sh` (cleanup). Cleanup runs after the database is copied to the host, and also if the attack fails or the simulation is interrupted.

```bash
./scripts/simulate-vm.sh --scenario apt29/apt29_discovery
./scripts/simulate-vm.sh -- touch /tmp/esgraph-test
```

### Scenario catalogue

Scenarios live in [`scripts/attack-scenarios/`](../scripts/attack-scenarios/). See the [scenario README](../scripts/attack-scenarios/README.md) for MITRE ATT&CK mappings and references.

```bash
./scripts/simulate-vm.sh --list-scenarios
```

### Options

| Option | Default | Meaning |
|--------|---------|---------|
| `--scenario NAME` | none | Run a named scenario from `scripts/attack-scenarios/` using `<actor>/<name>` |
| `--list-scenarios` | n/a | Print available scenarios and exit |
| `--warmup-sec N` | `3` | Wait before starting esgraphd (ESF off; lets the VM settle) |
| `--cooldown-sec N` | `3` | Wait after stopping esgraphd (ESF off; before status/archive) |
| `--rust-log FILTER` | `esgraph=info` | `RUST_LOG` value for collector run |
| `--config PATH` | `/opt/esgraph/config/default.toml` | Config path on VM |
| `--output-dir PATH` | `./artefacts/simulations` | Host folder for copied artefacts |
| `--host-http` | auto | Force-start the host HTTP server (also auto for `# host-http: yes` scenarios) |
| `--no-host-http` | off | Skip the host HTTP server even when the scenario declares `# host-http: yes` |
| `--host-http-port N` | `8765` | TCP port for the host HTTP server |

You can either pass `--scenario <actor>/<name>` or an explicit command after `--`.

### Host HTTP server

Scenarios with `# host-http: yes` in their execution script automatically start [`scripts/scenario-http-server.sh`](../scripts/scenario-http-server.sh) on the host. The VM receives `ESGRAPH_HOST_HTTP=http://<host-ip>:<port>` for payload download and exfil upload curls.

Captured uploads land in `<run-dir>/host-http/uploads/`. If the VM cannot reach the auto-detected host IP, set `ESGRAPH_HOST_IP` in `config/vm.env`.

### Copied artefacts

Each run creates a host folder like `artefacts/simulations/<timestamp>-<scenario>/` containing:

- `events.lbug.tar.gz` (and extracted `events.lbug` / `events.lbug.wal`)
- `esgraphd-run.log`
- `status.txt`
- `run-meta.txt`
- `attack-command.txt`

After each successful copy, the same `events.lbug` (and `.wal` if present) is also written to `artefacts/simulations/latest/` so Ladybug Explorer can use a fixed mount path. `latest/source-run.txt` points at the timestamped run folder.

After successful copy, the script runs the scenario cleanup script (removes `/tmp` staging from the attack), then removes the corresponding log/status/database files from the VM.

## `stop-vm-collector.sh`

Stop a stuck or leftover `esgraphd run` process on the VM.

```bash
./scripts/stop-vm-collector.sh
```

`esgraphd status` only reads the graph directory and exits — it does not mean the live collector is running. Use `ps -ax | grep '[/]opt/esgraph/esgraphd run'` on the VM to check.
