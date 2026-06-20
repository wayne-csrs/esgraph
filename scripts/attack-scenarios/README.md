# Attack scenarios

Non-destructive command sequences for [`simulate-vm.sh`](../simulate-vm.sh). Each scenario is a pair of scripts that emulate behaviours associated with published threat groups so ESF events can be captured on the research VM.

**Do not run these against production systems.** They are intended for isolated VMs only.

## Usage

```bash
./scripts/simulate-vm.sh --list-scenarios
./scripts/simulate-vm.sh --scenario apt29/apt29_discovery
./scripts/simulate-vm.sh --scenario amos/amos_stealer_chain
```

Scenarios that declare `# host-http: yes` in their execution script automatically start [`scripts/scenario-http-server.sh`](../scenario-http-server.sh) on the host. The server exposes benign payloads (e.g. `/curl/stage1`, `/frozenfix/update`) and captures exfil uploads under `host-http/uploads/` in the run artefacts directory. Set `ESGRAPH_HOST_IP` in `config/vm.env` if the VM cannot reach the auto-detected host address (common with NAT guests).

Use `--no-host-http` to skip the server, or `--host-http` to force it for ad-hoc commands.

You can also run the server manually:

```bash
./scripts/scenario-http-server.sh start --root /tmp/esgraph-host-http --port 8765
./scripts/scenario-http-server.sh stop --root /tmp/esgraph-host-http
```

`simulate-vm.sh` runs the **execution** script during collection, copies the LadybugDB database to the host, then runs the **cleanup** script on the VM. Cleanup also runs if the attack fails or the simulation is interrupted.

## Scenarios

| Name | Emulated focus | MITRE ATT&CK |
|------|----------------|--------------|
| [apt29/apt29_discovery](apt29/apt29_discovery.scenario.sh) | Host and account discovery | [G0016 APT29](https://attack.mitre.org/groups/G0016/) — T1033, T1057, T1082, T1083 |
| [wizard-spider/wizard_spider_staging](wizard-spider/wizard_spider_staging.scenario.sh) | Local staging and archive | [G0102 Wizard Spider](https://attack.mitre.org/groups/G0102/) — T1083, T1074.001, T1560.001 |
| [lazarus/lazarus_collection](lazarus/lazarus_collection.scenario.sh) | Local data collection | [G0032 Lazarus Group](https://attack.mitre.org/groups/G0032/) — T1005, T1083, T1074.001 |
| [oilrig/oilrig_uipc_probe](oilrig/oilrig_uipc_probe.scenario.sh) | Process probe + UIPC bind/connect | [G0049 OilRig](https://attack.mitre.org/groups/G0049/) — T1057, T1082, UIPC (macOS ESF) |
| [amos/amos_stealer_chain](amos/amos_stealer_chain.scenario.sh) | AMOS-like stealer chain (VM checks, browser/notes staging, archive, exfil) | [S1048 AMOS](https://attack.mitre.org/software/S1048/) — T1497.001, T1555.003, T1041, T1543.001 |

Each scenario has two files:

| File | Purpose |
|------|---------|
| `<name>.scenario.sh` | Execution — runs while `esgraphd` is collecting |
| `<name>.cleanup.sh` | Cleanup — removes VM artefacts after the database is copied |

Execution scripts write under `/tmp/..._${ESGRAPH_RUN_ID}`. Cleanup scripts remove those paths using the same `ESGRAPH_RUN_ID` set by `simulate-vm.sh`.

## Adding a scenario

1. Create `scripts/attack-scenarios/<actor>/<name>.scenario.sh` (execution)
2. Create `scripts/attack-scenarios/<actor>/<name>.cleanup.sh` (cleanup)
3. Add metadata comment lines at the top of the execution script (`# description:`, `# actor:`, `# mitre:`, `# reference:`)
4. List paths removed by the cleanup script in `# removes:` comments (comma-separated absolute paths; `${ESGRAPH_RUN_ID}` is expanded). `simulate-vm.sh` verifies these no longer exist after cleanup.
5. If the scenario downloads payloads or uploads to a host C2 emulator, add `# host-http: yes` and use `${ESGRAPH_HOST_HTTP}` in curl commands
6. Use `${ESGRAPH_RUN_ID}` in temp paths so cleanup can target the same directories
7. Run `./scripts/simulate-vm.sh --list-scenarios` to verify discovery (`--scenario` names use `<actor>/<name>`)

## References

- [MITRE ATT&CK](https://attack.mitre.org/)
- [APT29 — MITRE G0016](https://attack.mitre.org/groups/G0016/)
- [Wizard Spider — MITRE G0102](https://attack.mitre.org/groups/G0102/)
- [Lazarus Group — MITRE G0032](https://attack.mitre.org/groups/G0032/)
- [OilRig — MITRE G0049](https://attack.mitre.org/groups/G0049/)
- [AMOS Stealer — Objective-See blog 0x88](https://objective-see.org/blog/blog_0x88.html)
- [AMOS — MITRE S1048](https://attack.mitre.org/software/S1048/)
- [Apple Endpoint Security event types](https://developer.apple.com/documentation/endpointsecurity/es_event_type_t)
